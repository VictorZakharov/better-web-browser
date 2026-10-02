use super::super::ports;
use super::{platform, throw_named};
use std::{cell::RefCell, rc::Rc};
use v8::{ValueDeserializerHelper, ValueSerializerHelper};

pub(super) struct CloneDelegate {
    pub(super) ports: Vec<v8::Global<v8::Object>>,
    pub(super) blobs: Vec<v8::Global<v8::Object>>,
    pub(super) audio: Vec<v8::Global<v8::Object>>,
    pub(super) snapshots: Rc<RefCell<Vec<Vec<u8>>>>,
    pub(super) audio_snapshots: Rc<RefCell<Vec<Vec<u8>>>>,
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
                || ports::id(scope, object).is_some()
                || platform::is_blob(scope, object).unwrap_or(false)
                || platform::is_audio(scope, object).unwrap_or(false),
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
        let tag = if platform::is_blob(scope, object) == Some(true) {
            1
        } else if platform::is_audio(scope, object) == Some(true) {
            2
        } else {
            0
        };
        if tag != 0 {
            let snapshot = if tag == 1 {
                platform::snapshot(scope, object)?
            } else {
                platform::snapshot_audio(scope, object)?
            };
            if self
                .snapshots
                .borrow()
                .iter()
                .chain(self.audio_snapshots.borrow().iter())
                .map(Vec::len)
                .sum::<usize>()
                .saturating_add(snapshot.len())
                > 16 * 1024 * 1024
            {
                throw_named(
                    scope,
                    "QuotaExceededError",
                    "The message's platform resource data exceeds its limit",
                );
                return None;
            }
            let mut snapshots = if tag == 1 {
                self.snapshots.borrow_mut()
            } else {
                self.audio_snapshots.borrow_mut()
            };
            serializer.write_uint32(tag);
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
        if tag > 2 {
            return None;
        }
        let mut index = 0;
        if !deserializer.read_uint32(&mut index) {
            return None;
        }
        (match tag {
            0 => &self.ports,
            1 => &self.blobs,
            _ => &self.audio,
        })
        .get(index as usize)
        .map(|port| v8::Local::new(scope, port))
    }
}
