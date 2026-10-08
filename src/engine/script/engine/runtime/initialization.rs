//! Process platform ownership and per-realm global-object setup.
use std::sync::{Once, OnceLock};

static INITIALIZE_V8: Once = Once::new();
static V8_PLATFORM: OnceLock<v8::SharedRef<v8::Platform>> = OnceLock::new();

pub(crate) fn initialize_v8() {
    INITIALIZE_V8.call_once(|| {
        let platform = v8::new_default_platform(0, false).make_shared();
        v8::V8::initialize_platform(platform.clone());
        v8::V8::initialize();
        V8_PLATFORM
            .set(platform)
            .unwrap_or_else(|_| unreachable!("V8 platform initialized once"));
    });
}

pub(super) fn global_template<'s>(
    scope: &mut v8::PinScope<'s, '_, ()>,
    is_window: bool,
) -> v8::Local<'s, v8::ObjectTemplate> {
    // A dedicated worker owns an isolated WorkerGlobalScope, not a WindowProxy.
    // Cross-origin Window handlers remain installed for document realms only.
    // https://html.spec.whatwg.org/multipage/workers.html#workerglobalscope
    if is_window {
        super::super::v8_api::window_template(scope)
    } else {
        v8::ObjectTemplate::new(scope)
    }
}
