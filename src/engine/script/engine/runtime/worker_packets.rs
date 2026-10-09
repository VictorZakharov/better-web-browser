//! Private realm setup and a reader lease scoped only around synchronous graph decoding.
use super::*;
use crate::engine::script::JsNativeError;
use crate::engine::script::worker_message::WorkerMessage;

impl Context {
    pub(in crate::engine::script) fn install_worker_packets(&mut self) -> JsResult<()> {
        let context = self.context.clone();
        self.agent.borrow_mut().run(|isolate| {
            v8::scope!(let scope,isolate);
            let context = v8::Local::new(scope, &context);
            let scope = &mut v8::ContextScope::new(scope, context);
            super::super::worker_packets::install(scope)
                .ok_or_else(|| allocation_error("Worker clone bindings"))
        })
    }

    pub(in crate::engine::script) fn call_worker_hook(
        &mut self,
        name: &str,
        arguments: &[JsValue],
        message: WorkerMessage,
    ) -> JsResult<JsValue> {
        let _reader = self
            .worker_packets
            .read(message)
            .map_err(|message| JsNativeError::typ().with_message(message))?;
        self.call_private_hook(name, arguments)
    }
}
