use super::bridge::{HostBridge, install_host_call, value_from_v8, value_to_v8};
use super::modules::EngineModuleEvaluation;
use super::value::{JsError, JsErrorKind, JsResult, JsValue, Source};
use super::watchdog::ExecutionWatchdog;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Once, OnceLock};

static INITIALIZE_V8: Once = Once::new();
static V8_PLATFORM: OnceLock<v8::SharedRef<v8::Platform>> = OnceLock::new();

pub(in crate::engine::script) struct Context {
    // Stop cross-thread termination before the persistent handles and isolate are released.
    watchdog: ExecutionWatchdog,
    // Persistent handles must be released before their isolate.
    context: v8::Global<v8::Context>,
    isolate: v8::OwnedIsolate,
    next_module_promise: u64,
    module_promises: HashMap<u64, v8::Global<v8::Promise>>,
}

pub(in crate::engine::script) enum ModuleEvaluation {
    Missing(Vec<String>),
    Fulfilled,
    Rejected(String),
    Pending(u64),
}

impl Context {
    pub(in crate::engine::script) fn new(bridge: HostBridge) -> JsResult<Self> {
        initialize_v8();
        let mut isolate = v8::Isolate::new(v8::CreateParams::default());
        isolate.set_microtasks_policy(v8::MicrotasksPolicy::Explicit);
        isolate.set_host_initialize_import_meta_object_callback(
            super::modules::initialize_import_meta,
        );
        let context = {
            v8::scope!(let scope, &mut isolate);
            let context = v8::Context::new(scope, Default::default());
            context.set_slot(Rc::new(bridge));
            let scope = &mut v8::ContextScope::new(scope, context);
            install_host_call(scope, context)?;
            v8::Global::new(scope, context)
        };
        let watchdog = ExecutionWatchdog::new(isolate.thread_safe_handle())?;
        // rusty_v8 enters new isolates for their full lifetime. Breeze retains multiple document
        // realms on one renderer thread, so execution enters only the isolate it is about to use.
        unsafe { isolate.exit() };
        Ok(Self {
            watchdog,
            context,
            isolate,
            next_module_promise: 1,
            module_promises: HashMap::new(),
        })
    }

    pub(in crate::engine::script) fn eval(&mut self, source: Source) -> JsResult<JsValue> {
        let context = self.context.clone();
        self.watchdog.run(&mut self.isolate, |isolate| {
            v8::scope!(let scope, isolate);
            let context = v8::Local::new(scope, &context);
            let scope = &mut v8::ContextScope::new(scope, context);
            v8::tc_scope!(let tc, scope);
            let code = v8::String::new(tc, &source.code)
                .ok_or_else(|| allocation_error("script source"))?;
            let resource_name = source
                .path
                .as_deref()
                .and_then(std::path::Path::to_str)
                .unwrap_or("<script>");
            let resource_name =
                v8::String::new(tc, resource_name).ok_or_else(|| allocation_error("script URL"))?;
            let origin = v8::ScriptOrigin::new(
                tc,
                resource_name.into(),
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
            let script = v8::Script::compile(tc, code, Some(&origin))
                .ok_or_else(|| caught_error(tc, "compile JavaScript"))?;
            let value = script
                .run(tc)
                .ok_or_else(|| caught_error(tc, "evaluate JavaScript"))?;
            value_from_v8(tc, value)
        })
    }

    pub(in crate::engine::script) fn run_jobs(&mut self) -> JsResult<()> {
        self.watchdog.run(&mut self.isolate, |isolate| {
            isolate.perform_microtask_checkpoint();
            Ok(())
        })
    }

    pub(in crate::engine::script) fn call_global(
        &mut self,
        name: &str,
        arguments: &[JsValue],
    ) -> JsResult<JsValue> {
        let context = self.context.clone();
        self.watchdog.run(&mut self.isolate, |isolate| {
            v8::scope!(let scope, isolate);
            let context = v8::Local::new(scope, &context);
            let scope = &mut v8::ContextScope::new(scope, context);
            v8::tc_scope!(let tc, scope);
            let key = v8::String::new(tc, name).ok_or_else(|| allocation_error("function name"))?;
            let value = context
                .global(tc)
                .get(tc, key.into())
                .ok_or_else(|| caught_error(tc, "read global function"))?;
            let function = v8::Local::<v8::Function>::try_from(value).map_err(|_| JsError {
                kind: JsErrorKind::Type,
                message: format!("{name} hook is unavailable"),
            })?;
            let values = arguments
                .iter()
                .map(|value| value_to_v8(tc, value))
                .collect::<JsResult<Vec<_>>>()?;
            let receiver: v8::Local<v8::Value> = context.global(tc).into();
            let result = function
                .call(tc, receiver, &values)
                .ok_or_else(|| caught_error(tc, &format!("call {name}")))?;
            value_from_v8(tc, result)
        })
    }

    pub(in crate::engine::script) fn initialize_iframe_realm(
        &mut self,
        bootstrap: &str,
    ) -> JsResult<()> {
        let parent = self.context.clone();
        self.watchdog.run(&mut self.isolate, |isolate| {
            v8::scope!(let scope, isolate);
            let parent = v8::Local::new(scope, &parent);
            let iframe = v8::Context::new(scope, Default::default());
            // The synthetic iframe is same-origin with its owning document. V8 otherwise gives
            // every Context a distinct token and rejects WindowProxy access as "no access".
            iframe.set_security_token(parent.get_security_token(scope));
            let bridge = parent.get_slot::<HostBridge>().ok_or_else(|| JsError {
                kind: JsErrorKind::Type,
                message: "browser host bridge is unavailable".into(),
            })?;
            iframe.set_slot(bridge);
            {
                let scope = &mut v8::ContextScope::new(scope, iframe);
                install_host_call(scope, iframe)?;
                let source = v8::String::new(scope, bootstrap)
                    .ok_or_else(|| allocation_error("iframe bootstrap"))?;
                let script = v8::Script::compile(scope, source, None).ok_or_else(|| JsError {
                    kind: JsErrorKind::Error,
                    message: "compile iframe browser bindings".into(),
                })?;
                script.run(scope).ok_or_else(|| JsError {
                    kind: JsErrorKind::Error,
                    message: "evaluate iframe browser bindings".into(),
                })?;
            }
            let scope = &mut v8::ContextScope::new(scope, parent);
            let key = v8::String::new(scope, "__iframeWindow")
                .ok_or_else(|| allocation_error("iframe global name"))?;
            parent
                .global(scope)
                .set(scope, key.into(), iframe.global(scope).into())
                .filter(|set| *set)
                .ok_or_else(|| JsError {
                    kind: JsErrorKind::Error,
                    message: "expose iframe JavaScript realm".into(),
                })?;
            Ok(())
        })
    }

    pub(in crate::engine::script) fn evaluate_module(
        &mut self,
        root_url: &str,
        root_source: &str,
        sources: &HashMap<String, String>,
    ) -> JsResult<ModuleEvaluation> {
        let context = self.context.clone();
        let evaluation = self.watchdog.run(&mut self.isolate, |isolate| {
            super::modules::evaluate(isolate, &context, root_url, root_source, sources)
        })?;
        match evaluation {
            EngineModuleEvaluation::Missing(urls) => Ok(ModuleEvaluation::Missing(urls)),
            EngineModuleEvaluation::Fulfilled => Ok(ModuleEvaluation::Fulfilled),
            EngineModuleEvaluation::Rejected(error) => Ok(ModuleEvaluation::Rejected(error)),
            EngineModuleEvaluation::Pending(promise) => {
                let id = self.next_module_promise;
                self.next_module_promise = id.checked_add(1).ok_or_else(|| JsError {
                    kind: JsErrorKind::Range,
                    message: "module Promise identifiers were exhausted".into(),
                })?;
                self.module_promises.insert(id, promise);
                Ok(ModuleEvaluation::Pending(id))
            }
        }
    }

    pub(in crate::engine::script) fn track_module_promise(
        &mut self,
        promise_id: u64,
        operation: &str,
        completion_id: u32,
    ) -> JsResult<()> {
        let promise = self
            .module_promises
            .remove(&promise_id)
            .ok_or_else(|| JsError {
                kind: JsErrorKind::Type,
                message: "module Promise is unavailable".into(),
            })?;
        let property = format!("__breezeModulePromise{promise_id}");
        {
            let context = self.context.clone();
            self.watchdog.run(&mut self.isolate, |isolate| {
                v8::scope!(let scope, isolate);
                let context = v8::Local::new(scope, &context);
                let scope = &mut v8::ContextScope::new(scope, context);
                let key = v8::String::new(scope, &property)
                    .ok_or_else(|| allocation_error("module Promise name"))?;
                let promise = v8::Local::new(scope, promise);
                context
                    .global(scope)
                    .set(scope, key.into(), promise.into())
                    .filter(|set| *set)
                    .ok_or_else(|| JsError {
                        kind: JsErrorKind::Error,
                        message: "expose pending module Promise".into(),
                    })?;
                Ok(())
            })?;
        }
        let property = serde_json::to_string(&property).map_err(|error| JsError {
            kind: JsErrorKind::Error,
            message: error.to_string(),
        })?;
        let operation = serde_json::to_string(operation).map_err(|error| JsError {
            kind: JsErrorKind::Error,
            message: error.to_string(),
        })?;
        self.eval(Source::from_bytes(format!(
            "globalThis[{property}].then(\
                () => __hostCall({operation}, {completion_id}, true, ''),\
                reason => __hostCall({operation}, {completion_id}, false, String(reason))\
             ).finally(() => delete globalThis[{property}]);"
        )))?;
        Ok(())
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        // OwnedIsolate::drop balances one entry. Restore that invariant after Breeze's explicit
        // per-operation entry guards have all exited.
        unsafe { self.isolate.enter() };
    }
}

fn initialize_v8() {
    INITIALIZE_V8.call_once(|| {
        let platform = v8::new_default_platform(0, false).make_shared();
        v8::V8::initialize_platform(platform.clone());
        v8::V8::initialize();
        V8_PLATFORM
            .set(platform)
            .unwrap_or_else(|_| unreachable!("V8 platform initialized once"));
    });
}

fn caught_error(
    scope: &mut v8::PinnedRef<'_, v8::TryCatch<v8::HandleScope>>,
    action: &str,
) -> JsError {
    let exception = scope.exception();
    let detail = exception
        .and_then(|exception| exception.to_string(scope))
        .map(|message| message.to_rust_string_lossy(scope))
        .unwrap_or_else(|| action.to_string());
    let location = exception.and_then(|exception| {
        let message = v8::Exception::create_message(scope, exception);
        let resource = message
            .get_script_resource_name(scope)?
            .to_string(scope)?
            .to_rust_string_lossy(scope);
        if resource.is_empty() {
            return None;
        }
        Some(format!(
            "{resource}:{}:{}",
            message.get_line_number(scope).unwrap_or_default(),
            message.get_start_column().saturating_add(1)
        ))
    });
    JsError {
        kind: JsErrorKind::Error,
        message: location.map_or(detail.clone(), |location| format!("{detail} at {location}")),
    }
}

fn allocation_error(value: &str) -> JsError {
    JsError {
        kind: JsErrorKind::Range,
        message: format!("V8 could not allocate {value}"),
    }
}
