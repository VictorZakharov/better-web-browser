//! Bounded timer-task execution for a retained JavaScript realm.

use super::dynamic_scripts::drain_dynamic_scripts;
use super::*;

// Each callback and its microtask checkpoint are one indivisible HTML task. Yield between tasks
// after this wall-clock slice so rendering and the renderer control plane get an opportunity to
// run even when many timers became due while another command was in flight.
const TIMER_TASK_WALL_SLICE: Duration = Duration::from_millis(25);

#[derive(Debug, Clone, Copy)]
pub(super) struct TimerSlice {
    pub(super) advance: Duration,
    pub(super) max_callbacks: usize,
}

pub(super) fn append_timer_summary(host: &Rc<RefCell<HostState>>, outcome: &mut ScriptOutcome) {
    let timer_summary = host.borrow().timer_summary();
    if !timer_summary.is_empty() {
        outcome
            .diagnostics
            .push(format!("JavaScript timers after settling: {timer_summary}"));
    }
}

pub(super) fn settle_startup_timer_slice(
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
        TimerSlice {
            advance: STARTUP_TIMER_SLICE,
            max_callbacks: MAX_TIMER_CALLBACKS_PER_SLICE,
        },
        None,
    );
}

pub(super) fn settle_timer_slice(
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    outcome: &mut ScriptOutcome,
    dynamic_script_loader: &mut Option<&mut DynamicScriptLoader<'_>>,
    total_bytes: &mut usize,
    slice: TimerSlice,
    mut stage_reporter: Option<&mut dyn FnMut(&str)>,
) {
    let horizon = host.borrow().timers.now().saturating_add(slice.advance);
    let slice_started = Instant::now();

    for _ in 0..slice.max_callbacks {
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
        let label = context
            .call_global("__timerLabel", &[timer_id.into()])
            .and_then(|value| value.to_string(context))
            .map(|value| value.to_std_string_escaped())
            .unwrap_or_else(|_| "unknown callback".to_string());
        if let Some(reporter) = stage_reporter.as_deref_mut() {
            reporter(&format!("executing JavaScript timer {timer_id}: {label}"));
        }
        // Keep profiling from earlier script work separate from this callback. Diagnostic
        // selectors opt into host timing; ordinary browsing leaves the profile disabled.
        host.borrow_mut()
            .append_host_call_diagnostics(&mut outcome.diagnostics);
        let callback_started = Instant::now();
        let callback_result = context.call_global("__runTimer", &[timer_id.into()]);
        let callback_elapsed = callback_started.elapsed();
        if let Err(error) = &callback_result {
            let callback = (!label.is_empty())
                .then(|| format!(" ({label})"))
                .unwrap_or_default();
            outcome
                .errors
                .push(format!("JavaScript timer {timer_id}{callback}: {error}"));
        }
        let mut callback_diagnostics = Vec::new();
        host.borrow_mut()
            .append_host_call_diagnostics(&mut callback_diagnostics);
        if callback_result.is_err() || callback_elapsed >= Duration::from_millis(100) {
            outcome.diagnostics.extend(
                callback_diagnostics.into_iter().map(|diagnostic| {
                    format!("JavaScript timer {timer_id} ({label}): {diagnostic}")
                }),
            );
        }
        outcome.record_timing("JavaScript timer callback", callback_elapsed);
        // HTML performs a microtask checkpoint after every task. V8 owns the Promise job queue,
        // so drain it here rather than once after a whole batch of timer callbacks.
        if let Some(reporter) = stage_reporter.as_deref_mut() {
            reporter(&format!(
                "draining promise jobs after JavaScript timer {timer_id}: {label}"
            ));
        }
        let jobs_started = Instant::now();
        if let Err(error) = context.run_jobs() {
            outcome
                .errors
                .push(format!("JavaScript timer {timer_id} promise job: {error}"));
        }
        outcome.record_timing("JavaScript timer promise jobs", jobs_started.elapsed());
        super::module_lifecycle::drain(context, host, outcome);
        drain_dynamic_scripts(context, host, outcome, dynamic_script_loader, total_bytes);
        if slice_started.elapsed() >= TIMER_TASK_WALL_SLICE {
            break;
        }
    }

    host.borrow_mut().timers.advance_to(horizon);
}
