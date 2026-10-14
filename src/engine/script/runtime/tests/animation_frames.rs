//! Display policy, timer isolation, and rendering-opportunity contracts.
use super::*;
use crate::engine::rendering::RefreshRate;

fn fixture(code: &str, rate: u32) -> (dom::Dom, ScriptRuntime) {
    let dom = dom::parse_with_scripting("<body><script></script></body>", true);
    let node = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    runtime.set_rendering_environment(RefreshRate::new(rate, 1).unwrap(), true);
    let result = runtime.execute_initial_before_document_completion(
        &[input(&node, "frames.js", code, false)],
        None,
    );
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    (dom, runtime)
}

fn run_next(runtime: &mut ScriptRuntime) {
    let delay = runtime.next_timer_delay().expect("pending work");
    let result = runtime.advance_time(delay, 1);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
}

#[test]
fn active_display_rate_controls_pending_animation_tasks() {
    for frequency in [144, 240, 360, 500] {
        let (_, mut runtime) = fixture("requestAnimationFrame(() => {});", frequency);
        let delay = runtime.next_timer_delay().unwrap();
        assert!(delay > Duration::ZERO, "{frequency}: {delay:?}");
        assert_eq!(
            runtime.host.borrow().animation_frames.rate,
            RefreshRate::new(frequency, 1).unwrap()
        );
        let mut state = runtime.host.borrow_mut();
        state.timers.advance_to(delay);
        let mut source = None;
        state.timers.run_one_task(|_, work| {
            if let crate::engine::scheduler::ScheduledWork::Task(task) = work {
                source = Some(task.source);
            }
        });
        assert_eq!(
            source,
            Some(crate::engine::scheduler::TaskSource::Rendering)
        );
    }
}

#[test]
fn display_changes_retime_frames_without_retiming_author_timers() {
    let (_, mut runtime) = fixture(
        "setTimeout(() => {}, 2000); requestAnimationFrame(() => {});",
        60,
    );
    let timer_due = {
        let state = runtime.host.borrow();
        state
            .timer_handles
            .values()
            .filter_map(|handle| state.timers.scheduled_for(*handle))
            .max()
            .unwrap()
    };
    runtime.set_rendering_environment(RefreshRate::new(240, 1).unwrap(), true);
    assert!(runtime.next_timer_delay().unwrap() < timer_due);
    let state = runtime.host.borrow();
    assert!(
        state
            .timer_handles
            .values()
            .any(|handle| state.timers.scheduled_for(*handle) == Some(timer_due))
    );
}

#[test]
fn hidden_documents_retain_requests_without_animation_wakeups_or_catch_up() {
    let (dom, mut runtime) = fixture(
        r#"
        let count = 0;
        function frame() { document.body.setAttribute('data-count', String(++count)); requestAnimationFrame(frame); }
        requestAnimationFrame(frame);
    "#,
        240,
    );
    runtime.set_rendering_environment(RefreshRate::new(240, 1).unwrap(), false);
    assert_eq!(runtime.next_timer_delay(), None);
    assert!(
        runtime
            .advance_time(Duration::from_secs(5), 8)
            .errors
            .is_empty()
    );
    assert!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-count")
            .is_none()
    );
    runtime.set_rendering_environment(RefreshRate::new(240, 1).unwrap(), true);
    assert!(runtime.next_timer_delay().unwrap() > Duration::ZERO);
    run_next(&mut runtime);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-count")
            .as_deref(),
        Some("1")
    );
    assert!(runtime.next_timer_delay().unwrap() > Duration::ZERO);
}

#[test]
fn cancellation_uses_private_scheduler_and_author_timer_ids_cannot_cancel_frames() {
    let (dom, mut runtime) = fixture(
        r#"
        const first = requestAnimationFrame(() => { document.body.setAttribute('data-bad', 'ran'); });
        clearTimeout = () => { throw Error('author override'); };
        cancelAnimationFrame(first);
        const second = requestAnimationFrame(() => { document.body.setAttribute('data-ok', 'ran'); });
        clearInterval(2); // second private task id is not an author timer
    "#,
        240,
    );
    run_next(&mut runtime);
    let body = dom.elements_named("body").next().unwrap();
    assert!(body.attr("data-bad").is_none());
    assert_eq!(body.attr("data-ok").as_deref(), Some("ran"));
    assert_eq!(runtime.next_timer_delay(), None);
}

#[test]
fn callback_batch_has_shared_native_timestamp_and_cancellation_during_delivery() {
    let (dom, mut runtime) = fixture(
        r#"
        const stamps = [];
        performance.now = () => -42;
        requestAnimationFrame(time => { stamps.push(time); cancelAnimationFrame(cancelled); requestAnimationFrame(next => {
            document.body.setAttribute('data-next', String(next >= time));
        }); });
        const cancelled = requestAnimationFrame(() => { throw Error('cancelled callback ran'); });
        requestAnimationFrame(time => { stamps.push(time); document.body.setAttribute('data-batch', String(stamps[0] === stamps[1] && time >= 0)); });
    "#,
        240,
    );
    run_next(&mut runtime);
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(body.attr("data-batch").as_deref(), Some("true"));
    assert!(body.attr("data-next").is_none());
    run_next(&mut runtime);
    assert_eq!(body.attr("data-next").as_deref(), Some("true"));
}

#[test]
fn late_delivery_runs_one_current_batch_and_schedules_only_a_future_opportunity() {
    let (dom, mut runtime) = fixture(
        r#"
        let count = 0;
        function frame() { document.body.setAttribute('data-count', String(++count)); requestAnimationFrame(frame); }
        requestAnimationFrame(frame);
    "#,
        240,
    );
    let outcome = runtime.advance_time(Duration::from_secs(2), 8);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-count")
            .as_deref(),
        Some("1")
    );
    assert!(runtime.next_timer_delay().unwrap() > Duration::ZERO);
}

#[test]
fn completed_frame_and_jobs_yield_to_publication_before_new_timer_tasks() {
    let (dom, mut runtime) = fixture(
        r#"
        requestAnimationFrame(() => {
            document.body.dataset.order = 'frame';
            queueMicrotask(() => document.body.dataset.order += '|jobs');
            setTimeout(() => document.body.dataset.order += '|timer', 0);
        });
    "#,
        240,
    );
    let delay = runtime.next_timer_delay().unwrap();
    assert!(runtime.advance_time(delay, 8).errors.is_empty());
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(body.attr("data-order").as_deref(), Some("frame|jobs"));
    assert_eq!(runtime.next_timer_delay(), Some(Duration::ZERO));
    assert!(runtime.advance_time(Duration::ZERO, 8).errors.is_empty());
    assert_eq!(body.attr("data-order").as_deref(), Some("frame|jobs|timer"));
}
