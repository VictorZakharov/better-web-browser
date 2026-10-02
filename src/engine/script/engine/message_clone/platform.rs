//! Private platform-object serialization hooks; no author-replaceable constructors/getters.
use v8::{ValueDeserializerHelper, ValueSerializerHelper};
struct Snapshot;
impl v8::ValueSerializerImpl for Snapshot {
    fn throw_data_clone_error<'s>(
        &self,
        scope: &mut v8::PinScope<'s, '_>,
        message: v8::Local<'s, v8::String>,
    ) {
        let error = v8::Exception::error(scope, message);
        scope.throw_exception(error);
    }
}
impl v8::ValueDeserializerImpl for Snapshot {}
pub(in crate::engine::script::engine) fn install(scope: &mut v8::PinScope) -> Option<()> {
    install_named(scope, "__blobCloneBindings")?;
    install_named(scope, "__audioMessageCloneBindings")
}
fn install_named(scope: &mut v8::PinScope, binding_name: &str) -> Option<()> {
    let global = scope.get_current_context().global(scope);
    let name = v8::String::new(scope, binding_name)?;
    let value = global.get(scope, name.into())?;
    let private = v8::Private::for_api(scope, Some(name));
    global.set_private(scope, private, value)?;
    (global.delete(scope, name.into()) == Some(true)).then_some(())
}
fn binding<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    index: u32,
    binding_name: &str,
) -> Option<v8::Local<'s, v8::Function>> {
    let global = scope.get_current_context().global(scope);
    let name = v8::String::new(scope, binding_name)?;
    let key = v8::Private::for_api(scope, Some(name));
    let array = v8::Local::<v8::Array>::try_from(global.get_private(scope, key)?).ok()?;
    v8::Local::<v8::Function>::try_from(array.get_index(scope, index)?).ok()
}
pub(super) fn is_blob(scope: &mut v8::PinScope, object: v8::Local<v8::Object>) -> Option<bool> {
    check(scope, object, "__blobCloneBindings", 0)
}
pub(super) fn is_audio(scope: &mut v8::PinScope, object: v8::Local<v8::Object>) -> Option<bool> {
    check(scope, object, "__audioMessageCloneBindings", 0)
}
pub(super) fn transferable_audio(
    scope: &mut v8::PinScope,
    object: v8::Local<v8::Object>,
) -> Option<bool> {
    check(scope, object, "__audioMessageCloneBindings", 3)
}
pub(super) fn detach_audio(
    scope: &mut v8::PinScope,
    object: v8::Local<v8::Object>,
) -> Option<bool> {
    check(scope, object, "__audioMessageCloneBindings", 4)
}
fn check(
    scope: &mut v8::PinScope,
    object: v8::Local<v8::Object>,
    name: &str,
    index: u32,
) -> Option<bool> {
    let owner = object.get_creation_context(scope)?;
    let scope = &mut v8::ContextScope::new(scope, owner);
    let function = binding(scope, index, name)?;
    let receiver = v8::undefined(scope).into();
    Some(function.call(scope, receiver, &[object.into()])?.is_true())
}
pub(super) fn snapshot<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    object: v8::Local<'s, v8::Object>,
) -> Option<Vec<u8>> {
    snapshot_named(scope, object, "__blobCloneBindings")
}
pub(super) fn snapshot_audio<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    object: v8::Local<'s, v8::Object>,
) -> Option<Vec<u8>> {
    snapshot_named(scope, object, "__audioMessageCloneBindings")
}
fn snapshot_named<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    object: v8::Local<'s, v8::Object>,
    name: &str,
) -> Option<Vec<u8>> {
    let owner = object.get_creation_context(scope)?;
    let scope = &mut v8::ContextScope::new(scope, owner);
    let function = binding(scope, 1, name)?;
    let receiver = v8::undefined(scope).into();
    let value = function.call(scope, receiver, &[object.into()])?;
    let serializer = v8::ValueSerializer::new(scope, Box::new(Snapshot));
    serializer.write_header();
    if serializer.write_value(scope.get_current_context(), value) != Some(true) {
        return None;
    }
    Some(serializer.release())
}
pub(super) fn restore<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    bytes: &[u8],
) -> Option<v8::Local<'s, v8::Object>> {
    restore_named(scope, bytes, "__blobCloneBindings")
}
pub(super) fn restore_audio<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    bytes: &[u8],
) -> Option<v8::Local<'s, v8::Object>> {
    restore_named(scope, bytes, "__audioMessageCloneBindings")
}
fn restore_named<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    bytes: &[u8],
    name: &str,
) -> Option<v8::Local<'s, v8::Object>> {
    let decoder = v8::ValueDeserializer::new(scope, Box::new(Snapshot), bytes);
    let context = scope.get_current_context();
    if decoder.read_header(context) != Some(true) {
        return None;
    }
    let value = decoder.read_value(context)?;
    let function = binding(scope, 2, name)?;
    let receiver = v8::undefined(scope).into();
    v8::Local::<v8::Object>::try_from(function.call(scope, receiver, &[value])?).ok()
}
