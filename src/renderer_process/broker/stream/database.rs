//! Required IndexedDB replies have a bounded, nonblocking broker lane.

use super::*;

impl DatabaseEventSink {
    pub(in crate::renderer_process::broker) fn new(
        document: DocumentId,
        sender: mpsc::SyncSender<DatabaseEvent>,
        queued_bytes: Arc<AtomicUsize>,
        overflow: Arc<AtomicBool>,
        wake: super::super::wake::BrokerWake,
    ) -> Self {
        Self {
            document,
            sender,
            queued_bytes,
            overflow,
            wake,
        }
    }

    /// A required response must not block the browser-wide database service.
    /// Overflow fails the renderer session instead of leaving a Promise pending.
    pub fn try_send(&self, event: DatabaseEvent) -> Result<(), String> {
        event.validate().map_err(|error| error.to_string())?;
        if event.document != self.document {
            return Err("IndexedDB event document mismatch".into());
        }
        let bytes = event.payload.len();
        if self
            .queued_bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current
                    .checked_add(bytes)
                    .filter(|next| *next <= MAX_QUEUED_DATABASE_RESPONSE_BYTES)
            })
            .is_err()
        {
            self.fail_overflow();
            return Err("renderer database response byte budget is full".into());
        }
        match self.sender.try_send(event) {
            Ok(()) => {}
            Err(mpsc::TrySendError::Full(_)) => {
                self.queued_bytes.fetch_sub(bytes, Ordering::AcqRel);
                self.fail_overflow();
                return Err("renderer database response mailbox is full".into());
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                self.queued_bytes.fetch_sub(bytes, Ordering::AcqRel);
                return Err("renderer database response mailbox is closed".into());
            }
        }
        self.wake.notify();
        Ok(())
    }

    fn fail_overflow(&self) {
        self.overflow.store(true, Ordering::Release);
        self.wake.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_reply_overflow_fails_closed_without_blocking_its_producer() {
        let document = DocumentId::new(7).unwrap();
        let (sender, receiver) = mpsc::sync_channel(1);
        let queued_bytes = Arc::new(AtomicUsize::new(0));
        let overflow = Arc::new(AtomicBool::new(false));
        let sink = DatabaseEventSink::new(
            document,
            sender,
            Arc::clone(&queued_bytes),
            Arc::clone(&overflow),
            super::super::super::wake::BrokerWake::default(),
        );
        let event = DatabaseEvent {
            document,
            request_id: 1,
            payload: r#"{"kind":"open"}"#.into(),
        };
        sink.try_send(event.clone()).unwrap();
        assert!(!overflow.load(Ordering::Acquire));
        assert!(sink.try_send(event).is_err());
        assert!(overflow.load(Ordering::Acquire));
        assert_eq!(
            queued_bytes.load(Ordering::Acquire),
            r#"{"kind":"open"}"#.len()
        );
        assert_eq!(receiver.try_recv().unwrap().request_id, 1);
    }

    #[test]
    fn maximum_sized_database_replies_cannot_exceed_aggregate_byte_budget() {
        let document = DocumentId::new(8).unwrap();
        let (sender, _receiver) = mpsc::sync_channel(8);
        let queued_bytes = Arc::new(AtomicUsize::new(0));
        let overflow = Arc::new(AtomicBool::new(false));
        let sink = DatabaseEventSink::new(
            document,
            sender,
            Arc::clone(&queued_bytes),
            Arc::clone(&overflow),
            super::super::super::wake::BrokerWake::default(),
        );
        for request_id in 1..=4 {
            sink.try_send(DatabaseEvent {
                document,
                request_id,
                payload: "x".repeat(crate::limits::MAX_INDEXED_DB_IPC_BYTES),
            })
            .unwrap();
        }
        assert_eq!(
            queued_bytes.load(Ordering::Acquire),
            MAX_QUEUED_DATABASE_RESPONSE_BYTES
        );
        assert!(
            sink.try_send(DatabaseEvent {
                document,
                request_id: 5,
                payload: "x".into(),
            })
            .is_err()
        );
        assert!(overflow.load(Ordering::Acquire));
    }
}
