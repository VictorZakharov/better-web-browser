//! Static module-graph loading and asynchronous evaluation for dedicated workers.

use super::module_loader::WebModuleLoader;
use super::worker_host::WorkerHostState;
use super::*;

pub(super) fn evaluate(
    context: &mut Context,
    host: &Rc<RefCell<WorkerHostState>>,
    module_loader: &Rc<WebModuleLoader>,
    total_script_bytes: &mut usize,
    source_url: &str,
    source: &str,
) -> Result<(), String> {
    for _ in 0..MAX_DYNAMIC_SCRIPTS {
        let missing = match context
            .evaluate_module(source_url, source, &module_loader.sources())
            .map_err(|error| error.to_string())?
        {
            ModuleEvaluation::Missing(missing) => missing,
            ModuleEvaluation::Fulfilled => return Ok(()),
            ModuleEvaluation::Rejected(reason) => return Err(reason),
            ModuleEvaluation::Pending(promise) => {
                host.borrow_mut().module_evaluation_pending = true;
                context
                    .track_module_promise(promise, "workerModuleComplete", 0)
                    .map_err(|error| error.to_string())?;
                return Ok(());
            }
        };
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
