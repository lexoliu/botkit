//! `botkit-cli` — the driver for bots running on `botkit_cli::Transport::Unix`.
//!
//! Every command is one socket round-trip: write the event, read the
//! ack/error, exit. `tail` stays connected and streams outbound actions as
//! JSONL until killed — `timeout 5 botkit-cli --sock s tail` is the intended
//! way to capture a window of bot output.
//!
//! Usage:
//!   botkit-cli --sock PATH tail
//!   botkit-cli --sock PATH message --chat C --user U [--name N] [--text T]
//!       [--caption T] [--message-id N] [--thread N] [--reply-to N]
//!       [--file KIND:PATH]... [--sticker FILE_ID]
//!   botkit-cli --sock PATH command --chat C --user U --name NAME [--args A]
//!   botkit-cli --sock PATH button  --chat C --user U --data D [--message-id N]
//!   botkit-cli --sock PATH react   --chat C --user U --message-id N [--add E]... [--remove E]...
//!   botkit-cli --sock PATH edit    --chat C --user U --message-id N --text T

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::ExitCode;

use botkit_cli::wire::{
    Inbound, InboundButton, InboundCommand, InboundEdited, InboundMessage, InboundReaction,
    WireFile, WireSticker, WireUser,
};

const USAGE: &str = "\
botkit-cli — drive a botkit-cli bot over its unix socket

  --sock PATH tail                                        stream outbound JSONL
  --sock PATH message --chat C --user U [opts]            inject a message
  --sock PATH command --chat C --user U --name N [--args A]
  --sock PATH button  --chat C --user U --data D [--message-id N]
  --sock PATH react   --chat C --user U --message-id N [--add E]... [--remove E]...
  --sock PATH edit    --chat C --user U --message-id N --text T

message opts: --name N --text T --caption T --message-id N --thread N
              --reply-to N --file KIND:PATH (repeatable) --sticker FILE_ID";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut parser = Args::new(&args);

    let Some(sock) = parser.value("--sock").map(PathBuf::from) else {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    };
    let Some(command) = parser.next_positional() else {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    };

    match command.as_str() {
        "tail" => tail(&sock),
        other => match build_event(other, &mut parser) {
            Ok(event) => inject(&sock, &event),
            Err(message) => {
                eprintln!("{message}\n{USAGE}");
                ExitCode::FAILURE
            }
        },
    }
}

/// Open with `{"type":"subscribe"}` and copy the stream to stdout.
fn tail(sock: &PathBuf) -> ExitCode {
    let Ok(mut stream) = UnixStream::connect(sock) else {
        eprintln!("cannot connect to {}", sock.display());
        return ExitCode::FAILURE;
    };
    if writeln!(stream, r#"{{"type":"subscribe"}}"#).is_err() {
        eprintln!("subscribe failed");
        return ExitCode::FAILURE;
    }
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        match line {
            Ok(line) => println!("{line}"),
            Err(_) => break,
        }
    }
    ExitCode::SUCCESS
}

/// Send one event, print the one-line response, exit.
fn inject(sock: &PathBuf, event: &Inbound) -> ExitCode {
    let Ok(mut stream) = UnixStream::connect(sock) else {
        eprintln!("cannot connect to {}", sock.display());
        return ExitCode::FAILURE;
    };
    let Ok(line) = serde_json::to_string(event) else {
        eprintln!("failed to serialize event");
        return ExitCode::FAILURE;
    };
    if writeln!(stream, "{line}").is_err() {
        eprintln!("write failed");
        return ExitCode::FAILURE;
    }
    let mut response = String::new();
    if BufReader::new(&mut stream)
        .read_line(&mut response)
        .is_err()
        || response.is_empty()
    {
        eprintln!("no response");
        return ExitCode::FAILURE;
    }
    print!("{response}");
    if response.contains(r#""type":"error""#) {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// Build the `Inbound` for an inject subcommand.
fn build_event(command: &str, args: &mut Args) -> Result<Inbound, String> {
    let chat = args.value("--chat").ok_or("--chat is required")?;
    let user = WireUser {
        id: args.value("--user").ok_or("--user is required")?,
        name: args
            .value("--name")
            .unwrap_or_else(|| args.value("--user").unwrap()),
    };

    match command {
        "message" | "send" => Ok(Inbound::Message(InboundMessage {
            chat,
            user,
            message_id: args.i64("--message-id"),
            text: args.opt_value("--text"),
            caption: args.opt_value("--caption"),
            thread_id: args.i64("--thread"),
            reply_to: args
                .i64("--reply-to")
                .map(|message_id| botkit_cli::wire::WireReplyRef {
                    message_id,
                    from: None,
                    text: None,
                }),
            files: args
                .values("--file")
                .into_iter()
                .map(|spec| {
                    let (kind, path) = spec
                        .split_once(':')
                        .ok_or("--file expects KIND:PATH".to_string())?;
                    Ok(WireFile {
                        kind: kind.to_string(),
                        path: path.to_string(),
                        mime: None,
                        file_id: None,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?,
            sticker: args.opt_value("--sticker").map(|file_id| WireSticker {
                file_id,
                emoji: args.opt_value("--emoji"),
                set_name: args.opt_value("--set"),
                format: args
                    .opt_value("--format")
                    .unwrap_or_else(|| "static".to_string()),
                path: args.opt_value("--sticker-path"),
            }),
            ambient: args
                .opt_value("--ambient")
                .is_some_and(|v| !matches!(v.as_str(), "false" | "0" | "no")),
        })),
        "command" => Ok(Inbound::Command(InboundCommand {
            chat,
            user,
            name: args.value("--name").ok_or("--name is required")?,
            args: args.opt_value("--args").unwrap_or_default(),
            message_id: args.i64("--message-id"),
            thread_id: args.i64("--thread"),
        })),
        "button" => Ok(Inbound::Button(InboundButton {
            chat,
            user,
            data: args.value("--data").ok_or("--data is required")?,
            message_id: args.i64("--message-id"),
            message_text: args.opt_value("--message-text"),
            thread_id: args.i64("--thread"),
        })),
        "react" => Ok(Inbound::Reaction(InboundReaction {
            chat,
            user,
            message_id: args.i64("--message-id").ok_or("--message-id is required")?,
            added: args.values("--add"),
            removed: args.values("--remove"),
        })),
        "edit" => Ok(Inbound::Edited(InboundEdited {
            chat,
            user,
            message_id: args.i64("--message-id").ok_or("--message-id is required")?,
            text: args.opt_value("--text"),
            thread_id: args.i64("--thread"),
        })),
        other => Err(format!("unknown command {other:?}")),
    }
}

/// Dead-simple `--flag value` parser: positional words in order, named
/// values on demand. Flags may repeat (`values` collects all).
struct Args<'a> {
    all: &'a [String],
}

impl<'a> Args<'a> {
    fn new(all: &'a [String]) -> Self {
        Self { all }
    }

    /// The first word that is not a flag or a flag's value.
    fn next_positional(&self) -> Option<String> {
        let mut iter = self.all.iter();
        while let Some(arg) = iter.next() {
            if arg.starts_with("--") {
                iter.next(); // the flag's value
            } else {
                return Some(arg.clone());
            }
        }
        None
    }

    /// All values for a repeated flag.
    fn values(&self, flag: &str) -> Vec<String> {
        let mut found = Vec::new();
        let mut iter = self.all.iter();
        while let Some(arg) = iter.next() {
            if arg == flag
                && let Some(value) = iter.next()
            {
                found.push(value.clone());
            }
        }
        found
    }

    /// First value for a flag; empty string when absent.
    fn value(&self, flag: &str) -> Option<String> {
        self.values(flag).into_iter().next()
    }

    /// Optional value (None when the flag is absent).
    fn opt_value(&self, flag: &str) -> Option<String> {
        self.value(flag)
    }

    /// Optional integer flag.
    fn i64(&self, flag: &str) -> Option<i64> {
        self.value(flag).and_then(|v| v.parse().ok())
    }
}
