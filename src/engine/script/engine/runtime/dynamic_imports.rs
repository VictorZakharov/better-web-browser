use super::super::dynamic_imports::{Imports, Origin, fulfilled, rejected};
use super::*;

impl Context {
    pub(in crate::engine::script) fn set_module_response_base(&mut self, key: &str, base: &str) {
        self.imports
            .origins
            .borrow_mut()
            .entry(key.into())
            .or_insert_with(|| Origin {
                base: base.into(),
                options: crate::engine::script::ScriptFetchOptions::for_kind(
                    crate::engine::script::ScriptKind::Module,
                ),
            })
            .base = base.into();
    }

    pub(in crate::engine::script) fn register_script_origin(
        &mut self,
        key: &str,
        base: &str,
        options: crate::engine::script::ScriptFetchOptions,
    ) {
        self.imports
            .origins
            .borrow_mut()
            .entry(key.into())
            .or_insert_with(|| Origin {
                base: base.into(),
                options,
            });
    }

    pub(in crate::engine::script) fn finish_import(
        &mut self,
        id: u32,
        url: &str,
        result: Result<(), String>,
        cache_error: bool,
    ) -> JsResult<()> {
        let context = self.context.clone();
        self.watchdog.run(&mut self.isolate, |isolate| {
            v8::scope!(let scope, isolate);
            let context = v8::Local::new(scope, &context);
            let scope = &mut v8::ContextScope::new(scope, context);
            v8::tc_scope!(let tc, scope);
            let imports = context.get_slot::<Imports>().unwrap();
            let Some(resolver) = imports.resolvers.borrow().get(&id).cloned() else {
                return Ok(());
            };
            let resolver = v8::Local::new(tc, resolver);
            let registry = context.get_slot::<super::super::modules::ModuleRegistry>();
            let error = registry
                .as_ref()
                .and_then(|r| r.errors.borrow().get(url).cloned());
            if let Some(error) = error {
                let error = v8::Local::new(tc, error);
                resolver.reject(tc, error);
                imports.resolvers.borrow_mut().remove(&id);
                return Ok(());
            }
            if let Err(message) = result {
                let message = v8::String::new(tc, &message)
                    .ok_or_else(|| allocation_error("import error"))?;
                let error = v8::Exception::type_error(tc, message);
                if cache_error && let Some(registry) = &registry {
                    registry
                        .errors
                        .borrow_mut()
                        .insert(url.into(), v8::Global::new(tc, error));
                }
                resolver.reject(tc, error);
                imports.resolvers.borrow_mut().remove(&id);
                return Ok(());
            }
            let module = registry
                .as_ref()
                .and_then(|r| r.by_url.borrow().get(url).cloned())
                .ok_or_else(|| allocation_error("prepared module"))?;
            let module = v8::Local::new(tc, module);
            if module.get_status() == v8::ModuleStatus::Uninstantiated
                && module.instantiate_module(tc, super::super::modules::resolve_module)
                    != Some(true)
            {
                let error = tc
                    .exception()
                    .ok_or_else(|| caught_error(tc, "link import"))?;
                tc.reset();
                resolver.reject(tc, error);
                imports.resolvers.borrow_mut().remove(&id);
                return Ok(());
            }
            if module.get_status() == v8::ModuleStatus::Errored {
                resolver.reject(tc, module.get_exception());
                imports.resolvers.borrow_mut().remove(&id);
                return Ok(());
            }
            let value = module
                .evaluate(tc)
                .ok_or_else(|| caught_error(tc, "evaluate import"))?;
            let promise = v8::Local::<v8::Promise>::try_from(value)
                .map_err(|_| allocation_error("module promise"))?;
            let id_value = v8::Integer::new_from_unsigned(tc, id);
            let data =
                v8::Array::new_with_elements(tc, &[id_value.into(), module.get_module_namespace()]);
            let on_fulfilled = v8::Function::builder(fulfilled)
                .data(data.into())
                .build(tc)
                .ok_or_else(|| allocation_error("import fulfillment"))?;
            let on_rejected = v8::Function::builder(rejected)
                .data(data.into())
                .build(tc)
                .ok_or_else(|| allocation_error("import rejection"))?;
            promise
                .then2(tc, on_fulfilled, on_rejected)
                .ok_or_else(|| caught_error(tc, "observe module evaluation"))?;
            Ok(())
        })
    }
}
