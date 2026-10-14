use super::support::*;
use better_web_browser::engine::rendering::RefreshRate;
use better_web_browser::renderer_process::{RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::PresentedViewport;
use std::time::Duration;

#[test]
fn monitor_cadence_update_retimes_frames_without_resizing_or_repainting() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch renderer");
    let initial = load_scripted_html_document(
        &session,
        975,
        r#"<!doctype html><title>untouched</title>
      <canvas width=4 height=2></canvas><script>
      const gl=document.querySelector('canvas').getContext('webgl2',{preserveDrawingBuffer:true,antialias:false});
      gl.clearColor(0,1,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
      let resized=false;addEventListener('resize',()=>resized=true);
      setTimeout(()=>{resized=false;console.log('baseline ready');},5000);
      setTimeout(()=>{requestAnimationFrame(()=>{
        const pixel=new Uint8Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
        document.title=resized?'unexpected resize':pixel.join()==='0,255,0,255'?'retained green':'lost pixels';
      });console.log('frame queued');},10000);
      </script>"#,
    );
    acknowledge(&session, &initial);
    let mut baseline_ready = false;
    for step in 0..10 {
        session
            .advance_time(
                initial.document,
                if step == 0 {
                    Duration::from_secs(5)
                } else {
                    Duration::ZERO
                },
                1,
            )
            .unwrap();
        loop {
            match session.wait_for_event(Duration::from_secs(3)).unwrap() {
                RendererEvent::RuntimeUpdate(update) => {
                    assert!(update.runtime.errors.is_empty());
                    baseline_ready = update
                        .runtime
                        .console
                        .iter()
                        .any(|value| value == "log: baseline ready");
                    break;
                }
                RendererEvent::Presentation(value) => {
                    assert!(value.runtime.errors.is_empty());
                    baseline_ready = value
                        .runtime
                        .console
                        .iter()
                        .any(|value| value == "log: baseline ready");
                    acknowledge(&session, &value);
                    break;
                }
                RendererEvent::Diagnostic { .. } => {}
                other => panic!("unexpected initial settlement: {other:?}"),
            }
        }
        if baseline_ready {
            break;
        }
    }
    assert!(baseline_ready);
    session
        .update_viewport(
            initial.document,
            PresentedViewport {
                width: 800.0,
                height: 600.0,
                style_width: 800.0,
                dpi: 96,
                prefers_dark_color_scheme: false,
                refresh_rate: RefreshRate::new(480, 1).unwrap(),
            },
        )
        .unwrap();
    loop {
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::RuntimeUpdate(update) => {
                assert!(!update.clock_advanced);
                assert!(update.runtime.errors.is_empty());
                break;
            }
            RendererEvent::Diagnostic { .. } => {}
            other => panic!("cadence-only update must not publish a resized page: {other:?}"),
        }
    }
    let mut next = None;
    for step in 0..10 {
        session
            .advance_time(
                initial.document,
                if step == 0 {
                    Duration::from_secs(11)
                } else {
                    Duration::ZERO
                },
                1,
            )
            .unwrap();
        loop {
            match session.wait_for_event(Duration::from_secs(3)).unwrap() {
                RendererEvent::RuntimeUpdate(update) => {
                    assert!(update.runtime.errors.is_empty());
                    if update
                        .runtime
                        .console
                        .iter()
                        .any(|value| value == "log: frame queued")
                    {
                        next = update.next_timer_micros;
                    }
                    break;
                }
                RendererEvent::Diagnostic { .. } => {}
                other => panic!("unexpected timer setup: {other:?}"),
            }
        }
        if next.is_some() {
            break;
        }
    }
    let delay = next.expect("timeout did not queue an animation frame");
    assert!(
        delay > 0 && delay < 10_000,
        "480 Hz must not inherit a 16 ms timer: {delay}"
    );
    session
        .advance_time(initial.document, Duration::from_micros(delay), 1)
        .unwrap();
    let frame = wait_for_presentation(
        &session,
        initial.document,
        "next animation callback",
        |_| true,
    );
    assert_eq!(frame.title, "retained green");
    session.shutdown().unwrap();
}
