//! A fixed HUD may replace text and redraw an SVG graph without rebuilding background boxes.
use super::*;

const FIXED: &str = "Layout checkpoint refreshed independent fixed layers";

fn visible_text(presentation: &RendererPresentation) -> String {
    presentation
        .layout
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text { text, .. } => Some(&**text),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn fixed_text_and_svg_updates_publish_real_pixels_and_unknown_dom_work_falls_back() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let initial = load_html_document_with_selectors(
        &session,
        355,
        r#"<!doctype html><style>body{margin:0}section{position:fixed;left:10px;top:20px;width:180px;overflow:hidden}
        canvas{display:block}</style><main>unrelated background text</main>
        <section><span id=counter>10 FPS</span><svg xmlns='http://www.w3.org/2000/svg' width=20 height=20 viewBox='0 0 20 20'>
        <path id=graph fill='red' d='M0 0H10V20H0Z'/></svg></section><canvas width=2 height=1></canvas><script>
            const canvas=document.querySelector('canvas'),ctx=canvas.getContext('2d');
            const paint=c=>{ctx.fillStyle=c;ctx.fillRect(0,0,2,1)};paint('red');
            setTimeout(()=>{
                document.querySelector('#counter').textContent='144 FPS';
                document.querySelector('#graph').setAttribute('d','M10 0H20V20H10Z');paint('blue');
            },10000);
            setTimeout(()=>{
                const child=document.createElement('b');child.textContent='new element';
                document.querySelector('section').appendChild(child);paint('green');
            },20000);
        </script>"#,
        vec!["section".into()],
    );
    acknowledge(&session, &initial);
    let blue = next_canvas_pixels(&session, &initial, &[255, 0, 0, 255].repeat(2));
    assert!(
        blue.runtime.diagnostics.iter().any(|s| s == FIXED),
        "{:?}",
        blue.runtime.diagnostics
    );
    assert!(
        visible_text(&blue).contains("144 FPS"),
        "{}",
        visible_text(&blue)
    );
    let svg = blue
        .images
        .iter()
        .find(|image| image.url.starts_with("inline-svg:"))
        .expect("updated graph pixels");
    let image = &svg.image;
    let pixel = |x: u32| {
        let offset = ((image.height / 2 * image.width + x) * 4) as usize;
        &image.bgra[offset..offset + 4]
    };
    assert_eq!(pixel(image.width / 4), &[0, 0, 0, 0]);
    assert_eq!(pixel(image.width * 3 / 4), &[0, 0, 255, 255]);
    assert_eq!(image_rect(&blue), image_rect(&initial));
    acknowledge(&session, &blue);
    let green = next_canvas_pixels(&session, &blue, &[0, 128, 0, 255].repeat(2));
    assert!(!green.runtime.diagnostics.iter().any(|s| s == FIXED));
    assert!(
        visible_text(&green).contains("new element"),
        "{}",
        visible_text(&green)
    );
    session.shutdown().unwrap();
}
