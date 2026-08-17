//! Static module-graph loading and asynchronous evaluation for dedicated workers.

use super::module_loader::{WebModuleLoader, parse_module};
use super::worker_host::{WorkerHostLink, WorkerHostState};
use super::*;
use boa_engine::builtins::promise::PromiseState;

pub(super) fn evaluate(
    context: &mut Context,
    host: &Rc<RefCell<WorkerHostState>>,
    module_loader: &Rc<WebModuleLoader>,
    total_script_bytes: &mut usize,
    source_url: &str,
    source: &str,
) -> Result<(), String> {
    for _ in 0..MAX_DYNAMIC_SCRIPTS {
        let root = parse_module(source, source_url, context).map_err(|error| error.to_string())?;
        module_loader.begin_attempt(source_url, root.clone());
        let promise = root.load_link_evaluate(context);
        context.run_jobs().map_err(|error| error.to_string())?;
        let missing = module_loader.take_missing();
        if missing.is_empty() {
            return match promise.state() {
                PromiseState::Rejected(reason) => Err(reason.display().to_string()),
                PromiseState::Fulfilled(_) => Ok(()),
                PromiseState::Pending => {
                    host.borrow_mut().module_evaluation_pending = true;
                    track_pending(&promise, context);
                    Ok(())
                }
            };
        }
        let source_loader = Rc::clone(host).borrow().source_loader.clone();
        let mut loaded = false;
        for url in missing {
            let code = source_loader(&url, ScriptKind::Module)
                .map_err(|error| format!("{url}: {error}"))?;
            if total_script_bytes.saturating_add(code.len()) > MAX_SCRIPT_BYTES {
                return Err("Worker module graph exceeded the JavaScript byte limit".into());
            }
            if module_loader.add_source(url, code.clone()) {
                *total_script_bytes += code.len();
            }
            loaded = true;
        }
        if !loaded {
            break;
        }
    }
    Err("Worker module graph exceeded its dependency limit".into())
}

fn track_pending(promise: &boa_engine::object::builtins::JsPromise, context: &mut Context) {
    let fulfilled = NativeFunction::from_copy_closure(|_, _, context| complete(context, Ok(())))
        .to_js_function(context.realm());
    let rejected = NativeFunction::from_copy_closure(|_, args, context| {
        let reason = args
            .first()
            .map(|value| value.display().to_string())
            .unwrap_or_else(|| "Worker module evaluation rejected".to_string());
        complete(context, Err(reason))
    })
    .to_js_function(context.realm());
    let _ = promise.then(Some(fulfilled), Some(rejected), context);
}

fn complete(context: &mut Context, result: Result<(), String>) -> JsResult<JsValue> {
    if let Some(host) = context
        .get_data::<WorkerHostLink>()
        .and_then(|link| link.0.upgrade())
    {
        host.borrow_mut().module_evaluation_completion = Some(result);
    }
    Ok(JsValue::undefined())
}
