//! A submitted operation owns its reply and its original bounded deadline.
//! Void batches may overlap caller work; every observation consumes their reply.
use super::*;
use std::time::Instant;

pub(super) struct Submitted {
    incoming: mpsc::Receiver<Reply>,
    deadline: Instant,
}

pub(super) fn submit(operation: Operation) -> Option<Submitted> {
    let owner = owner()?;
    let (reply, incoming) = mpsc::sync_channel(1);
    let mut pending = Request { operation, reply };
    let deadline = Instant::now() + RESPONSE_TIMEOUT;
    loop {
        match owner.try_send(pending) {
            Ok(()) => return Some(Submitted { incoming, deadline }),
            Err(mpsc::TrySendError::Disconnected(_)) => return None,
            Err(mpsc::TrySendError::Full(value)) => {
                if Instant::now() >= deadline {
                    return None;
                }
                pending = value;
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
}

impl Submitted {
    #[cfg(test)]
    pub(super) fn test_pending(deadline: Instant) -> (mpsc::SyncSender<Reply>, Self) {
        let (sender, incoming) = mpsc::sync_channel(1);
        (sender, Self { incoming, deadline })
    }

    pub(super) fn wait(self) -> Option<Reply> {
        self.incoming
            .recv_timeout(self.deadline.saturating_duration_since(Instant::now()))
            .ok()
    }

    /// None means still pending, Some(None) means unavailable/timed out.
    /// A caller must not reset the deadline each time it checks a batch.
    pub(super) fn poll(&self) -> Option<Option<Reply>> {
        match self.incoming.try_recv() {
            Ok(reply) => Some(Some(reply)),
            Err(mpsc::TryRecvError::Disconnected) => Some(None),
            Err(mpsc::TryRecvError::Empty) if Instant::now() >= self.deadline => Some(None),
            Err(mpsc::TryRecvError::Empty) => None,
        }
    }
}

#[cfg(test)]
mod tests;
