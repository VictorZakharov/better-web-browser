//! Browser-owned BroadcastChannel membership for top-level documents.
//!
//! HTML §9.5 keys destinations by storage key and channel name. For a top-level
//! document the top-level site is determined by its own origin, so exact origin
//! matching also implies storage-key matching. Embedded realms need a separate
//! top-level-site authority before they can join this registry.

use crate::renderer_protocol::BroadcastDelivery;
use crate::renderer_protocol::{BroadcastCommand, BroadcastOperation, DocumentId};
use std::collections::{HashMap, VecDeque};

const MAX_CHANNELS_PER_DOCUMENT: usize = 128;
const MAX_PENDING_PER_TAB: usize = 256;
const MAX_PENDING_RECIPIENTS_PER_BROWSER: usize = 65_536;
const MAX_PENDING_BYTES_PER_BROWSER: usize = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Destination {
    pub tab: u64,
    pub document: DocumentId,
    pub channel_id: u64,
}

struct Channel {
    destination: Destination,
    origin: String,
    name: String,
}

#[derive(Default)]
pub struct BroadcastRegistry {
    channels: Vec<Channel>,
    pending: HashMap<u64, Pending>,
    batches: HashMap<u64, Batch>,
    next_batch_id: u64,
    pending_bytes: usize,
    pending_recipients: usize,
}

#[derive(Default)]
struct Pending {
    queue: VecDeque<QueuedDelivery>,
}

#[derive(Clone, Copy)]
struct QueuedDelivery {
    document: DocumentId,
    channel_id: u64,
    batch_id: u64,
}

struct Batch {
    origin: String,
    serialized: String,
    recipients: usize,
}

impl BroadcastRegistry {
    pub fn apply(
        &mut self,
        tab: u64,
        origin: &str,
        command: &BroadcastCommand,
    ) -> Result<bool, &'static str> {
        command
            .validate()
            .map_err(|_| "invalid BroadcastChannel command")?;
        let source = Destination {
            tab,
            document: command.document,
            channel_id: command.channel_id,
        };
        let position = self
            .channels
            .iter()
            .position(|channel| channel.destination == source);
        match &command.operation {
            BroadcastOperation::Open { name } => {
                if position.is_some() {
                    return Err("duplicate BroadcastChannel identifier");
                }
                if self
                    .channels
                    .iter()
                    .filter(|channel| {
                        channel.destination.tab == tab
                            && channel.destination.document == command.document
                    })
                    .count()
                    >= MAX_CHANNELS_PER_DOCUMENT
                {
                    return Err("BroadcastChannel membership limit reached");
                }
                self.channels.push(Channel {
                    destination: source,
                    origin: origin.into(),
                    name: name.clone(),
                });
                Ok(true)
            }
            BroadcastOperation::Post { .. } => {
                let source_channel = position
                    .and_then(|index| self.channels.get(index))
                    .ok_or("unknown BroadcastChannel identifier")?;
                if source_channel.origin != origin {
                    return Err("BroadcastChannel origin changed");
                }
                let targets = self
                    .channels
                    .iter()
                    .filter(|channel| {
                        channel.destination != source
                            && channel.origin == origin
                            && channel.name == source_channel.name
                    })
                    .map(|channel| channel.destination)
                    .collect::<Vec<_>>();
                let BroadcastOperation::Post { serialized } = &command.operation else {
                    unreachable!()
                };
                if targets.is_empty() {
                    return Ok(true);
                }
                if targets.len() > MAX_PENDING_RECIPIENTS_PER_BROWSER {
                    return Err("BroadcastChannel fanout exceeds the browser recipient limit");
                }
                let mut per_tab = HashMap::<u64, usize>::new();
                for target in &targets {
                    *per_tab.entry(target.tab).or_default() += 1;
                }
                for (tab, count) in per_tab {
                    if self
                        .pending
                        .get(&tab)
                        .map_or(0, |pending| pending.queue.len())
                        + count
                        > MAX_PENDING_PER_TAB
                    {
                        return Ok(false);
                    }
                }
                // Recipients reference one shared serialized batch. A large but valid
                // fanout does not multiply payload memory by its destination count.
                let bytes = serialized.len().saturating_add(origin.len());
                if self.pending_recipients.saturating_add(targets.len())
                    > MAX_PENDING_RECIPIENTS_PER_BROWSER
                    || self.pending_bytes.saturating_add(bytes) > MAX_PENDING_BYTES_PER_BROWSER
                {
                    return Ok(false);
                }
                let batch_id = self
                    .next_batch_id
                    .checked_add(1)
                    .ok_or("BroadcastChannel batch identifier exhausted")?;
                self.next_batch_id = batch_id;
                self.batches.insert(
                    batch_id,
                    Batch {
                        origin: origin.into(),
                        serialized: serialized.clone(),
                        recipients: targets.len(),
                    },
                );
                self.pending_bytes += bytes;
                self.pending_recipients += targets.len();
                for target in targets {
                    let pending = self.pending.entry(target.tab).or_default();
                    pending.queue.push_back(QueuedDelivery {
                        document: target.document,
                        channel_id: target.channel_id,
                        batch_id,
                    });
                }
                Ok(true)
            }
            BroadcastOperation::Close => {
                if let Some(position) = position {
                    self.channels.remove(position);
                    let mut removed_batches = Vec::new();
                    if let Some(pending) = self.pending.get_mut(&tab) {
                        pending.queue.retain(|delivery| {
                            if delivery.document == command.document
                                && delivery.channel_id == command.channel_id
                            {
                                removed_batches.push(delivery.batch_id);
                                false
                            } else {
                                true
                            }
                        });
                    }
                    for batch_id in removed_batches {
                        self.release_batch_reference(batch_id);
                    }
                }
                Ok(true)
            }
        }
    }

    pub fn retire_tab(&mut self, tab: u64) {
        self.channels
            .retain(|channel| channel.destination.tab != tab);
        if let Some(pending) = self.pending.remove(&tab) {
            for delivery in pending.queue {
                self.release_batch_reference(delivery.batch_id);
            }
        }
    }

    pub fn has_pending(&self, tab: u64) -> bool {
        self.pending
            .get(&tab)
            .is_some_and(|pending| !pending.queue.is_empty())
    }

    /// A full renderer command queue leaves the oldest delivery in place for retry.
    pub fn take_if(&mut self, tab: u64, accepts: impl FnOnce(&BroadcastDelivery) -> bool) -> bool {
        let Some(front) = self
            .pending
            .get(&tab)
            .and_then(|pending| pending.queue.front())
            .copied()
        else {
            return false;
        };
        let batch = self
            .batches
            .get(&front.batch_id)
            .expect("queued batch exists");
        let delivery = BroadcastDelivery {
            document: front.document,
            channel_id: front.channel_id,
            origin: batch.origin.clone(),
            serialized: batch.serialized.clone(),
        };
        if !accepts(&delivery) {
            return false;
        }
        self.pending
            .get_mut(&tab)
            .expect("pending tab exists")
            .queue
            .pop_front();
        self.release_batch_reference(front.batch_id);
        true
    }

    fn release_batch_reference(&mut self, batch_id: u64) {
        self.pending_recipients -= 1;
        let batch = self
            .batches
            .get_mut(&batch_id)
            .expect("queued batch exists");
        batch.recipients -= 1;
        if batch.recipients == 0 {
            let batch = self.batches.remove(&batch_id).expect("queued batch exists");
            self.pending_bytes -= batch.origin.len() + batch.serialized.len();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(document: u64, channel_id: u64, operation: BroadcastOperation) -> BroadcastCommand {
        BroadcastCommand {
            document: DocumentId::new(document).unwrap(),
            channel_id,
            operation,
        }
    }

    #[test]
    fn same_origin_name_and_sender_exclusion_across_tabs() {
        let mut registry = BroadcastRegistry::default();
        let open = |document, channel_id, name: &str| {
            command(
                document,
                channel_id,
                BroadcastOperation::Open { name: name.into() },
            )
        };
        registry
            .apply(1, "https://a.test", &open(1, 1, "006100"))
            .unwrap_err();
        registry
            .apply(1, "https://a.test", &open(1, 1, "0061"))
            .unwrap();
        registry
            .apply(1, "https://a.test", &open(1, 2, "0061"))
            .unwrap();
        registry
            .apply(2, "https://a.test", &open(2, 1, "0061"))
            .unwrap();
        registry
            .apply(3, "https://b.test", &open(3, 1, "0061"))
            .unwrap();
        registry
            .apply(4, "https://a.test", &open(4, 1, "0062"))
            .unwrap();
        assert!(
            registry
                .apply(
                    1,
                    "https://a.test",
                    &command(
                        1,
                        1,
                        BroadcastOperation::Post {
                            serialized: "42".into()
                        }
                    )
                )
                .unwrap()
        );
        assert!(registry.has_pending(1));
        assert!(registry.has_pending(2));
        assert!(!registry.has_pending(3));
        assert!(!registry.has_pending(4));
        assert!(registry.take_if(1, |delivery| delivery.channel_id == 2
            && delivery.serialized == "42"));
        assert!(registry.take_if(2, |delivery| delivery.channel_id == 1
            && delivery.origin == "https://a.test"));
        registry.retire_tab(2);
        assert!(
            registry
                .apply(
                    1,
                    "https://a.test",
                    &command(
                        1,
                        1,
                        BroadcastOperation::Post {
                            serialized: "42".into()
                        }
                    )
                )
                .unwrap()
        );
        assert!(registry.has_pending(1));
        assert!(!registry.has_pending(2));
    }

    #[test]
    fn large_same_tab_fanout_is_shared_and_backpressured_in_fifo_order() {
        let mut registry = BroadcastRegistry::default();
        let open = |id| {
            command(
                1,
                id,
                BroadcastOperation::Open {
                    name: "0061".into(),
                },
            )
        };
        for id in 1..=128 {
            assert!(registry.apply(1, "https://a.test", &open(id)).unwrap());
        }
        let post = |value: &str| {
            command(
                1,
                1,
                BroadcastOperation::Post {
                    serialized: value.into(),
                },
            )
        };
        let first = "a".repeat(128 * 1024);
        let second = "b".repeat(128 * 1024);
        assert!(registry.apply(1, "https://a.test", &post(&first)).unwrap());
        assert!(registry.apply(1, "https://a.test", &post(&second)).unwrap());
        assert!(!registry.apply(1, "https://a.test", &post("third")).unwrap());
        assert_eq!(registry.batches.len(), 2);
        assert_eq!(
            registry.pending_bytes,
            first.len() + second.len() + 2 * "https://a.test".len()
        );
        for id in 2..=128 {
            assert!(registry.take_if(1, |delivery| delivery.channel_id == id
                && delivery.serialized == first));
        }
        assert!(registry.apply(1, "https://a.test", &post("third")).unwrap());
        for id in 2..=128 {
            assert!(registry.take_if(1, |delivery| delivery.channel_id == id
                && delivery.serialized == second));
        }
        assert_eq!(registry.batches.len(), 1);
        registry.retire_tab(1);
        assert!(!registry.has_pending(1));
        assert_eq!(registry.pending_bytes, 0);
        assert_eq!(registry.pending_recipients, 0);
    }
}
