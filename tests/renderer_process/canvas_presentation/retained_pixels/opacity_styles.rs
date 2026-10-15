//! Coalesced pixel/opacity publication retains geometry; mixed work falls back.
use super::*;

const OPACITY: &str = "Style checkpoint updated retained opacity groups without layout";

fn retained(presentation: &RendererPresentation) -> bool {
    presentation
        .runtime
        .diagnostics
        .iter()
        .any(|s| s == OPACITY)
}

#[test]
fn canvas_opacity_updates_publish_pixels_and_mixed_changes_rebuild() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let initial = load_html_document_with_selectors(
        &session,
        354,
        r#"<!doctype html><style>body{margin:0}canvas{display:block;opacity:.8}</style>
        <canvas width=2 height=1></canvas><p>before</p><script>
            const canvas=document.querySelector('canvas'),ctx=canvas.getContext('2d');
            const paint=c=>{ctx.fillStyle=c;ctx.fillRect(0,0,canvas.width,1)};
            paint('red');
            setTimeout(()=>{canvas.style.opacity='.4';paint('blue')},10000);
            setTimeout(()=>{canvas.style.opacity='1';paint('green')},20000);
            setTimeout(()=>{canvas.style.opacity='.6';canvas.width=3;paint('yellow')},30000);
            setTimeout(()=>{canvas.style.opacity='.3';document.querySelector('p').textContent='after';paint('white')},40000);
        </script>"#,
        vec!["canvas".into()],
    );
    acknowledge(&session, &initial);
    let blue = next_canvas_pixels(&session, &initial, &[255, 0, 0, 255].repeat(2));
    assert!(retained(&blue), "{:?}", blue.runtime.diagnostics);
    assert!(blue.layout.items.iter().any(|item| matches!(
        item, DisplayItem::BeginOpacity { opacity, .. } if *opacity == 0.4
    )));
    assert_eq!(image_rect(&blue), image_rect(&initial));
    acknowledge(&session, &blue);

    let green = next_canvas_pixels(&session, &blue, &[0, 128, 0, 255].repeat(2));
    assert!(
        !retained(&green),
        "removing a stacking context needs normal paint"
    );
    assert!(
        !green
            .layout
            .items
            .iter()
            .any(|item| matches!(item, DisplayItem::BeginOpacity { .. }))
    );
    acknowledge(&session, &green);

    let yellow = next_canvas_pixels(&session, &green, &[0, 255, 255, 255].repeat(3));
    assert!(!retained(&yellow), "bitmap resize needs natural geometry");
    assert_eq!(image_rect(&yellow).width, 3.);
    acknowledge(&session, &yellow);

    let white = next_canvas_pixels(&session, &yellow, &[255, 255, 255, 255].repeat(3));
    assert!(!retained(&white), "mixed text/opacity changes need layout");
    assert!(white.layout.items.iter().any(|item| matches!(
        item, DisplayItem::Text { text, .. } if text == "after"
    )));
    session.shutdown().unwrap();
}
