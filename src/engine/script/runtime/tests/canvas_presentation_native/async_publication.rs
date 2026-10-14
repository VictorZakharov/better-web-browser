use super::*;
use std::time::{Duration, Instant};

#[test]
fn native_canvas_completion_wakeup_obeys_visibility_and_monitor_cadence() {
    let dom = dom::parse("<body></body>");
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    let rate = crate::engine::rendering::RefreshRate::new(1000, 1).unwrap();
    runtime.host.borrow_mut().defer_canvas_poll();
    runtime.set_rendering_environment(rate, false);
    assert_eq!(runtime.next_timer_delay(), None);
    runtime.elapse_time(Duration::from_secs(2));
    assert!(!runtime.host.borrow().timers.render_requested());
    runtime.set_rendering_environment(rate, true);
    let delay = runtime.next_timer_delay().unwrap();
    assert!(delay > Duration::ZERO);
    runtime.elapse_time(delay);
    assert_eq!(
        runtime.host.borrow_mut().timers.take_render_scope(),
        Some(crate::engine::scheduler::RenderScope::SurfacePixels)
    );
    assert_eq!(runtime.next_timer_delay(), None);
}

pub(super) fn take_gpu(
    runtime: &mut ScriptRuntime,
) -> Vec<crate::engine::script::runtime::canvas_presentation::CanvasPaintSnapshot> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let frames = runtime.take_canvas_presentation().unwrap();
        if frames.iter().any(|frame| frame.gpu_frame.is_some()) {
            return frames;
        }
        assert!(
            Instant::now() < deadline,
            "pending GPU presentation never completed"
        );
        runtime.elapse_time(Duration::from_millis(17));
        std::thread::yield_now();
    }
}

#[test]
fn pending_canvas_wakes_without_author_animation_and_recovers_before_completion() {
    let dom = dom::parse("<canvas width=4 height=2></canvas><script></script>");
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    let outcome = runtime.execute_initial(&[input(&script, "const canvas=document.querySelector('canvas');const gl=canvas.getContext('webgl2',{antialias:false});gl.clear(gl.COLOR_BUFFER_BIT);")]);
    assert!(outcome.errors.is_empty());
    runtime.take_canvas_presentation().unwrap();
    runtime.set_gpu_canvas_enabled(true);
    run(
        &mut runtime,
        &script,
        "gl.clearColor(0,1,0,1);gl.clear(gl.COLOR_BUFFER_BIT);",
    );
    assert!(runtime.take_canvas_presentation().unwrap().is_empty());
    runtime.host.borrow_mut().timers.take_render_request();
    let delay = runtime
        .next_timer_delay()
        .expect("native completion must wake without an author timer");
    assert!(delay > Duration::ZERO);
    assert!(!runtime.host.borrow().timers.render_requested());
    runtime.elapse_time(delay);
    assert!(runtime.host.borrow().timers.render_requested());
    // Repair must read the queued green frame, not the implicitly cleared source.
    runtime.set_gpu_canvas_enabled(false);
    let frames = runtime.take_canvas_presentation().unwrap();
    assert_eq!(
        frames[0].pixels.as_deref().unwrap(),
        [0, 255, 0, 255].repeat(8)
    );
    assert!(
        runtime
            .host
            .borrow()
            .canvas_poll
            .delay(Duration::ZERO, true)
            .is_none()
    );
    assert!(runtime.take_canvas_presentation().unwrap().is_empty());
}

#[test]
fn pending_canvas_completes_and_detachment_retires_its_wakeup() {
    let dom = dom::parse("<canvas width=4 height=2></canvas><script></script>");
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    let outcome = runtime.execute_initial(&[input(&script, "const canvas=document.querySelector('canvas');const gl=canvas.getContext('webgl2',{antialias:false});gl.clear(gl.COLOR_BUFFER_BIT);")]);
    assert!(outcome.errors.is_empty());
    runtime.take_canvas_presentation().unwrap();
    runtime.set_gpu_canvas_enabled(true);
    run(
        &mut runtime,
        &script,
        "gl.clearColor(1,0,0,1);gl.clear(gl.COLOR_BUFFER_BIT);",
    );
    assert!(runtime.take_canvas_presentation().unwrap().is_empty());
    let frames = take_gpu(&mut runtime);
    assert_eq!((frames[0].width, frames[0].height), (4, 2));
    assert!(runtime.take_canvas_presentation().unwrap().is_empty());
    run(
        &mut runtime,
        &script,
        "gl.clearColor(0,1,0,1);gl.clear(gl.COLOR_BUFFER_BIT);",
    );
    runtime.take_canvas_presentation().unwrap();
    run(&mut runtime, &script, "canvas.remove();");
    assert!(runtime.take_canvas_presentation().unwrap().is_empty());
    assert!(
        runtime
            .host
            .borrow()
            .canvas_poll
            .delay(Duration::ZERO, true)
            .is_none()
    );
}
