//! Realm-confined capture state; immutable messages, never V8 handles, cross threads.
//! HTML structured serialization stays in the existing shared graph implementation.
use crate::engine::script::worker_message::{self, BinaryBytes, Lease, WorkerMessage};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};
mod bindings;
mod capture;
pub(super) use bindings::dispatch;
pub(super) use capture::{install, serialize};

const MAX_NESTING: usize = 32;
const MAX_BINARIES: usize = 4096;
pub(super) const MAX_BINARY_BYTES: usize = 16 * 1024 * 1024;
const TOKEN_PREFIX: &str = "@breeze-binary/";

#[derive(Default)]
pub(super) struct State {
    next_id: Cell<u64>,
    poisoned: Cell<bool>,
    writers: RefCell<Vec<(u64, Writer)>>,
    readers: RefCell<Vec<(u64, Reader)>>,
}
struct Writer {
    nonce: String,
    binaries: Vec<BinaryBytes>,
    lease: Lease,
}
struct Reader {
    message: WorkerMessage,
    consumed: Vec<bool>,
}

impl State {
    fn next(&self) -> Result<u64, &'static str> {
        if self.poisoned.get() {
            return Err("Worker clone capture is unavailable");
        }
        let id = self
            .next_id
            .get()
            .checked_add(1)
            .ok_or("Worker clone identifiers exhausted")?;
        self.next_id.set(id);
        Ok(id)
    }

    pub(super) fn write(self: &Rc<Self>) -> Result<WriterGuard, &'static str> {
        if self.writers.borrow().len() >= MAX_NESTING {
            return Err("Worker clone nesting limit exceeded");
        }
        let id = self.next()?;
        let mut random = [0; 16];
        getrandom::fill(&mut random).map_err(|_| "Worker clone nonce allocation failed")?;
        let nonce: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
        let lease = worker_message::begin()?;
        self.writers.borrow_mut().push((
            id,
            Writer {
                nonce,
                binaries: Vec::new(),
                lease,
            },
        ));
        Ok(WriterGuard {
            state: self.clone(),
            id,
            finished: false,
        })
    }

    pub(super) fn append(
        &self,
        length: usize,
        copy: impl FnOnce() -> Result<BinaryBytes, &'static str>,
    ) -> Result<String, &'static str> {
        if length > MAX_BINARY_BYTES {
            return Err("Worker clone binary exceeds the 16 MiB limit");
        }
        let mut writers = self.writers.borrow_mut();
        let (_, writer) = writers
            .last_mut()
            .ok_or("Worker clone has no active writer")?;
        if writer.binaries.len() >= MAX_BINARIES {
            return Err("Worker clone binary count limit exceeded");
        }
        writer.lease.grow(
            length
                .checked_add(worker_message::BINARY_ENTRY_CHARGE)
                .ok_or("Worker clone binary size overflow")?,
        )?;
        // Reserve before allocating. The copy closure only reads native V8 byte storage;
        // it must not execute JavaScript or reenter the writer stack.
        let bytes = copy()?;
        if bytes.len() != length {
            return Err("Worker clone binary copy length changed");
        }
        let token = format!("{TOKEN_PREFIX}{}/{}", writer.nonce, writer.binaries.len());
        writer.binaries.push(bytes);
        Ok(token)
    }

    pub(super) fn read(
        self: &Rc<Self>,
        message: WorkerMessage,
    ) -> Result<ReaderGuard, &'static str> {
        if self.readers.borrow().len() >= MAX_NESTING {
            return Err("Worker message nesting limit exceeded");
        }
        let id = self.next()?;
        let consumed = vec![false; message.binary_count()];
        self.readers
            .borrow_mut()
            .push((id, Reader { message, consumed }));
        Ok(ReaderGuard {
            state: self.clone(),
            id,
        })
    }

    pub(super) fn consume(&self, token: &str) -> Result<BinaryBytes, &'static str> {
        let encoded = token
            .strip_prefix(TOKEN_PREFIX)
            .ok_or("Invalid Worker binary token")?;
        let (nonce, index) = encoded
            .split_once('/')
            .ok_or("Invalid Worker binary token")?;
        let index: usize = index.parse().map_err(|_| "Invalid Worker binary index")?;
        let mut readers = self.readers.borrow_mut();
        let (_, reader) = readers
            .last_mut()
            .ok_or("Worker binary has no active receiver")?;
        let bytes = reader
            .message
            .binary(nonce, index)
            .ok_or("Worker binary does not belong to this message")?;
        let consumed = reader
            .consumed
            .get_mut(index)
            .ok_or("Invalid Worker binary index")?;
        if *consumed {
            return Err("Worker binary token was already consumed");
        }
        *consumed = true;
        Ok(bytes)
    }

    fn unwind<T>(&self, frames: &RefCell<Vec<(u64, T)>>, id: u64) {
        let mut frames = frames.borrow_mut();
        if frames.last().is_some_and(|frame| frame.0 == id) {
            frames.pop();
        } else {
            frames.clear();
            self.poisoned.set(true);
        }
    }
}

pub(super) struct WriterGuard {
    state: Rc<State>,
    id: u64,
    finished: bool,
}
impl WriterGuard {
    pub(super) fn finish(mut self, metadata: String) -> Result<WorkerMessage, &'static str> {
        let mut writers = self.state.writers.borrow_mut();
        if writers.last().is_none_or(|frame| frame.0 != self.id) {
            return Err("Worker clone writer ownership changed");
        }
        let (_, writer) = writers.pop().unwrap();
        self.finished = true;
        WorkerMessage::from_parts(metadata, writer.nonce, writer.binaries, writer.lease)
    }
}
impl Drop for WriterGuard {
    fn drop(&mut self) {
        if !self.finished {
            self.state.unwind(&self.state.writers, self.id);
        }
    }
}
pub(super) struct ReaderGuard {
    state: Rc<State>,
    id: u64,
}
impl Drop for ReaderGuard {
    fn drop(&mut self) {
        self.state.unwind(&self.state.readers, self.id);
    }
}

pub(super) fn state(scope: &mut v8::PinScope) -> Result<Rc<State>, &'static str> {
    scope
        .get_current_context()
        .get_slot::<State>()
        .ok_or("Worker clone realm state is unavailable")
}

#[cfg(test)]
mod tests;
