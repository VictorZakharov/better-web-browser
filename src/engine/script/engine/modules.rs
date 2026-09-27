use super::bridge::HostBridge;
use super::value::{JsError, JsErrorKind, JsResult};
use crate::engine::script::module_loader::resolve_specifier;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;

pub(super) enum EngineModuleEvaluation {
    Ready,
    Missing(Vec<String>),
    Fulfilled,
    Rejected(String),
    Pending(v8::Global<v8::Promise>),
}

#[derive(Default)]
pub(super) struct ModuleRegistry {
    pub by_url: RefCell<HashMap<String, v8::Global<v8::Module>>>,
    pub errors: RefCell<HashMap<String, v8::Global<v8::Value>>>,
    url_by_script_id: RefCell<HashMap<i32, String>>,
}

pub(super) fn evaluate(
    isolate: &mut v8::OwnedIsolate,
    persistent_context: &v8::Global<v8::Context>,
    root_url: &str,
    root_source: &str,
    loaded_sources: &HashMap<String, String>,
    prepare_only: bool,
) -> JsResult<EngineModuleEvaluation> {
    let context = persistent_context.clone();
    v8::scope!(let scope, isolate);
    let context = v8::Local::new(scope, &context);
    let scope = &mut v8::ContextScope::new(scope, context);
    v8::tc_scope!(let tc, scope);

    // A realm owns its module map. Retain compiled records across fetch completions and roots;
    // re-creating this map would recompile graphs and evaluate shared dependencies twice.
    let registry = context.get_slot::<ModuleRegistry>().unwrap_or_else(|| {
        let registry = Rc::new(ModuleRegistry::default());
        context.set_slot(Rc::clone(&registry));
        registry
    });
    let mut queue = VecDeque::from([root_url.to_string()]);
    let mut queued = HashSet::from([root_url.to_string()]);
    let mut missing = Vec::new();

    while let Some(url) = queue.pop_front() {
        let imports = context
            .get_slot::<super::dynamic_imports::Imports>()
            .unwrap();
        let inherited = imports.origins.borrow().get(root_url).cloned();
        if let Some(mut origin) = inherited {
            if url != root_url {
                origin.base = url.clone();
            }
            imports
                .origins
                .borrow_mut()
                .entry(url.clone())
                .or_insert(origin);
        }
        let cached_error = registry.errors.borrow().get(&url).cloned();
        if let Some(error) = cached_error {
            let error = v8::Local::new(tc, error);
            registry
                .errors
                .borrow_mut()
                .insert(root_url.into(), v8::Global::new(tc, error));
            return Err(type_error(error.to_rust_string_lossy(tc)));
        }
        let cached = registry.by_url.borrow().get(&url).cloned();
        let module = if let Some(cached) = cached {
            v8::Local::new(tc, cached)
        } else {
            let source = if url == root_url {
                root_source
            } else {
                loaded_sources
                    .get(&url)
                    .expect("queued module source is present")
            };
            match compile_module(tc, &url, source) {
                Ok(module) => module,
                Err(error) => {
                    if let Some(exception) = tc.exception() {
                        registry
                            .errors
                            .borrow_mut()
                            .insert(root_url.into(), v8::Global::new(tc, exception));
                        registry
                            .errors
                            .borrow_mut()
                            .insert(url.clone(), v8::Global::new(tc, exception));
                    }
                    return Err(error);
                }
            }
        };
        let script_id = module
            .script_id()
            .ok_or_else(|| type_error("compiled source-text module has no script ID"))?;
        registry
            .url_by_script_id
            .borrow_mut()
            .insert(script_id, url.clone());
        registry
            .by_url
            .borrow_mut()
            .insert(url.clone(), v8::Global::new(tc, module));

        let requests = module.get_module_requests();
        for index in 0..requests.length() {
            let request = requests
                .get(tc, index)
                .and_then(|request| v8::Local::<v8::ModuleRequest>::try_from(request).ok())
                .ok_or_else(|| type_error("V8 returned an invalid module request"))?;
            if request.get_phase() != v8::ModuleImportPhase::kEvaluation {
                return Err(type_error("source-phase module imports are not supported"));
            }
            let specifier = request.get_specifier().to_rust_string_lossy(tc);
            if request.get_import_attributes().length() != 0 {
                return Err(type_error("import attributes are not supported"));
            }
            let base = imports
                .origins
                .borrow()
                .get(&url)
                .map(|origin| origin.base.clone())
                .unwrap_or_else(|| url.clone());
            let dependency = resolve_in_context(context, &base, &specifier).map_err(type_error)?;
            if dependency != root_url
                && !loaded_sources.contains_key(&dependency)
                && !registry.by_url.borrow().contains_key(&dependency)
            {
                if !missing.contains(&dependency) {
                    missing.push(dependency);
                }
            } else if queued.insert(dependency.clone()) {
                queue.push_back(dependency);
            }
        }
    }

    if !missing.is_empty() {
        return Ok(EngineModuleEvaluation::Missing(missing));
    }
    if prepare_only {
        return Ok(EngineModuleEvaluation::Ready);
    }
    let root = registry
        .by_url
        .borrow()
        .get(root_url)
        .cloned()
        .ok_or_else(|| type_error("root module was not compiled"))?;
    let root = v8::Local::new(tc, root);
    if root.get_status() == v8::ModuleStatus::Uninstantiated {
        root.instantiate_module(tc, resolve_module)
            .filter(|instantiated| *instantiated)
            .ok_or_else(|| caught_error(tc, "instantiate module graph"))?;
    }
    let promise = root
        .evaluate(tc)
        .and_then(|value| v8::Local::<v8::Promise>::try_from(value).ok())
        .ok_or_else(|| caught_error(tc, "evaluate module graph"))?;
    tc.perform_microtask_checkpoint();
    Ok(match promise.state() {
        v8::PromiseState::Fulfilled => EngineModuleEvaluation::Fulfilled,
        v8::PromiseState::Rejected => EngineModuleEvaluation::Rejected(
            promise
                .result(tc)
                .to_string(tc)
                .map(|value| value.to_rust_string_lossy(tc))
                .unwrap_or_else(|| "module evaluation rejected".into()),
        ),
        v8::PromiseState::Pending => EngineModuleEvaluation::Pending(v8::Global::new(tc, promise)),
    })
}

fn compile_module<'s>(
    scope: &mut v8::PinnedRef<'s, v8::TryCatch<v8::HandleScope>>,
    url: &str,
    source: &str,
) -> JsResult<v8::Local<'s, v8::Module>> {
    let code = v8::String::new(scope, source)
        .ok_or_else(|| range_error("V8 could not allocate module source"))?;
    let resource_name = v8::String::new(scope, url)
        .ok_or_else(|| range_error("V8 could not allocate module URL"))?;
    let origin = v8::ScriptOrigin::new(
        scope,
        resource_name.into(),
        0,
        0,
        false,
        0,
        None,
        false,
        false,
        true,
        None,
    );
    let mut source = v8::script_compiler::Source::new(code, Some(&origin));
    v8::script_compiler::compile_module(scope, &mut source)
        .ok_or_else(|| caught_error(scope, "compile module"))
}

pub(super) fn resolve_module<'s>(
    context: v8::Local<'s, v8::Context>,
    specifier: v8::Local<'s, v8::String>,
    _import_attributes: v8::Local<'s, v8::FixedArray>,
    referrer: v8::Local<'s, v8::Module>,
) -> Option<v8::Local<'s, v8::Module>> {
    v8::callback_scope!(unsafe scope, context);
    let registry = context.get_slot::<ModuleRegistry>()?;
    let base = registry
        .url_by_script_id
        .borrow()
        .get(&referrer.script_id()?)
        .cloned()?;
    let specifier = specifier.to_rust_string_lossy(scope);
    let base = context
        .get_slot::<super::dynamic_imports::Imports>()
        .and_then(|imports| {
            imports
                .origins
                .borrow()
                .get(&base)
                .map(|origin| origin.base.clone())
        })
        .unwrap_or(base);
    let url = match resolve_in_context(context, &base, &specifier) {
        Ok(url) => url,
        Err(error) => {
            let message = v8::String::new(scope, &error)?;
            let exception = v8::Exception::type_error(scope, message);
            scope.throw_exception(exception);
            return None;
        }
    };
    let module = registry.by_url.borrow().get(&url).cloned()?;
    Some(v8::Local::new(scope, module))
}

/// Both graph discovery and V8 instantiation must consult the document's same
/// import map, otherwise an apparently fetched dependency can fail to link.
fn resolve_in_context(
    context: v8::Local<'_, v8::Context>,
    base: &str,
    specifier: &str,
) -> Result<String, String> {
    if let Some(bridge) = context.get_slot::<HostBridge>()
        && let HostBridge::Document(host) = &*bridge
        && let Some(host) = host.upgrade()
    {
        return host.borrow().module_loader.resolve(base, specifier);
    }
    resolve_specifier(base, specifier)
}

pub(super) extern "C" fn initialize_import_meta(
    context: v8::Local<v8::Context>,
    module: v8::Local<v8::Module>,
    meta: v8::Local<v8::Object>,
) {
    v8::callback_scope!(unsafe scope, context);
    let Some(registry) = context.get_slot::<ModuleRegistry>() else {
        return;
    };
    let Some(script_id) = module.script_id() else {
        return;
    };
    let Some(url) = registry.url_by_script_id.borrow().get(&script_id).cloned() else {
        return;
    };
    let url = context
        .get_slot::<super::dynamic_imports::Imports>()
        .and_then(|imports| {
            imports
                .origins
                .borrow()
                .get(&url)
                .map(|origin| origin.base.clone())
        })
        .unwrap_or(url);
    let Some(key) = v8::String::new(scope, "url") else {
        return;
    };
    let Some(value) = v8::String::new(scope, &url) else {
        return;
    };
    let _ = meta.create_data_property(scope, key.into(), value.into());
    let Some(resolve_key) = v8::String::new(scope, "resolve") else {
        return;
    };
    let Some(resolve) = v8::Function::builder(resolve_import_meta)
        .data(value.into())
        .constructor_behavior(v8::ConstructorBehavior::Throw)
        .build(scope)
    else {
        return;
    };
    let _ = meta.create_data_property(scope, resolve_key.into(), resolve.into());
}

/// `import.meta.resolve()` uses the referring module's URL, not the document URL.
/// Keep it on the same document import-map path as static and dynamic imports.
fn resolve_import_meta(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    mut result: v8::ReturnValue,
) {
    let Ok(base) = v8::Local::<v8::String>::try_from(args.data()) else {
        return;
    };
    let base = base.to_rust_string_lossy(scope);
    let Some(specifier) = args.get(0).to_string(scope) else {
        return;
    };
    let specifier = specifier.to_rust_string_lossy(scope);
    let url = resolve_in_context(scope.get_current_context(), &base, &specifier);
    match url {
        Ok(url) => {
            if let Some(url) = v8::String::new(scope, &url) {
                result.set(url.into());
            }
        }
        Err(error) => {
            if let Some(message) = v8::String::new(scope, &error) {
                let exception = v8::Exception::type_error(scope, message);
                scope.throw_exception(exception);
            }
        }
    }
}

fn caught_error(
    scope: &mut v8::PinnedRef<'_, v8::TryCatch<v8::HandleScope>>,
    fallback: &str,
) -> JsError {
    let message = scope
        .exception()
        .and_then(|exception| exception.to_string(scope))
        .map(|value| value.to_rust_string_lossy(scope))
        .unwrap_or_else(|| fallback.to_string());
    JsError {
        kind: JsErrorKind::Error,
        message,
    }
}

fn type_error(message: impl Into<String>) -> JsError {
    JsError {
        kind: JsErrorKind::Type,
        message: message.into(),
    }
}

fn range_error(message: impl Into<String>) -> JsError {
    JsError {
        kind: JsErrorKind::Range,
        message: message.into(),
    }
}
