//! Script evaluation and bounded event-loop settlement for an owned document realm.

use super::dynamic_scripts::drain_dynamic_scripts;
use super::timer_execution::{
    TimerSlice, append_timer_summary, settle_startup_timer_slice, settle_timer_slice,
};
use super::*;

const SCRIPT_TASK_TIMER_SLICE: Duration = Duration::from_millis(16);
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

pub(super) fn execute_inner(
    scripts: &[ScriptInput],
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    total_bytes: &mut usize,
    dynamic_script_loader: &mut Option<&mut DynamicScriptLoader<'_>>,
    defer_dynamic_scripts: bool,
    request_document_lifecycle: bool,
) -> ScriptOutcome {
    let mut outcome = ScriptOutcome::default();
    if let Err(error) = context.initialize_iframe_realm(IFRAME_REALM_BOOTSTRAP) {
        outcome
            .errors
            .push(format!("initialize iframe browser bindings: {error}"));
        return outcome;
    }

    if let Err(error) = context.eval(Source::from_bytes(super::bootstrap::BROWSER_BOOTSTRAP)) {
        outcome
            .errors
            .push(format!("initialize browser bindings: {error}"));
        return outcome;
    }

    {
        let mut state = host.borrow_mut();
        // Parsed scripts have already been prepared by the time this static document model starts
        // evaluation. Preserve HTML''s "already started" flag so moving one of those elements
        // while it runs cannot enqueue and execute the same script a second time.
        for script in scripts {
            state.mark_script_started(&script.node);
        }
        state.begin_task();
    }
    for script in scripts {
        if script.kind == ScriptKind::Module
            || (script.node.attr("src").is_some()
                && script.node.attr("defer").is_some()
                && script.node.attr("async").is_none())
        {
            runtime::document_lifecycle::enter_interactive(context, host, &mut outcome);
        }
        if total_bytes.saturating_add(script.code.len()) > MAX_PAGE_SCRIPT_BYTES {
            outcome.errors.push(format!(
                "{}: skipped because the page exceeds the {} MiB JavaScript limit",
                script.source_url,
                MAX_PAGE_SCRIPT_BYTES / 1024 / 1024
            ));
            break;
        }
        *total_bytes += script.code.len();
        evaluate_script(
            context,
            host,
            &mut outcome,
            script,
            false,
            dynamic_script_loader,
            total_bytes,
        );
        super::module_lifecycle::drain(context, host, &mut outcome);
        // External and module scripts execute as event-loop tasks rather than one monolithic
        // page task. Give already-requested rendering/timer work one turn before the next script
        // task so animation-frame queues cannot grow without bound behind network execution.
        if script.node.attr("src").is_some() || script.kind == ScriptKind::Module {
            let mut no_dynamic_script_loader = None;
            settle_timer_slice(
                context,
                host,
                &mut outcome,
                &mut no_dynamic_script_loader,
                total_bytes,
                TimerSlice {
                    advance: SCRIPT_TASK_TIMER_SLICE,
                    max_callbacks: 1,
                },
                None,
            );
        }
        drain_dynamic_scripts_for_initial_task(
            context,
            host,
            &mut outcome,
            dynamic_script_loader,
            total_bytes,
            defer_dynamic_scripts,
        );
    }

    // An async-only document still completes parsing before its first external script arrives.
    let finish_lifecycle =
        scripts.is_empty() || scripts.iter().any(|script| script.finish_lifecycle);
    if let Err(error) = context.eval(Source::from_bytes("document.__setCurrentScript(0);")) {
        outcome
            .errors
            .push(format!("clear current script: {error}"));
    }
    if request_document_lifecycle && finish_lifecycle {
        runtime::document_lifecycle::parsing_finished(context, host, &mut outcome);
    }
    if let Err(error) = context.run_jobs() {
        outcome.errors.push(format!("finish promise jobs: {error}"));
    }
    super::module_lifecycle::drain(context, host, &mut outcome);
    drain_dynamic_scripts_for_initial_task(
        context,
        host,
        &mut outcome,
        dynamic_script_loader,
        total_bytes,
        defer_dynamic_scripts,
    );
    // A live document returns to its embedder for the first rendering opportunity.
    // Timer tasks must use subsequent real clock advances, not the synthetic settling
    // horizon used by the standalone execution helper. Microtasks above still complete.
    // https://html.spec.whatwg.org/multipage/webappapis.html#event-loop-processing-model
    if !request_document_lifecycle {
        append_timer_summary(host, &mut outcome);
        return outcome;
    }
    // Standalone execution helpers settle the two document tasks; live renderers select each
    // through advance_time so resource discovery and rendering can run between them.
    for _ in 0..2 {
        runtime::document_lifecycle::run_one(context, host, &mut outcome);
    }
    for _ in 0..STARTUP_TIMER_PASSES {
        if defer_dynamic_scripts {
            let mut no_dynamic_script_loader = None;
            settle_startup_timer_slice(
                context,
                host,
                &mut outcome,
                &mut no_dynamic_script_loader,
                total_bytes,
            );
        } else {
            settle_startup_timer_slice(
                context,
                host,
                &mut outcome,
                dynamic_script_loader,
                total_bytes,
            );
        }
    }

    append_timer_summary(host, &mut outcome);

    outcome
}

fn drain_dynamic_scripts_for_initial_task(
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    outcome: &mut ScriptOutcome,
    dynamic_script_loader: &mut Option<&mut DynamicScriptLoader<'_>>,
    total_bytes: &mut usize,
    defer_dynamic_scripts: bool,
) {
    if defer_dynamic_scripts {
        let mut no_dynamic_script_loader = None;
        drain_dynamic_scripts(
            context,
            host,
            outcome,
            &mut no_dynamic_script_loader,
            total_bytes,
        );
    } else {
        drain_dynamic_scripts(context, host, outcome, dynamic_script_loader, total_bytes);
    }
}

pub(super) fn execute_additional_inner(
    scripts: &[ScriptInput],
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    total_bytes: &mut usize,
    dynamic_script_loader: &mut Option<&mut DynamicScriptLoader<'_>>,
) -> ScriptOutcome {
    let mut outcome = ScriptOutcome::default();
    {
        let mut state = host.borrow_mut();
        for script in scripts {
            state.mark_script_started(&script.node);
        }
        state.begin_task();
    }
    for script in scripts {
        if total_bytes.saturating_add(script.code.len()) > MAX_PAGE_SCRIPT_BYTES {
            outcome.errors.push(format!(
                "{}: skipped because the page exceeds the {} MiB JavaScript limit",
                script.source_url,
                MAX_PAGE_SCRIPT_BYTES / 1024 / 1024
            ));
            continue;
        }
        *total_bytes += script.code.len();
        evaluate_script(
            context,
            host,
            &mut outcome,
            script,
            true,
            dynamic_script_loader,
            total_bytes,
        );
        super::module_lifecycle::drain(context, host, &mut outcome);
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
    super::module_lifecycle::drain(context, host, &mut outcome);
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

pub(super) fn evaluate_script(
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    outcome: &mut ScriptOutcome,
    script: &ScriptInput,
    dispatch_load: bool,
    dynamic_script_loader: &mut Option<&mut DynamicScriptLoader<'_>>,
    total_bytes: &mut usize,
) -> bool {
    if script.kind == ScriptKind::Module {
        return module_evaluation::evaluate_module(
            context,
            host,
            outcome,
            script,
            dispatch_load,
            dynamic_script_loader,
            total_bytes,
        );
    }
    let node_id = host.borrow_mut().id_for(&script.node);
    let current_script = format!(
        "(() => {{ const old = document.currentScript; document.__setCurrentScript({node_id}); if (document.currentScript.getRootNode() instanceof ShadowRoot) document.__setCurrentScript(0); return old ? old.__id : 0; }})()"
    );
    let previous_script = match context.eval(Source::from_bytes(&current_script)) {
        Ok(value) => value.as_number().unwrap_or_default() as u32,
        Err(error) => {
            outcome.errors.push(format!(
                "{}: set document.currentScript: {error}",
                script.source_url
            ));
            0
        }
    };

    let script_started = Instant::now();
    let succeeded =
        match mutation_host::eval_with_writes(context, host, &script.code, &script.source_url) {
            Ok(_) => {
                outcome.executed += 1;
                host.borrow_mut().executed += 1;
                true
            }
            Err(error) => {
                outcome
                    .errors
                    .push(format!("{}: {error}", script.source_url));
                false
            }
        };
    // Running a classic script ends with a microtask checkpoint, before the outer
    // execute-script-element algorithm restores currentScript and fires load.
    // https://html.spec.whatwg.org/multipage/webappapis.html#clean-up-after-running-script
    if let Err(error) = context.run_jobs() {
        outcome
            .errors
            .push(format!("{}: promise job: {error}", script.source_url));
    }
    let script_time = script_started.elapsed();
    if script_time.as_millis() >= 1 {
        outcome.diagnostics.push(format!(
            "JavaScript {:.3} ms: {}",
            script_time.as_secs_f64() * 1_000.0,
            script.source_url
        ));
    }

    let restore = format!("document.__setCurrentScript({previous_script});");
    if let Err(error) = context.eval(Source::from_bytes(&restore)) {
        outcome.errors.push(format!(
            "{}: clear current script: {error}",
            script.source_url
        ));
    }
    if dispatch_load {
        // Fetch success, not successful evaluation, determines the element's load event.
        // Runtime exceptions are reported separately; handlers observe restored currentScript.
        // https://html.spec.whatwg.org/multipage/scripting.html#execute-the-script-element
        super::module_lifecycle::dispatch_script_event(
            context,
            outcome,
            node_id,
            "load",
            &script.source_url,
        );
    }
    succeeded
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
