//! Structured clone into the receiving realm; transferred buffers detach only after success.
use super::messaging::throw_named;
use std::{cell::RefCell, rc::Rc};
use v8::{ValueDeserializerHelper, ValueSerializerHelper};
mod platform;
pub(super) use platform::install;

mod delegate;
use delegate::CloneDelegate;

pub(super) struct Serialized {
    bytes: Vec<u8>,
    buffers: Vec<v8::SharedRef<v8::BackingStore>>,
    pending_transfers: Vec<v8::Global<v8::ArrayBuffer>>,
    pending_ports: Vec<v8::Global<v8::Object>>,
    pending_audio: Vec<v8::Global<v8::Object>>,
    ports: Vec<u32>,
    blobs: Vec<Vec<u8>>,
    audio: Vec<Vec<u8>>,
    endpoints: std::rc::Weak<super::ports::Ports>,
    committed: bool,
    received: std::cell::Cell<bool>,
}

impl Drop for Serialized {
    fn drop(&mut self) {
        if self.committed
            && !self.received.get()
            && let Some(endpoints) = self.endpoints.upgrade()
        {
            endpoints.retire(&self.ports);
        }
    }
}

impl Serialized {
    pub(super) fn write<'s>(
        scope: &mut v8::PinScope<'s, '_>,
        value: v8::Local<'s, v8::Value>,
        transfers: v8::Local<'s, v8::Array>,
    ) -> Option<Self> {
        let mut buffers = Vec::new();
        let mut ports = Vec::new();
        let mut port_ids = Vec::new();
        let mut pending_audio = Vec::new();
        let tree = super::frames::tree(scope.get_current_context())?;
        for index in 0..transfers.length() {
            let value = transfers.get_index(scope, index)?;
            if let Ok(object) = v8::Local::<v8::Object>::try_from(value)
                && super::ports::id(scope, object).is_some()
            {
                let Some(id) = tree
                    .ports
                    .valid(scope, object)
                    .filter(|id| !port_ids.contains(id))
                else {
                    throw_named(
                        scope,
                        "DataCloneError",
                        "Duplicate or detached MessagePort transfer",
                    );
                    return None;
                };
                ports.push(v8::Global::new(scope, object));
                port_ids.push(id);
                continue;
            }
            if let Ok(object) = v8::Local::<v8::Object>::try_from(value)
                && platform::is_audio(scope, object) == Some(true)
            {
                if platform::transferable_audio(scope, object) != Some(true)
                    || pending_audio
                        .iter()
                        .any(|audio| v8::Local::new(scope, audio) == object)
                {
                    throw_named(
                        scope,
                        "DataCloneError",
                        "Duplicate or non-transferable audio resource",
                    );
                    return None;
                }
                pending_audio.push(v8::Global::new(scope, object));
                continue;
            }
            let Ok(buffer) = v8::Local::<v8::ArrayBuffer>::try_from(value) else {
                throw_named(
                    scope,
                    "DataCloneError",
                    "This object is not a supported transferable",
                );
                return None;
            };
            if buffer.was_detached() || buffers.contains(&buffer) {
                throw_named(
                    scope,
                    "DataCloneError",
                    "A transfer list contains a duplicate or detached ArrayBuffer",
                );
                return None;
            }
            buffers.push(buffer);
        }
        let snapshots = Rc::new(RefCell::new(Vec::new()));
        let audio_snapshots = Rc::new(RefCell::new(Vec::new()));
        let serializer = v8::ValueSerializer::new(
            scope,
            Box::new(CloneDelegate {
                ports: ports.clone(),
                blobs: Vec::new(),
                audio: Vec::new(),
                snapshots: snapshots.clone(),
                audio_snapshots: audio_snapshots.clone(),
            }),
        );
        serializer.write_header();
        for (index, buffer) in buffers.iter().enumerate() {
            serializer.transfer_array_buffer(index as u32, *buffer);
        }
        if serializer.write_value(scope.get_current_context(), value) != Some(true) {
            return None;
        }
        let bytes = serializer.release();
        let blobs = std::mem::take(&mut *snapshots.borrow_mut());
        let audio = std::mem::take(&mut *audio_snapshots.borrow_mut());
        if bytes
            .len()
            .saturating_add(blobs.iter().map(Vec::len).sum::<usize>())
            .saturating_add(audio.iter().map(Vec::len).sum::<usize>())
            .saturating_add(
                buffers
                    .iter()
                    .map(|buffer| buffer.byte_length())
                    .sum::<usize>(),
            )
            > 16 * 1024 * 1024
        {
            throw_named(
                scope,
                "QuotaExceededError",
                "The message exceeds the 16 MiB structured-clone limit",
            );
            return None;
        }
        let stores = buffers
            .iter()
            .map(|buffer| buffer.get_backing_store())
            .collect();
        let pending_transfers = buffers
            .into_iter()
            .map(|buffer| v8::Global::new(scope, buffer))
            .collect();
        Some(Self {
            bytes,
            buffers: stores,
            pending_transfers,
            pending_ports: ports,
            pending_audio,
            ports: port_ids,
            blobs,
            audio,
            endpoints: Rc::downgrade(&tree.ports),
            committed: false,
            received: std::cell::Cell::new(false),
        })
    }

    pub(super) fn size(&self) -> usize {
        self.bytes
            .len()
            .saturating_add(self.blobs.iter().map(Vec::len).sum::<usize>())
            .saturating_add(self.audio.iter().map(Vec::len).sum::<usize>())
            .saturating_add(
                self.buffers
                    .iter()
                    .map(|buffer| buffer.byte_length())
                    .sum::<usize>(),
            )
    }

    pub(super) fn commit_transfers(&mut self, scope: &mut v8::PinScope) -> bool {
        let Some(tree) = super::frames::tree(scope.get_current_context()) else {
            return false;
        };
        let buffers: Vec<_> = self
            .pending_transfers
            .iter()
            .map(|buffer| v8::Local::new(scope, buffer))
            .collect();
        if buffers.iter().any(|buffer| buffer.was_detached())
            || self.pending_ports.iter().any(|port| {
                let port = v8::Local::new(scope, port);
                tree.ports.valid(scope, port).is_none()
            })
            || self.pending_audio.iter().any(|audio| {
                let audio = v8::Local::new(scope, audio);
                platform::transferable_audio(scope, audio) != Some(true)
            })
        {
            throw_named(
                scope,
                "DataCloneError",
                "A transfer was detached while serializing the message",
            );
            return false;
        }
        for buffer in buffers {
            buffer.detach(None);
        }
        self.pending_transfers.clear();
        for audio in self.pending_audio.drain(..) {
            let audio = v8::Local::new(scope, audio);
            if platform::detach_audio(scope, audio) != Some(true) {
                return false;
            }
        }
        self.committed = true;
        for port in self.pending_ports.drain(..) {
            let port = v8::Local::new(scope, port);
            if tree.ports.detach(scope, port).is_none() {
                return false;
            }
        }
        true
    }

    pub(super) fn read<'s>(
        &self,
        scope: &mut v8::PinScope<'s, '_>,
    ) -> Option<(v8::Local<'s, v8::Value>, v8::Local<'s, v8::Array>)> {
        let tree = super::frames::tree(scope.get_current_context())?;
        let mut ports = Vec::new();
        let port_array = v8::Array::new(scope, self.ports.len() as i32);
        for (index, id) in self.ports.iter().enumerate() {
            let port = tree.ports.receive(scope, *id)?;
            port_array.set_index(scope, index as u32, port.into())?;
            ports.push(v8::Global::new(scope, port));
        }
        // V8 forbids JavaScript execution inside ReadHostObject. Construct platform wrappers
        // first, then let the deserializer resolve only native side-table references.
        let mut blobs = Vec::new();
        for snapshot in &self.blobs {
            let blob = platform::restore(scope, snapshot)?;
            blobs.push(v8::Global::new(scope, blob));
        }
        let mut audio = Vec::new();
        for snapshot in &self.audio {
            let object = platform::restore_audio(scope, snapshot)?;
            audio.push(v8::Global::new(scope, object));
        }
        let decoder = v8::ValueDeserializer::new(
            scope,
            Box::new(CloneDelegate {
                ports,
                blobs,
                audio,
                snapshots: Default::default(),
                audio_snapshots: Default::default(),
            }),
            &self.bytes,
        );
        let context = scope.get_current_context();
        if decoder.read_header(context) != Some(true) {
            return None;
        }
        for (index, store) in self.buffers.iter().enumerate() {
            let buffer = v8::ArrayBuffer::with_backing_store(scope, store);
            decoder.transfer_array_buffer(index as u32, buffer);
        }
        decoder.read_value(context).map(|value| {
            self.received.set(true);
            (value, port_array)
        })
    }
}
