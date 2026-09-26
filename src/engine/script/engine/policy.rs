//! Engine code-generation boundaries that JavaScript cannot override.
pub(super) unsafe extern "C" fn allow_wasm(
    context: v8::Local<v8::Context>,
    _: v8::Local<v8::String>,
) -> bool {
    context
        .get_slot::<super::bridge::HostBridge>()
        .and_then(|bridge| bridge.code_generation_allowed())
        .unwrap_or(false)
}
