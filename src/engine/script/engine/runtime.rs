use super::agent::Agent;
use super::bridge::{HostBridge, install_host_call, value_from_v8, value_to_v8};
use super::modules::EngineModuleEvaluation;
use super::value::{JsError, JsErrorKind, JsResult, JsValue, Source};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
mod initialization;
pub(crate) use initialization::initialize_v8;
mod diagnostics;
mod dynamic_imports;
mod errors;
mod frames;
use errors::{allocation_error, caught_error};
#[cfg(all(test, windows))]
mod gpu_task_tests;
mod gpu_tasks;
mod hooks;
mod module_preparation;
mod platform_tasks;
mod task_boundary;
#[cfg(all(test, windows))]
mod webgl2_bindings_tests;
#[cfg(all(test, windows))]
mod webgl2_data_bindings_tests;
#[cfg(all(test, windows))]
mod webgl2_framebuffer_bindings_tests;
#[cfg(all(test, windows))]
mod webgl2_image_bindings_tests;
#[cfg(all(test, windows))]
mod webgl2_lifecycle_bindings_tests;
#[cfg(all(test, windows))]
mod webgl2_multisample_bindings_tests;
#[cfg(all(test, windows))]
mod webgl2_name_bindings_tests;
#[cfg(all(test, windows))]
mod webgl2_norm16_bindings_tests;
#[cfg(all(test, windows))]
mod webgl2_pixel_bindings_tests;
#[cfg(all(test, windows))]
mod webgl2_precise_bitmap_tests;
#[cfg(all(test, windows))]
mod webgl2_precise_image_tests;
#[cfg(all(test, windows))]
mod webgl2_sync_bindings_tests;
#[cfg(all(test, windows))]
mod webgl2_texture_bindings_tests;
#[cfg(all(test, windows))]
mod webgl2_uniform_bindings_tests;
#[cfg(test)]
mod webgl2_view_transform_tests;
#[cfg(all(test, windows))]
mod webgl_argument_plan_tests;
#[cfg(all(test, windows))]
mod webgl_buffer_admission_tests;
#[cfg(all(test, windows))]
mod webgl_direct_uniform_tests;
#[cfg(all(test, windows))]
mod webgl_numeric_command_tests;
#[cfg(all(test, windows))]
mod webgl_owned_copy_tests;
#[cfg(all(test, windows))]
mod webgl_owned_typed_copy_tests;
#[cfg(all(test, windows))]
mod webgl_packet_admission_tests;
#[cfg(all(test, windows))]
mod webgl_packet_order_tests;
#[cfg(all(test, windows))]
mod webgl_readback_pixel_tests;
#[cfg(all(test, windows))]
mod webgl_reflection_bindings_tests;
#[cfg(all(test, windows))]
mod webgl_restoration_policy_tests;
#[cfg(all(test, windows))]
mod webgl_video_source_tests;
mod worker_packets;

pub(in crate::engine::script) struct Context {
    // Persistent handles must be released before their isolate.
    context: v8::Global<v8::Context>,
    private_hooks: HashMap<String, v8::Global<v8::Function>>,
    worker_packets: Rc<super::worker_packets::State>,
    imports: Rc<super::dynamic_imports::Imports>,
    _frames: Rc<super::frames::FrameTree>,
    gpu_host: std::rc::Weak<HostBridge>,
    next_module_promise: u64,
    module_promises: HashMap<u64, v8::Global<v8::Promise>>,
    agent: Rc<RefCell<Agent>>,
}

pub(in crate::engine::script) enum ModuleEvaluation {
    Missing(Vec<String>),
    Fulfilled,
    Rejected(String),
    Pending(u64),
}

impl Context {
    pub(in crate::engine::script) fn new(bridge: HostBridge) -> JsResult<Self> {
        Self::with_cancellation(bridge, super::watchdog::ScriptCancellation::default())
    }

    pub(in crate::engine::script) fn cancellation(&self) -> super::watchdog::ScriptCancellation {
        self.agent.borrow().cancellation()
    }

    pub(in crate::engine::script) fn with_cancellation(
        bridge: HostBridge,
        cancellation: super::watchdog::ScriptCancellation,
    ) -> JsResult<Self> {
        let is_window = matches!(bridge, HostBridge::Document(_));
        initialize_v8();
        let mut isolate = v8::Isolate::new(super::stack_boundary::create_params()?);
        isolate.set_microtasks_policy(v8::MicrotasksPolicy::Explicit);
        isolate.set_allow_wasm_code_generation_callback(super::policy::allow_wasm);
        isolate.set_host_import_module_dynamically_callback(super::dynamic_imports::request);
        let imports = Rc::new(super::dynamic_imports::Imports::default());
        let frames = Rc::new(super::frames::FrameTree::default());
        let worker_packets = Rc::new(super::worker_packets::State::default());
        let bridge = Rc::new(bridge);
        let gpu_host = Rc::downgrade(&bridge);
        isolate.set_host_initialize_import_meta_object_callback(
            super::modules::initialize_import_meta,
        );
        // Admit the control before creating persistent handles. If admission
        // fails, no Global can outlive the consumed isolate.
        let mut agent = Agent::with_cancellation(isolate, cancellation)?;
        let context = agent.run(|isolate| {
            v8::scope!(let scope, isolate);
            let global_template = initialization::global_template(scope, is_window);
            let context = v8::Context::new(
                scope,
                v8::ContextOptions {
                    global_template: Some(global_template),
                    ..Default::default()
                },
            );
            context.set_slot(bridge);
            context.set_slot(Rc::clone(&imports));
            context.set_slot(Rc::clone(&worker_packets));
            super::frames::register(context, &frames);
            let scope = &mut v8::ContextScope::new(scope, context);
            super::frames::register_document(scope, context, &frames);
            install_host_call(scope, context)?;
            Ok(v8::Global::new(scope, context))
        })?;
        let agent = Rc::new(RefCell::new(agent));
        Ok(Self {
            context,
            private_hooks: HashMap::new(),
            worker_packets,
            imports,
            _frames: frames,
            gpu_host,
            next_module_promise: 1,
            module_promises: HashMap::new(),
            agent,
        })
    }

    pub(in crate::engine::script) fn eval(&mut self, source: Source) -> JsResult<JsValue> {
        let context = self.context.clone();
        self.agent.borrow_mut().run_sampled(|isolate| {
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

    pub(in crate::engine::script) fn call_global(
        &mut self,
        name: &str,
        arguments: &[JsValue],
    ) -> JsResult<JsValue> {
        let context = self.context.clone();
        let captured = self.private_hooks.get(name).cloned();
        // Skip repeated profiler setup for captured state/presentation hooks.
        // This is a diagnostic selection, not proof that getters cannot run
        // author code: every call still has its ordinary execution watchdog.
        // Uncaptured replacements remain part of source sampling.
        let sample = captured.is_none()
            || !matches!(
                name,
                "__setCurrentScript" | "__setHistoryMetrics" | "__takeCanvasPresentation"
            );
        self.agent
            .borrow_mut()
            .run_with_sampling(sample, |isolate| {
                v8::scope!(let scope, isolate);
                let context = v8::Local::new(scope, &context);
                let scope = &mut v8::ContextScope::new(scope, context);
                v8::tc_scope!(let tc, scope);
                let key =
                    v8::String::new(tc, name).ok_or_else(|| allocation_error("function name"))?;
                let value = if let Some(captured) = &captured {
                    v8::Local::new(tc, captured).into()
                } else {
                    context
                        .global(tc)
                        .get(tc, key.into())
                        .ok_or_else(|| caught_error(tc, "read global function"))?
                };
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

    pub(in crate::engine::script) fn evaluate_module(
        &mut self,
        root_url: &str,
        root_source: &str,
        sources: &HashMap<String, String>,
    ) -> JsResult<ModuleEvaluation> {
        let context = self.context.clone();
        let evaluation = self.agent.borrow_mut().run_sampled(|isolate| {
            super::modules::evaluate(isolate, &context, root_url, root_source, sources, false)
        })?;
        match evaluation {
            EngineModuleEvaluation::Ready => unreachable!("evaluation requested"),
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
        let hook = self
            .private_hooks
            .get("__moduleCompletionHandlers")
            .cloned()
            .ok_or_else(|| allocation_error("private module completion hook"))?;
        // Keep the internal evaluation Promise native. V8's public Then API
        // performs settlement without author constructor/species access, unlike
        // even a captured Promise.prototype.then. The private factory supplies
        // bridge-owning callbacks, never an author-global Promise or fallback.
        let context = self.context.clone();
        self.agent.borrow_mut().run(|isolate| {
            v8::scope!(let scope, isolate);
            let context = v8::Local::new(scope, &context);
            let scope = &mut v8::ContextScope::new(scope, context);
            v8::tc_scope!(let tc, scope);
            let hook = v8::Local::new(tc, &hook);
            let promise = v8::Local::new(tc, &promise);
            let operation = v8::String::new(tc, operation)
                .ok_or_else(|| allocation_error("module completion operation"))?;
            let completion_id = v8::Integer::new_from_unsigned(tc, completion_id);
            let arguments: [v8::Local<v8::Value>; 2] = [operation.into(), completion_id.into()];
            let receiver: v8::Local<v8::Value> = context.global(tc).into();
            let handlers = hook
                .call(tc, receiver, &arguments)
                .ok_or_else(|| caught_error(tc, "construct module completion handlers"))?;
            let handlers = v8::Local::<v8::Array>::try_from(handlers)
                .map_err(|_| allocation_error("module completion handlers"))?;
            if handlers.length() != 2 {
                return Err(allocation_error("module completion handler count"));
            }
            let fulfilled = handlers
                .get_index(tc, 0)
                .and_then(|value| v8::Local::<v8::Function>::try_from(value).ok())
                .ok_or_else(|| allocation_error("module fulfillment handler"))?;
            let rejected = handlers
                .get_index(tc, 1)
                .and_then(|value| v8::Local::<v8::Function>::try_from(value).ok())
                .ok_or_else(|| allocation_error("module rejection handler"))?;
            promise
                .then2(tc, fulfilled, rejected)
                .ok_or_else(|| caught_error(tc, "observe module evaluation"))?;
            Ok(())
        })
    }
}
