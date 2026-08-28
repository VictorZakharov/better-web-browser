//! Standards-oriented ECMAScript module graph loading and evaluation.

use super::module_loader::parse_module;
use super::*;
use boa_engine::builtins::promise::PromiseState;

pub(super) fn evaluate_module(
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    outcome: &mut ScriptOutcome,
    script: &ScriptInput,
    dispatch_load: bool,
    source_loader: &mut Option<&mut DynamicScriptLoader<'_>>,
    total_bytes: &mut usize,
) -> bool {
    if let Err(error) = context.eval(Source::from_bytes("document.__setCurrentScript(0);")) {
        outcome.errors.push(format!(
            "{}: clear document.currentScript: {error}",
            script.source_url
        ));
    }

    let started = Instant::now();
    let loader = Rc::clone(&host.borrow().module_loader);
    let mut result = Err("module graph could not be loaded".to_string());
    let mut pending_promise = None;
    for _ in 0..MAX_DYNAMIC_SCRIPTS {
        let root = match parse_module(&script.code, &script.source_url, context) {
            Ok(module) => module,
            Err(error) => {
                result = Err(error.to_string());
                break;
            }
        };
        loader.begin_attempt(&script.source_url, root.clone());
        let promise = root.load_link_evaluate(context);
        if let Err(error) = context.run_jobs() {
            result = Err(format!("module promise job: {error}"));
            break;
        }
        let missing = loader.take_missing();
        if missing.is_empty() {
            result = match promise.state() {
                PromiseState::Rejected(reason) => Err(reason.display().to_string()),
                PromiseState::Pending => {
                    pending_promise = Some(promise);
                    Ok(())
                }
                PromiseState::Fulfilled(_) => Ok(()),
            };
            break;
        }
        let Some(load) = source_loader.as_mut() else {
            result = Err(format!("module dependency is unavailable: {}", missing[0]));
            break;
        };
        let mut loaded_any = false;
        for url in missing {
            let source = match load(&url, ScriptKind::Module, script.fetch_options) {
                Ok(source) => source,
                Err(error) => {
                    result = Err(format!("{url}: module could not be loaded: {error}"));
                    continue;
                }
            };
            if total_bytes.saturating_add(source.len()) > MAX_PAGE_SCRIPT_BYTES {
                result = Err(format!(
                    "{url}: module graph exceeds the {} MiB JavaScript limit",
                    MAX_PAGE_SCRIPT_BYTES / 1024 / 1024
                ));
                continue;
            }
            if loader.add_source(url, source.clone()) {
                *total_bytes += source.len();
            }
            loaded_any = true;
        }
        if !loaded_any {
            break;
        }
    }

    super::mutation_host::flush_document_write(&mut host.borrow_mut());
    let mut error = result.err();
    let pending = if error.is_none() {
        pending_promise.is_some_and(|promise| {
            if let Err(track_error) = super::module_lifecycle::track_pending(
                &promise,
                context,
                host,
                script,
                dispatch_load,
            ) {
                error = Some(track_error);
                false
            } else {
                true
            }
        })
    } else {
        false
    };
    let succeeded = error.is_none() && !pending;
    if succeeded {
        outcome.executed += 1;
        host.borrow_mut().executed += 1;
    } else if let Some(error) = &error {
        outcome
            .errors
            .push(format!("{}: {error}", script.source_url));
    }
    let elapsed = started.elapsed();
    if elapsed.as_millis() >= 1 {
        outcome.diagnostics.push(format!(
            "JavaScript module {:.3} ms: {}",
            elapsed.as_secs_f64() * 1_000.0,
            script.source_url
        ));
    }
    if dispatch_load && !pending {
        let event_type = if succeeded { "load" } else { "error" };
        let node_id = host.borrow_mut().id_for(&script.node);
        super::module_lifecycle::dispatch_script_event(
            context,
            outcome,
            node_id,
            event_type,
            &script.source_url,
        );
    }
    error.is_none()
}
