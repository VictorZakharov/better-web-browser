//! Structured clone into the receiving realm; transferred buffers detach only after success.
use super::messaging::throw_named;
use std::{cell::RefCell, rc::Rc};
use v8::{ValueDeserializerHelper, ValueSerializerHelper};
mod platform;
pub(super) use platform::install;

struct CloneDelegate {
    ports: Vec<v8::Global<v8::Object>>,
    blobs: Vec<v8::Global<v8::Object>>,
    snapshots: Rc<RefCell<Vec<Vec<u8>>>>,
}
impl v8::ValueSerializerImpl for CloneDelegate {
    fn throw_data_clone_error<'s>(
        &self,
        scope: &mut v8::PinScope<'s, '_>,
        message: v8::Local<'s, v8::String>,
    ) {
        let message = message.to_rust_string_lossy(scope);
        throw_named(scope, "DataCloneError", &message);
    }
    fn has_custom_host_object(&self, _: &v8::Isolate) -> bool {
        true
    }
    fn is_host_object<'s>(
        &self,
        scope: &mut v8::PinScope<'s, '_>,
        object: v8::Local<'s, v8::Object>,
    ) -> Option<bool> {
        if object
            .get_creation_context(scope)
            .is_some_and(|context| context.global(scope) == object)
        {
            return Some(true);
        }
        let name = v8::String::new(scope, "Breeze.Node.handle")?;
        let brand = v8::Private::for_api(scope, Some(name));
        Some(
            object.get_private(scope, brand)?.is_uint32()
                || super::ports::id(scope, object).is_some()
                || platform::is_blob(scope, object).unwrap_or(false),
        )
    }
    fn write_host_object<'s>(
        &self,
        scope: &mut v8::PinScope<'s, '_>,
        object: v8::Local<'s, v8::Object>,
        serializer: &dyn ValueSerializerHelper,
    ) -> Option<bool> {
        if let Some(index) = self
            .ports
            .iter()
            .position(|port| v8::Local::new(scope, port) == object)
        {
            serializer.write_uint32(0);
            serializer.write_uint32(index as u32);
            return Some(true);
        }
        if platform::is_blob(scope, object) == Some(true) {
            let snapshot = platform::snapshot(scope, object)?;
            let mut snapshots = self.snapshots.borrow_mut();
            if snapshots
                .iter()
                .map(Vec::len)
                .sum::<usize>()
                .saturating_add(snapshot.len())
                > 16 * 1024 * 1024
            {
                throw_named(
                    scope,
                    "QuotaExceededError",
                    "The message's Blob data exceeds its limit",
                );
                return None;
            }
            serializer.write_uint32(1);
            serializer.write_uint32(snapshots.len() as u32);
            snapshots.push(snapshot);
            return Some(true);
        }
        throw_named(
            scope,
            "DataCloneError",
            "This platform object cannot be cloned without a supported transfer",
        );
        None
    }
}
impl v8::ValueDeserializerImpl for CloneDelegate {
    fn read_host_object<'s>(
        &self,
        scope: &mut v8::PinScope<'s, '_>,
        deserializer: &dyn ValueDeserializerHelper,
    ) -> Option<v8::Local<'s, v8::Object>> {
        let mut tag = 0;
        if !deserializer.read_uint32(&mut tag) {
            return None;
        }
        if tag > 1 {
            return None;
        }
        let mut index = 0;
        if !deserializer.read_uint32(&mut index) {
            return None;
        }
        (if tag == 0 { &self.ports } else { &self.blobs })
            .get(index as usize)
            .map(|port| v8::Local::new(scope, port))
    }
}

pub(super) struct Serialized {
    bytes: Vec<u8>,
    buffers: Vec<v8::SharedRef<v8::BackingStore>>,
    pending_transfers: Vec<v8::Global<v8::ArrayBuffer>>,
    pending_ports: Vec<v8::Global<v8::Object>>,
    ports: Vec<u32>,
    blobs: Vec<Vec<u8>>,
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
        let serializer = v8::ValueSerializer::new(
            scope,
            Box::new(CloneDelegate {
                ports: ports.clone(),
                blobs: Vec::new(),
                snapshots: snapshots.clone(),
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
        if bytes
            .len()
            .saturating_add(blobs.iter().map(Vec::len).sum::<usize>())
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
            ports: port_ids,
            blobs,
            endpoints: Rc::downgrade(&tree.ports),
            committed: false,
            received: std::cell::Cell::new(false),
        })
    }

    pub(super) fn size(&self) -> usize {
        self.bytes
            .len()
            .saturating_add(self.blobs.iter().map(Vec::len).sum::<usize>())
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
        let decoder = v8::ValueDeserializer::new(
            scope,
            Box::new(CloneDelegate {
                ports,
                blobs,
                snapshots: Default::default(),
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
