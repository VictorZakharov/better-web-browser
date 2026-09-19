//! Engine code-generation boundaries that JavaScript cannot override.
pub(super) unsafe extern "C" fn allow_wasm(
    context: v8::Local<v8::Context>,
    _: v8::Local<v8::String>,
) -> bool {
    super::node_wrappers::host(context).is_none_or(|host| {
        let host = host.borrow();
        !host.sandbox.scripts_blocked && host.policy.allows_eval()
    })
}
