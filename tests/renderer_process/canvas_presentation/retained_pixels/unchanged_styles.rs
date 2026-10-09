//! Exact cascade equality may retain boxes; author-visible differences may not.
use super::*;

const UNCHANGED: &str = "Style checkpoint retained exactly unchanged geometry and paint";

fn unchanged(presentation: &RendererPresentation) -> bool {
    presentation
        .runtime
        .diagnostics
        .iter()
        .any(|entry| entry == UNCHANGED)
}

#[test]
fn equal_inline_css_recomputes_styles_but_retains_boxes_and_publishes_new_pixels() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let initial = load_html_document_with_selectors(
        &session,
        350,
        r#"<!doctype html><style>body{margin:0}canvas{display:block}</style>
        <canvas width=2 height=1 style='color:red;margin-left:7px'></canvas><p>text</p><script>
            const canvas=document.querySelector('canvas'), ctx=canvas.getContext('2d');
            const paint=(color)=>{ctx.fillStyle=color;ctx.fillRect(0,0,2,1)};
            paint('red');
            setTimeout(()=>{canvas.style.cssText='margin-left:7px;color:red';paint('blue')},10000);
            setTimeout(()=>{canvas.style.color='green';paint('green')},20000);
            setTimeout(()=>{canvas.style.width='3px';paint('yellow')},30000);
            setTimeout(()=>{document.querySelector('p').textContent='changed';canvas.style.cssText='width:3px;margin-left:7px;color:green';paint('white')},40000);
        </script>"#,
        vec!["canvas".into()],
    );
    acknowledge(&session, &initial);
    let blue = next_canvas_pixels(&session, &initial, &[255, 0, 0, 255].repeat(2));
    assert!(unchanged(&blue), "{:?}", blue.runtime.diagnostics);
    assert!(
        blue.style.recomputed_styles > 0,
        "not a pixel-only shortcut"
    );
    assert_eq!(blue.style.changed_styles, 0);
    assert_eq!(image_rect(&blue), image_rect(&initial));
    acknowledge(&session, &blue);

    let green = next_canvas_pixels(&session, &blue, &[0, 128, 0, 255].repeat(2));
    assert!(
        !unchanged(&green),
        "paint changes must not reuse unchanged paint"
    );
    assert!(
        green.style.changed_styles > 0,
        "first cascade evidence retained"
    );
    acknowledge(&session, &green);

    let yellow = next_canvas_pixels(&session, &green, &[0, 255, 255, 255].repeat(2));
    assert!(!unchanged(&yellow));
    assert_eq!(image_rect(&yellow).width, 3.0);
    acknowledge(&session, &yellow);

    let white = next_canvas_pixels(&session, &yellow, &[255, 255, 255, 255].repeat(2));
    assert!(!unchanged(&white), "mixed text and CSS changes need layout");
    assert!(
        white
            .layout
            .items
            .iter()
            .any(|item| matches!(item, DisplayItem::Text{text,..}if text=="changed"))
    );
    session.shutdown().unwrap();
}

#[test]
fn style_attribute_relational_selector_refreshes_geometry_outside_the_dirty_subtree() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let initial = load_html_document_with_selectors(
        &session,
        351,
        r#"<!doctype html><style>body{margin:0}canvas{display:block}html:has([style^='padding']) body{padding-left:17px}</style>
        <section><canvas width=2 height=1 style='color:red;padding:0'></canvas></section><script>
            const canvas=document.querySelector('canvas'),ctx=canvas.getContext('2d');
            ctx.fillStyle='red';ctx.fillRect(0,0,2,1);
            setTimeout(()=>{canvas.style.cssText='padding:0;color:red';ctx.fillStyle='blue';ctx.fillRect(0,0,2,1)},10000);
        </script>"#,
        vec!["canvas".into()],
    );
    acknowledge(&session, &initial);
    let blue = next_canvas_pixels(&session, &initial, &[255, 0, 0, 255].repeat(2));
    assert!(!unchanged(&blue));
    assert_eq!(image_rect(&initial).x, 0.0);
    assert_eq!(image_rect(&blue).x, 17.0);
    session.shutdown().unwrap();
}

#[test]
fn equal_css_with_a_first_canvas_export_cannot_skip_natural_geometry() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let initial = load_html_document_with_selectors(
        &session,
        352,
        r#"<!doctype html><canvas width=2 height=1 style='color:red;padding:0'></canvas><script>
            setTimeout(()=>{const c=document.querySelector('canvas');c.style.cssText='padding:0;color:red';const ctx=c.getContext('2d');ctx.fillStyle='blue';ctx.fillRect(0,0,2,1)},10000);
        </script>"#,
        vec!["canvas".into()],
    );
    acknowledge(&session, &initial);
    let blue = next_canvas_pixels(&session, &initial, &[255, 0, 0, 255].repeat(2));
    assert!(
        !unchanged(&blue),
        "first export needs a new image display item"
    );
    assert_eq!(image_rect(&blue).width, 2.0);
    session.shutdown().unwrap();
}

#[test]
fn animation_overlays_cannot_force_layout_when_the_real_cascade_stays_equal() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let initial = load_html_document_with_selectors(
        &session,
        353,
        r#"<!doctype html><style>canvas{display:block;opacity:1!important}</style>
        <canvas width=2 height=1></canvas><script>
            const canvas=document.querySelector('canvas'),ctx=canvas.getContext('2d');
            ctx.fillStyle='red';ctx.fillRect(0,0,2,1);
            setTimeout(()=>{
                const animation=canvas.animate([{opacity:.2},{opacity:.8}],{duration:40000,fill:'both'});
                animation.pause();animation.currentTime=1000;
                ctx.fillStyle='blue';ctx.fillRect(0,0,2,1);
            },10000);
        </script>"#,
        vec!["canvas".into()],
    );
    acknowledge(&session, &initial);
    let blue = next_canvas_pixels(&session, &initial, &[255, 0, 0, 255].repeat(2));
    assert!(unchanged(&blue), "{:?}", blue.runtime.diagnostics);
    assert!(blue.style.recomputed_styles > 0);
    assert_eq!(blue.style.changed_styles, 0);
    assert_eq!(image_rect(&initial), image_rect(&blue));
    session.shutdown().unwrap();
}
