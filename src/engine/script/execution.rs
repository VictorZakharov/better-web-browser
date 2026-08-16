//! Script evaluation and bounded event-loop settlement for an owned document realm.

use super::*;
pub fn execute(document: NodeRef, document_url: &str, scripts: &[ScriptInput]) -> ScriptOutcome {
    execute_impl(document, document_url, scripts, None)
}

pub fn execute_with_loader(
    document: NodeRef,
    document_url: &str,
    scripts: &[ScriptInput],
    dynamic_script_loader: &mut DynamicScriptLoader<'_>,
) -> ScriptOutcome {
    execute_impl(document, document_url, scripts, Some(dynamic_script_loader))
}

fn execute_impl(
    document: NodeRef,
    document_url: &str,
    scripts: &[ScriptInput],
    dynamic_script_loader: Option<&mut DynamicScriptLoader<'_>>,
) -> ScriptOutcome {
    if scripts.is_empty() {
        return ScriptOutcome::default();
    }
    ScriptRuntime::new(document, document_url)
        .execute_initial_with_loader(scripts, dynamic_script_loader)
}

pub(super) fn panic_detail(payload: Box<dyn std::any::Any + Send>) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|message| (*message).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown evaluator panic".to_string())
}

pub(super) fn stopped_runtime_outcome(detail: String) -> ScriptOutcome {
    ScriptOutcome {
        errors: vec![format!(
            "JavaScript runtime was stopped safely after an evaluator failure: {detail}"
        )],
        runtime_stopped: true,
        ..ScriptOutcome::default()
    }
}

pub(super) fn inactive_runtime_outcome() -> ScriptOutcome {
    ScriptOutcome {
        errors: vec!["JavaScript runtime is inactive because its document was cancelled".into()],
        runtime_stopped: true,
        ..ScriptOutcome::default()
    }
}

pub(super) fn lifecycle_error(message: &str) -> ScriptOutcome {
    ScriptOutcome {
        errors: vec![format!("JavaScript runtime lifecycle: {message}")],
        ..ScriptOutcome::default()
    }
}

pub(super) fn execute_inner(
    scripts: &[ScriptInput],
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    total_bytes: &mut usize,
    dynamic_script_loader: &mut Option<&mut DynamicScriptLoader<'_>>,
) -> ScriptOutcome {
    let mut outcome = ScriptOutcome::default();
    context
        .runtime_limits_mut()
        .set_loop_iteration_limit(MAX_LOOP_ITERATIONS);
    context.runtime_limits_mut().set_recursion_limit(128);

    if let Err(error) = context.register_global_builtin_callable(
        boa_engine::js_string!("__hostCall"),
        1,
        NativeFunction::from_fn_ptr(host_call),
    ) {
        outcome
            .errors
            .push(format!("initialize JavaScript host bridge: {error}"));
        return outcome;
    }

    let iframe_realm = match context.create_realm() {
        Ok(realm) => realm,
        Err(error) => {
            outcome
                .errors
                .push(format!("initialize iframe JavaScript realm: {error}"));
            return outcome;
        }
    };
    let parent_realm = context.enter_realm(iframe_realm);
    let iframe_bootstrap = context.eval(Source::from_bytes(IFRAME_REALM_BOOTSTRAP));
    let iframe_window = context.global_object();
    context.enter_realm(parent_realm);
    if let Err(error) = iframe_bootstrap {
        outcome
            .errors
            .push(format!("initialize iframe browser bindings: {error}"));
        return outcome;
    }
    if let Err(error) = context.register_global_property(
        boa_engine::js_string!("__iframeWindow"),
        iframe_window,
        Attribute::all(),
    ) {
        outcome
            .errors
            .push(format!("expose iframe JavaScript realm: {error}"));
        return outcome;
    }

    if let Err(error) = context.eval(Source::from_bytes(super::bootstrap::BROWSER_BOOTSTRAP)) {
        outcome
            .errors
            .push(format!("initialize browser bindings: {error}"));
        return outcome;
    }

    host.borrow_mut().begin_task();
    for script in scripts {
        if total_bytes.saturating_add(script.code.len()) > MAX_SCRIPT_BYTES {
            outcome.errors.push(format!(
                "{}: skipped because the page exceeds the {} MiB JavaScript limit",
                script.source_url,
                MAX_SCRIPT_BYTES / 1024 / 1024
            ));
            break;
        }
        *total_bytes += script.code.len();
        evaluate_script(context, host, &mut outcome, script, false);
        drain_dynamic_scripts(
            context,
            host,
            &mut outcome,
            dynamic_script_loader,
            total_bytes,
        );
    }

    // An async-only document still completes parsing before its first external script arrives.
    let finish_lifecycle =
        scripts.is_empty() || scripts.iter().any(|script| script.finish_lifecycle);
    let lifecycle = if finish_lifecycle {
        "document.__setCurrentScript(0); __finishDocument();"
    } else {
        "document.__setCurrentScript(0);"
    };
    if let Err(error) = context.eval(Source::from_bytes(lifecycle)) {
        outcome
            .errors
            .push(format!("finish document lifecycle: {error}"));
    }
    if let Err(error) = context.run_jobs() {
        outcome.errors.push(format!("finish promise jobs: {error}"));
    }
    drain_dynamic_scripts(
        context,
        host,
        &mut outcome,
        dynamic_script_loader,
        total_bytes,
    );
    for _ in 0..STARTUP_TIMER_PASSES {
        settle_startup_timer_slice(
            context,
            host,
            &mut outcome,
            dynamic_script_loader,
            total_bytes,
        );
    }

    append_timer_summary(host, &mut outcome);

    outcome
}

pub(super) fn execute_additional_inner(
    scripts: &[ScriptInput],
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    total_bytes: &mut usize,
    dynamic_script_loader: &mut Option<&mut DynamicScriptLoader<'_>>,
) -> ScriptOutcome {
    let mut outcome = ScriptOutcome::default();
    host.borrow_mut().begin_task();
    for script in scripts {
        if total_bytes.saturating_add(script.code.len()) > MAX_SCRIPT_BYTES {
            outcome.errors.push(format!(
                "{}: skipped because the page exceeds the {} MiB JavaScript limit",
                script.source_url,
                MAX_SCRIPT_BYTES / 1024 / 1024
            ));
            continue;
        }
        *total_bytes += script.code.len();
        evaluate_script(context, host, &mut outcome, script, true);
        drain_dynamic_scripts(
            context,
            host,
            &mut outcome,
            dynamic_script_loader,
            total_bytes,
        );
    }

    if let Err(error) = context.eval(Source::from_bytes("document.__setCurrentScript(0);")) {
        outcome
            .errors
            .push(format!("finish additional script task: {error}"));
    }
    if let Err(error) = context.run_jobs() {
        outcome
            .errors
            .push(format!("finish additional script promise jobs: {error}"));
    }
    drain_dynamic_scripts(
        context,
        host,
        &mut outcome,
        dynamic_script_loader,
        total_bytes,
    );
    append_timer_summary(host, &mut outcome);
    outcome
}

pub(super) fn append_timer_summary(host: &Rc<RefCell<HostState>>, outcome: &mut ScriptOutcome) {
    let timer_summary = host.borrow().timer_summary();
    if !timer_summary.is_empty() {
        outcome
            .diagnostics
            .push(format!("JavaScript timers after settling: {timer_summary}"));
    }
}

fn settle_startup_timer_slice(
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    outcome: &mut ScriptOutcome,
    dynamic_script_loader: &mut Option<&mut DynamicScriptLoader<'_>>,
    total_bytes: &mut usize,
) {
    settle_timer_slice(
        context,
        host,
        outcome,
        dynamic_script_loader,
        total_bytes,
        STARTUP_TIMER_SLICE,
        MAX_TIMER_CALLBACKS_PER_SLICE,
    );
}

pub(super) fn settle_timer_slice(
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    outcome: &mut ScriptOutcome,
    dynamic_script_loader: &mut Option<&mut DynamicScriptLoader<'_>>,
    total_bytes: &mut usize,
    advance: Duration,
    max_callbacks: usize,
) {
    let horizon = host.borrow().timers.now().saturating_add(advance);

    for _ in 0..max_callbacks {
        let timer_id = {
            let mut host = host.borrow_mut();
            let due = host.timers.next_due_time();
            due.filter(|due| *due <= horizon).and_then(|due| {
                host.timers.advance_to(due);
                host.take_ready_timer()
            })
        };
        let Some(timer_id) = timer_id else {
            break;
        };

        host.borrow_mut().begin_task();
        let invocation = format!("__runTimer({timer_id});");
        if let Err(error) = context.eval(Source::from_bytes(&invocation)) {
            outcome
                .errors
                .push(format!("JavaScript timer {timer_id}: {error}"));
        }
        // HTML performs a microtask checkpoint after every task. Boa owns the Promise job queue,
        // so drain it here rather than once after a whole batch of timer callbacks.
        if let Err(error) = context.run_jobs() {
            outcome
                .errors
                .push(format!("JavaScript timer {timer_id} promise job: {error}"));
        }
        drain_dynamic_scripts(context, host, outcome, dynamic_script_loader, total_bytes);
    }

    host.borrow_mut().timers.advance_to(horizon);
}

fn evaluate_script(
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    outcome: &mut ScriptOutcome,
    script: &ScriptInput,
    dispatch_load: bool,
) -> bool {
    let node_id = host.borrow_mut().id_for(&script.node);
    let current_script = format!("document.__setCurrentScript({node_id});");
    if let Err(error) = context.eval(Source::from_bytes(&current_script)) {
        outcome.errors.push(format!(
            "{}: set document.currentScript: {error}",
            script.source_url
        ));
    }

    let script_started = Instant::now();
    let succeeded = match mutation_host::eval_with_writes(context, host, &script.code) {
        Ok(_) => {
            outcome.executed += 1;
            host.borrow_mut().executed += 1;
            if let Err(error) = context.run_jobs() {
                outcome
                    .errors
                    .push(format!("{}: promise job: {error}", script.source_url));
            }
            true
        }
        Err(error) => {
            outcome
                .errors
                .push(format!("{}: {error}", script.source_url));
            false
        }
    };
    let script_time = script_started.elapsed();
    if script_time.as_millis() >= 1 {
        outcome.diagnostics.push(format!(
            "JavaScript {:.3} ms: {}",
            script_time.as_secs_f64() * 1_000.0,
            script.source_url
        ));
    }

    if dispatch_load {
        let event_type = if succeeded { "load" } else { "error" };
        let dispatch = format!(
            "if (document.currentScript) document.currentScript.dispatchEvent(new Event('{event_type}'));"
        );
        if let Err(error) = context.eval(Source::from_bytes(&dispatch)) {
            outcome.errors.push(format!(
                "{}: dispatch {event_type} event: {error}",
                script.source_url
            ));
        }
    }
    succeeded
}

fn drain_dynamic_scripts(
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    outcome: &mut ScriptOutcome,
    dynamic_script_loader: &mut Option<&mut DynamicScriptLoader<'_>>,
    total_bytes: &mut usize,
) {
    let Some(loader) = dynamic_script_loader.as_mut() else {
        return;
    };
    let mut executed = 0_usize;
    loop {
        let pending = std::mem::take(&mut host.borrow_mut().pending_dynamic_scripts);
        if pending.is_empty() {
            return;
        }

        for pending_script in pending {
            if executed >= MAX_DYNAMIC_SCRIPTS {
                outcome.errors.push(format!(
                    "dynamically inserted scripts exceeded the limit of {MAX_DYNAMIC_SCRIPTS}"
                ));
                return;
            }
            executed += 1;
            let code = match loader(&pending_script.source_url) {
                Ok(code) => code,
                Err(error) => {
                    outcome.errors.push(format!(
                        "{}: dynamically inserted script could not be loaded: {error}",
                        pending_script.source_url
                    ));
                    continue;
                }
            };
            if total_bytes.saturating_add(code.len()) > MAX_SCRIPT_BYTES {
                outcome.errors.push(format!(
                    "{}: skipped because the page exceeds the {} MiB JavaScript limit",
                    pending_script.source_url,
                    MAX_SCRIPT_BYTES / 1024 / 1024
                ));
                return;
            }
            *total_bytes += code.len();
            let script = ScriptInput {
                node: pending_script.node,
                source_url: pending_script.source_url,
                code,
                finish_lifecycle: false,
            };
            evaluate_script(context, host, outcome, &script, true);
        }
    }
}
const IFRAME_REALM_BOOTSTRAP: &str = r#"
globalThis.window = globalThis;
globalThis.self = globalThis;
if (typeof String.prototype.substr !== 'function') {
    Object.defineProperty(String.prototype, 'substr', {
        configurable: true,
        writable: true,
        value(start, length) {
            const string = String(this);
            const size = string.length;
            let from = Number(start) || 0;
            from = from < 0 ? Math.max(size + Math.ceil(from), 0) : Math.min(Math.floor(from), size);
            if (length === undefined) return string.slice(from);
            let count = Number(length);
            if (Number.isNaN(count) || count <= 0) return '';
            if (count !== Infinity) count = Math.floor(count);
            return string.slice(from, Math.min(from + count, size));
        }
    });
}
"#;

pub(super) fn finish_host(
    mut outcome: ScriptOutcome,
    host: &Rc<RefCell<HostState>>,
) -> ScriptOutcome {
    let mut state = host.borrow_mut();
    outcome.mutation_count = std::mem::take(&mut state.mutation_count);
    outcome.executed = outcome.executed.max(std::mem::take(&mut state.executed));
    outcome.console.append(&mut state.console);
    outcome.diagnostics.append(&mut state.diagnostics);
    outcome.navigation_url = state.navigation_url.take();
    outcome.cookie_updates.append(&mut state.cookie_updates);
    outcome.render_requested = state.timers.take_render_request();
    outcome.invalidation = state.pending_invalidation.take(outcome.mutation_count);
    outcome
}
