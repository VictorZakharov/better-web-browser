use super::value::{JsError, JsErrorKind, JsResult};
use crate::engine::script::module_loader::resolve_specifier;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;

pub(super) enum EngineModuleEvaluation {
    Missing(Vec<String>),
    Fulfilled,
    Rejected(String),
    Pending(v8::Global<v8::Promise>),
}

#[derive(Default)]
struct ModuleRegistry {
    by_url: RefCell<HashMap<String, v8::Global<v8::Module>>>,
    url_by_script_id: RefCell<HashMap<i32, String>>,
}

pub(super) fn evaluate(
    isolate: &mut v8::OwnedIsolate,
    persistent_context: &v8::Global<v8::Context>,
    root_url: &str,
    root_source: &str,
    loaded_sources: &HashMap<String, String>,
) -> JsResult<EngineModuleEvaluation> {
    let context = persistent_context.clone();
    v8::scope!(let scope, isolate);
    let context = v8::Local::new(scope, &context);
    let scope = &mut v8::ContextScope::new(scope, context);
    v8::tc_scope!(let tc, scope);

    let mut sources = loaded_sources.clone();
    sources.insert(root_url.to_string(), root_source.to_string());
    let registry = Rc::new(ModuleRegistry::default());
    let mut queue = VecDeque::from([root_url.to_string()]);
    let mut queued = HashSet::from([root_url.to_string()]);
    let mut missing = Vec::new();

    while let Some(url) = queue.pop_front() {
        if registry.by_url.borrow().contains_key(&url) {
            continue;
        }
        let source = sources.get(&url).expect("queued module source is present");
        let module = compile_module(tc, &url, source)?;
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
            let dependency = resolve_specifier(&url, &specifier).map_err(type_error)?;
            if !sources.contains_key(&dependency) {
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
    context.set_slot(Rc::clone(&registry));
    let root = registry
        .by_url
        .borrow()
        .get(root_url)
        .cloned()
        .ok_or_else(|| type_error("root module was not compiled"))?;
    let root = v8::Local::new(tc, root);
    root.instantiate_module(tc, resolve_module)
        .filter(|instantiated| *instantiated)
        .ok_or_else(|| caught_error(tc, "instantiate module graph"))?;
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

fn resolve_module<'s>(
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
    let url = match resolve_specifier(&base, &specifier) {
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
    let Some(key) = v8::String::new(scope, "url") else {
        return;
    };
    let Some(value) = v8::String::new(scope, &url) else {
        return;
    };
    let _ = meta.create_data_property(scope, key.into(), value.into());
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
