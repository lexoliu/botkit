//! How events reach the bot and outbound actions reach drivers.
//!
//! `Stdio` reads `Inbound` lines from stdin and prints `Outbound` lines to
//! stdout — the simplest mode for `cargo run` debugging.
//!
//! `Unix` binds a socket; each connection either streams inbound events
//! (each answered with an `ack`/`error` line) or opens with
//! `{"type":"subscribe"}` and streams every outbound line — the mode to use
//! against a long-running bot process.
//!
//! `Manual` attaches no transport at all: the owner drives the bot through
//! the [`CliHub`] handle directly (tests, in-process embedding).

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use futures_lite::io::{AsyncBufReadExt, AsyncWriteExt, BufReader as AsyncBufReader};

use crate::hub::CliHub;
use crate::wire::{Inbound, Outbound, OutboundAck, OutboundError};

/// How the CLI platform moves events and actions.
#[derive(Debug, Clone)]
pub enum Transport {
    /// Inbound events on stdin, outbound actions on stdout.
    Stdio,
    /// A unix socket at this path.
    #[cfg(unix)]
    Unix(PathBuf),
    /// No transport — the owner injects events and reads outbound actions
    /// through the [`CliHub`] handle.
    Manual,
}

/// Start the transport: push inbound events into the hub and wire outbound
/// sinks. Returns immediately; all work happens on spawned tasks/threads.
pub(crate) fn start(transport: &Transport, hub: &CliHub) {
    match transport {
        Transport::Stdio => start_stdio(hub.clone()),
        #[cfg(unix)]
        Transport::Unix(path) => start_unix(path.clone(), hub.clone()),
        Transport::Manual => {}
    }
}

fn start_stdio(hub: CliHub) {
    // stdout becomes the single implicit subscriber.
    let lines = hub.subscribe();
    executor_core::spawn(async move {
        let stdout = std::io::stdout();
        while let Ok(line) = lines.recv().await {
            let mut out = stdout.lock();
            let _ = writeln!(out, "{line}");
            let _ = out.flush();
        }
    })
    .detach();

    // stdin is blocking IO; read it on a dedicated thread off the executor.
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        for line in BufReader::new(stdin.lock()).lines() {
            let Ok(line) = line else { break };
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<Inbound>(&line) {
                Ok(event) => {
                    let message_id = hub.inject(event);
                    hub.emit(Outbound::Ack(OutboundAck {
                        message_id: Some(message_id),
                    }));
                }
                Err(error) => hub.emit(Outbound::Error(OutboundError {
                    message: error.to_string(),
                })),
            }
        }
    });
}

#[cfg(unix)]
fn start_unix(path: PathBuf, hub: CliHub) {
    use async_io::Async;
    use std::os::unix::net::UnixListener;

    let _ = std::fs::remove_file(&path);
    let listener = match UnixListener::bind(&path).and_then(Async::new) {
        Ok(listener) => listener,
        Err(error) => {
            tracing::error!(%error, path = %path.display(), "cli transport failed to bind");
            return;
        }
    };
    tracing::info!(path = %path.display(), "cli transport listening");

    executor_core::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let hub = hub.clone();
                    executor_core::spawn(handle_conn(stream, hub)).detach();
                }
                Err(error) => {
                    tracing::warn!(%error, "cli transport accept failed");
                }
            }
        }
    })
    .detach();
}

#[cfg(unix)]
async fn handle_conn(stream: async_io::Async<std::os::unix::net::UnixStream>, hub: CliHub) {
    use futures_lite::stream::StreamExt;
    let (reader, mut writer) = futures_lite::io::split(stream);
    let mut lines = AsyncBufReader::new(reader).lines();
    // A driver connection is persistent: every event line gets an `ack` (or
    // `error`) reply, so one connection can drive a whole session.
    while let Some(line) = lines.next().await {
        let Ok(line) = line else { return };
        let reply = match serde_json::from_str::<Inbound>(&line) {
            Ok(Inbound::Subscribe) => {
                // Switch roles: this connection becomes an outbound sink.
                let sink = hub.subscribe();
                while let Ok(line) = sink.recv().await {
                    if writer.write_all(line.as_bytes()).await.is_err()
                        || writer.write_all(b"\n").await.is_err()
                    {
                        return;
                    }
                }
                return;
            }
            Ok(event) => {
                let message_id = hub.inject(event);
                Outbound::Ack(OutboundAck {
                    message_id: Some(message_id),
                })
            }
            Err(error) => Outbound::Error(OutboundError {
                message: error.to_string(),
            }),
        };
        if let Ok(reply) = serde_json::to_string(&reply)
            && (writer.write_all(reply.as_bytes()).await.is_err()
                || writer.write_all(b"\n").await.is_err())
        {
            return;
        }
    }
}
