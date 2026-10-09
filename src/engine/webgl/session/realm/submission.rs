//! One outstanding void batch per realm, plus one bounded caller-side list.
//! Commands remain owned and FIFO; queries, uploads and task boundaries drain both.
use super::*;

pub(super) struct Flight {
    reply: Submitted,
    ids: HashSet<u32>,
}

impl Contexts {
    pub(super) fn submit_pending(&mut self) {
        // Apply loss before taking the next list: remove() deletes unsent work
        // for lost IDs while preserving the order of every surviving context.
        self.reap_pending(true);
        let commands = self.pending.take();
        if commands.is_empty() {
            return;
        }
        let ids: HashSet<_> = commands.iter().map(|(id, _)| *id).collect();
        match submit(Operation::Batch(commands)) {
            Some(reply) => self.inflight = Some(Flight { reply, ids }),
            None => self.apply_batch_reply(None, ids),
        }
    }

    pub(super) fn flush_pending(&mut self) {
        self.submit_pending();
        self.reap_pending(true);
    }

    pub(super) fn reap_pending(&mut self, wait: bool) {
        let Some(flight) = self.inflight.as_ref() else {
            return;
        };
        if !wait {
            let Some(reply) = flight.reply.poll() else {
                return;
            };
            let flight = self.inflight.take().unwrap();
            self.apply_batch_reply(reply, flight.ids);
            return;
        }
        let flight = self.inflight.take().unwrap();
        self.apply_batch_reply(flight.reply.wait(), flight.ids);
    }

    fn apply_batch_reply(&mut self, reply: Option<Reply>, ids: HashSet<u32>) {
        let lost = match reply {
            Some(Reply::Batch(lost)) => lost,
            // A lost owner/reply cannot leave handles admitted as live. Only
            // the submitting realm's IDs are retired, never a peer registry.
            _ => ids.iter().copied().collect(),
        };
        for id in lost {
            if ids.contains(&id) {
                self.remove(id);
            }
        }
    }
}

#[cfg(test)]
mod tests;
