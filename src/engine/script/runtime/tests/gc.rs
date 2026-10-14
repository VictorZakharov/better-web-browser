use super::super::ScriptRuntime;

impl ScriptRuntime {
    pub(in crate::engine::script) fn collect_garbage_for_testing(&mut self) {
        let context = self.context.as_deref_mut().expect("initialized test realm");
        context.collect_garbage_for_testing().unwrap();
        context.run_jobs().unwrap();
    }
}
