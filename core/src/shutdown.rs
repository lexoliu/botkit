use std::future::pending;

/// Signals a running bot to stop
///
/// Cloneable and cheap; every clone signals the same bot. Dropping every clone
/// does *not* stop the bot — call [`ShutdownSignal::shutdown`] for that.
#[derive(Clone, Debug)]
pub struct ShutdownSignal {
    tx: async_channel::Sender<()>,
}

impl ShutdownSignal {
    /// Ask the bot to stop
    ///
    /// Returns immediately; the bot stops once it reaches its next await point.
    /// Calling this more than once is harmless.
    pub fn shutdown(&self) {
        self.tx.close();
    }

    /// Whether shutdown has already been requested
    pub fn is_shutdown(&self) -> bool {
        self.tx.is_closed()
    }
}

/// The receiving half of a shutdown signal, handed to [`crate::Bot::run_until`]
#[derive(Clone, Debug)]
pub struct Shutdown {
    rx: Option<async_channel::Receiver<()>>,
}

impl Shutdown {
    /// Create a linked signal/receiver pair
    pub fn channel() -> (ShutdownSignal, Self) {
        let (tx, rx) = async_channel::bounded(1);
        (ShutdownSignal { tx }, Self { rx: Some(rx) })
    }

    /// A shutdown that never fires, for bots meant to run until the process exits
    pub fn never() -> Self {
        Self { rx: None }
    }

    /// Whether shutdown has already been requested
    pub fn is_shutdown(&self) -> bool {
        self.rx.as_ref().is_some_and(|rx| rx.is_closed())
    }

    /// Resolve once shutdown is requested
    ///
    /// Never resolves for [`Shutdown::never`], so it is safe to select on
    /// unconditionally.
    pub async fn wait(&self) {
        match &self.rx {
            // `recv` resolves with `Err` as soon as the sender is closed.
            Some(rx) => while rx.recv().await.is_ok() {},
            None => pending().await,
        }
    }
}

impl Default for Shutdown {
    fn default() -> Self {
        Self::never()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_lite::future::{block_on, poll_once};

    #[test]
    fn wait_resolves_after_shutdown() {
        let (signal, shutdown) = Shutdown::channel();
        assert!(!shutdown.is_shutdown());
        assert!(block_on(poll_once(shutdown.wait())).is_none());

        signal.shutdown();

        assert!(signal.is_shutdown());
        assert!(shutdown.is_shutdown());
        assert!(block_on(poll_once(shutdown.wait())).is_some());
    }

    #[test]
    fn never_stays_pending() {
        let shutdown = Shutdown::never();
        assert!(!shutdown.is_shutdown());
        assert!(block_on(poll_once(shutdown.wait())).is_none());
    }

    #[test]
    fn shutdown_is_idempotent() {
        let (signal, shutdown) = Shutdown::channel();
        signal.shutdown();
        signal.clone().shutdown();
        assert!(block_on(poll_once(shutdown.wait())).is_some());
    }
}
