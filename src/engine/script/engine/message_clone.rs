//! Structured clone into the receiving realm; transferred buffers detach only after success.
use super::messaging::throw_named;
use v8::{ValueDeserializerHelper, ValueSerializerHelper};

struct CloneDelegate;
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
        Some(object.get_private(scope, brand)?.is_uint32())
    }
    fn write_host_object<'s>(
        &self,
        scope: &mut v8::PinScope<'s, '_>,
        _: v8::Local<'s, v8::Object>,
        _: &dyn ValueSerializerHelper,
    ) -> Option<bool> {
        throw_named(
            scope,
            "DataCloneError",
            "DOM nodes cannot be cloned in a message",
        );
        None
    }
}
impl v8::ValueDeserializerImpl for CloneDelegate {}

pub(super) struct Serialized {
    bytes: Vec<u8>,
    buffers: Vec<v8::SharedRef<v8::BackingStore>>,
    pending_transfers: Vec<v8::Global<v8::ArrayBuffer>>,
}

impl Serialized {
    pub(super) fn write<'s>(
        scope: &mut v8::PinScope<'s, '_>,
        value: v8::Local<'s, v8::Value>,
        transfers: v8::Local<'s, v8::Array>,
    ) -> Option<Self> {
        let mut buffers = Vec::new();
        for index in 0..transfers.length() {
            let value = transfers.get_index(scope, index)?;
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
        let serializer = v8::ValueSerializer::new(scope, Box::new(CloneDelegate));
        serializer.write_header();
        for (index, buffer) in buffers.iter().enumerate() {
            serializer.transfer_array_buffer(index as u32, *buffer);
        }
        if serializer.write_value(scope.get_current_context(), value) != Some(true) {
            return None;
        }
        let bytes = serializer.release();
        if bytes.len().saturating_add(
            buffers
                .iter()
                .map(|buffer| buffer.byte_length())
                .sum::<usize>(),
        ) > 16 * 1024 * 1024
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
        })
    }

    pub(super) fn size(&self) -> usize {
        self.bytes.len().saturating_add(
            self.buffers
                .iter()
                .map(|buffer| buffer.byte_length())
                .sum::<usize>(),
        )
    }

    pub(super) fn commit_transfers(&mut self, scope: &mut v8::PinScope) -> bool {
        let buffers: Vec<_> = self
            .pending_transfers
            .iter()
            .map(|buffer| v8::Local::new(scope, buffer))
            .collect();
        if buffers.iter().any(|buffer| buffer.was_detached()) {
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
        true
    }

    pub(super) fn read<'s>(
        &self,
        scope: &mut v8::PinScope<'s, '_>,
    ) -> Option<v8::Local<'s, v8::Value>> {
        let decoder = v8::ValueDeserializer::new(scope, Box::new(CloneDelegate), &self.bytes);
        let context = scope.get_current_context();
        if decoder.read_header(context) != Some(true) {
            return None;
        }
        for (index, store) in self.buffers.iter().enumerate() {
            let buffer = v8::ArrayBuffer::with_backing_store(scope, store);
            decoder.transfer_array_buffer(index as u32, buffer);
        }
        decoder.read_value(context)
    }
}
