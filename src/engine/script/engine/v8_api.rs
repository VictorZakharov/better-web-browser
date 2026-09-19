//! Checked ownership boundary for the pinned V8 public-API adapter.
unsafe extern "C" {
    fn breeze_v8_capture_incumbent(result: *const v8::Object) -> bool;
    fn breeze_v8_detach_global(context: *const v8::Context);
}

pub(super) fn incumbent<'s>(
    scope: &mut v8::PinScope<'s, '_>,
) -> Option<v8::Local<'s, v8::Context>> {
    let result = v8::Object::new(scope);
    // The temporary local object transports the handle through public V8 property APIs.
    // No persistent ownership or handle representation crosses this C ABI.
    if !unsafe { breeze_v8_capture_incumbent(&*result) } {
        return None;
    }
    let name = v8::String::new(scope, "Breeze.Incumbent")?;
    let key = v8::Private::for_api(scope, Some(name));
    let value = result.get_private(scope, key)?;
    let global = v8::Local::<v8::Object>::try_from(value).ok()?;
    global.get_creation_context(scope)
}

pub(super) fn detach(context: v8::Local<v8::Context>) {
    // Call only while the owning isolate is entered and before reusing its global proxy.
    unsafe { breeze_v8_detach_global(&*context) };
}
