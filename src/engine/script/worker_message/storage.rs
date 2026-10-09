//! A process-wide pending-clone budget also covers reentrant writer captures.
//! It follows the immutable envelope until its last native owner releases it.
use super::MAX_MESSAGE_BYTES;
use std::sync::{Arc, Mutex, OnceLock};

const MAX_PENDING_BYTES: usize = 128 * 1024 * 1024;
const MAX_PENDING_MESSAGES: usize = 1024;
const ENVELOPE_CHARGE: usize = 128;

#[derive(Default)]
struct Usage {
    bytes: usize,
    messages: usize,
    poisoned: bool,
}

pub(in crate::engine::script) struct Budget {
    usage: Mutex<Usage>,
    bytes: usize,
    messages: usize,
}

impl Budget {
    fn new(bytes: usize, messages: usize) -> Self {
        Self {
            usage: Mutex::new(Usage::default()),
            bytes,
            messages,
        }
    }

    fn begin(self: &Arc<Self>) -> Result<Lease, &'static str> {
        let mut usage = self
            .usage
            .lock()
            .map_err(|_| "Worker message accounting is unavailable")?;
        let next = usage
            .bytes
            .checked_add(ENVELOPE_CHARGE)
            .ok_or("Worker message budget overflow")?;
        if usage.poisoned || usage.messages >= self.messages || next > self.bytes {
            return Err("Pending Worker messages exceed their storage limit");
        }
        usage.bytes = next;
        usage.messages += 1;
        Ok(Lease {
            budget: self.clone(),
            bytes: ENVELOPE_CHARGE,
        })
    }
}

pub(in crate::engine::script) struct Lease {
    budget: Arc<Budget>,
    bytes: usize,
}

impl Lease {
    pub(in crate::engine::script) fn bytes(&self) -> usize {
        self.bytes
    }

    pub(in crate::engine::script) fn grow(&mut self, bytes: usize) -> Result<(), &'static str> {
        let next = self
            .bytes
            .checked_add(bytes)
            .filter(|bytes| *bytes <= MAX_MESSAGE_BYTES)
            .ok_or("Worker message exceeds its 32 MiB storage limit")?;
        let mut usage = self
            .budget
            .usage
            .lock()
            .map_err(|_| "Worker message accounting is unavailable")?;
        let total = usage
            .bytes
            .checked_add(bytes)
            .ok_or("Worker message budget overflow")?;
        if usage.poisoned || total > self.budget.bytes {
            return Err("Pending Worker messages exceed their storage limit");
        }
        usage.bytes = total;
        self.bytes = next;
        Ok(())
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        if let Ok(mut usage) = self.budget.usage.lock() {
            if let Some(bytes) = usage.bytes.checked_sub(self.bytes)
                && let Some(messages) = usage.messages.checked_sub(1)
            {
                usage.bytes = bytes;
                usage.messages = messages;
            } else {
                // Never grant fresh space after an internal accounting violation.
                usage.poisoned = true;
            }
        }
    }
}

pub(in crate::engine::script) fn begin() -> Result<Lease, &'static str> {
    static BUDGET: OnceLock<Arc<Budget>> = OnceLock::new();
    BUDGET
        .get_or_init(|| Arc::new(Budget::new(MAX_PENDING_BYTES, MAX_PENDING_MESSAGES)))
        .begin()
}

#[cfg(test)]
mod tests;
