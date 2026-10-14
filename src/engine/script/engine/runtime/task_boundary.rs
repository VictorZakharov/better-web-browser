//! Native settlement belongs to the HTML scheduler, not arbitrary V8 calls.
impl super::Context {
    pub(in crate::engine::script) fn complete_task(&mut self) -> super::JsResult<()> {
        if self.private_hooks.contains_key("__flushAudioRender") {
            self.call_private_hook("__flushAudioRender", &[])?;
        }
        self.complete_gpu_task()?;
        self.agent.borrow_mut().memory_pressure_checkpoint()
    }
}
