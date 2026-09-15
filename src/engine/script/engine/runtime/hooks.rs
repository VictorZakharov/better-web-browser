//! Embedder callbacks retained privately, not page-callable trusted event factories.
use super::*;

impl Context {
    pub(in crate::engine::script) fn capture_hook(&mut self, name: &str) -> JsResult<()> {
        let context = self.context.clone();
        let function = self.watchdog.run(&mut self.isolate, |isolate| {
            v8::scope!(let scope, isolate);
            let context = v8::Local::new(scope, &context);
            let scope = &mut v8::ContextScope::new(scope, context);
            v8::tc_scope!(let tc, scope);
            let key = v8::String::new(tc, name).ok_or_else(|| allocation_error("hook name"))?;
            let global = context.global(tc);
            let value = global
                .get(tc, key.into())
                .ok_or_else(|| caught_error(tc, "read bootstrap hook"))?;
            let function = v8::Local::<v8::Function>::try_from(value)
                .map_err(|_| allocation_error("bootstrap hook"))?;
            if global.delete(tc, key.into()) != Some(true) {
                return Err(allocation_error("remove bootstrap hook"));
            }
            Ok(v8::Global::new(tc, function))
        })?;
        self.private_hooks.insert(name.into(), function);
        Ok(())
    }
}
