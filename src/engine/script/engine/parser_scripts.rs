//! Nested classic evaluation shares the active isolate, watchdog deadline and global scope.
pub(super) fn run(scope: &mut v8::PinScope, arguments: v8::FunctionCallbackArguments) {
    if super::node_wrappers::host(scope.get_current_context()).is_some_and(|host| {
        let host = host.borrow();
        host.sandbox.scripts_blocked || !host.policy.allows_inline(false)
    }) {
        return;
    }
    let (Ok(code), Ok(url)) = (
        v8::Local::<v8::String>::try_from(arguments.get(1)),
        v8::Local::<v8::String>::try_from(arguments.get(2)),
    ) else {
        return;
    };
    let origin = v8::ScriptOrigin::new(
        scope,
        url.into(),
        0,
        0,
        false,
        0,
        None,
        false,
        false,
        false,
        None,
    );
    if let Some(script) = v8::Script::compile(scope, code, Some(&origin)) {
        // Preserve pending exceptions for the binding's error-reporting algorithm. Do not
        // convert the completion value: script evaluation does not coerce it to a string.
        script.run(scope);
    }
}
