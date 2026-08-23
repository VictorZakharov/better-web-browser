//! Lost-wakeup-safe broker notification for bounded in-process queues.

use std::sync::{Arc, OnceLock};
use std::thread::Thread;
use std::time::Duration;

#[derive(Clone, Default)]
pub(super) struct BrokerWake {
    broker: Arc<OnceLock<Thread>>,
}

impl BrokerWake {
    pub(super) fn register_current(&self) {
        let _ = self.broker.set(std::thread::current());
    }

    pub(super) fn notify(&self) {
        if let Some(broker) = self.broker.get() {
            broker.unpark();
        }
    }

    pub(super) fn wait(&self, timeout: Duration) {
        std::thread::park_timeout(timeout);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Instant;

    #[test]
    fn notification_releases_a_registered_waiter() {
        let wake = BrokerWake::default();
        let notifier = wake.clone();
        let (ready, receiver) = mpsc::channel();
        let waiter = std::thread::spawn(move || {
            wake.register_current();
            ready.send(()).unwrap();
            let started = Instant::now();
            wake.wait(Duration::from_secs(1));
            started.elapsed()
        });
        receiver.recv().unwrap();
        notifier.notify();
        assert!(waiter.join().unwrap() < Duration::from_millis(100));
    }
}
