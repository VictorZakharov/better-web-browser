//! Runtime failure containment and host-outcome collection.

use super::*;

thread_local! {
    static LAST_PANIC_SITE: RefCell<Option<String>> = const { RefCell::new(None) };
}

pub(crate) fn install_runtime_panic_hook() {
    std::panic::set_hook(Box::new(|information| {
        let site = information
            .location()
            .map(|location| {
                format!(
                    "{}:{}:{}",
                    location.file(),
                    location.line(),
                    location.column()
                )
            })
            .unwrap_or_else(|| "unknown source location".to_string());
        LAST_PANIC_SITE.with(|slot| *slot.borrow_mut() = Some(site));
    }));
}

pub(super) fn panic_detail(payload: Box<dyn std::any::Any + Send>) -> String {
    let detail = payload
        .downcast_ref::<&str>()
        .map(|message| (*message).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown evaluator panic".to_string());
    LAST_PANIC_SITE
        .with(|slot| slot.borrow_mut().take())
        .map_or(detail.clone(), |site| format!("{detail} at {site}"))
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

pub(super) fn finish_host(
    mut outcome: ScriptOutcome,
    host: &Rc<RefCell<HostState>>,
) -> ScriptOutcome {
    let mut state = host.borrow_mut();
    outcome.mutation_count = std::mem::take(&mut state.mutation_count);
    outcome.executed = outcome.executed.max(std::mem::take(&mut state.executed));
    outcome.console.append(&mut state.console);
    outcome.diagnostics.append(&mut state.diagnostics);
    state.append_host_call_diagnostics(&mut outcome.diagnostics);
    outcome.navigation_url = state.navigation_url.take();
    outcome.cookie_updates.append(&mut state.cookie_updates);
    outcome.storage_updates.append(&mut state.storage_updates);
    outcome
        .fetch_actions
        .append(&mut state.pending_fetch_actions);
    outcome
        .worker_actions
        .append(&mut state.pending_worker_actions);
    outcome
        .fullscreen_actions
        .append(&mut state.pending_fullscreen_actions);
    outcome
        .media_actions
        .append(&mut state.pending_media_actions);
    outcome.render_requested = state.timers.take_render_request();
    outcome.invalidation = state.pending_invalidation.take(outcome.mutation_count);
    outcome
}
