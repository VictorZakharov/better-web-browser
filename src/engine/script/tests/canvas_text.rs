use super::*;

fn run_canvas_text(script: &str) -> (crate::engine::dom::Dom, super::super::ScriptOutcome) {
    execute_html(&format!(
        r#"<body><output id="status">waiting</output>
        <canvas width="160" height="80"></canvas><script>{script}</script></body>"#
    ))
}

#[test]
fn canvas_text_shapes_measures_and_paints_actual_pixels() {
    let (dom, outcome) = run_canvas_text(
        r#"
        const context = document.querySelector('canvas').getContext('2d');
        context.font = 'bold 24px Arial';
        const metrics = context.measureText('Hello');
        const before = context.getImageData(0, 0, 160, 80).data;
        context.fillStyle = '#ff0000';
        context.fillText('Hello', 8, 42);
        const after = context.getImageData(0, 0, 160, 80).data;
        let painted = 0, colored = 0;
        for (let i = 0; i < after.length; i += 4) {
            if (after[i + 3]) {
                painted++;
                if (after[i] > after[i + 2]) colored++;
            }
        }
        const checks = [context.font === 'bold 24px Arial',
            metrics instanceof TextMetrics, metrics.width > 20,
            metrics.actualBoundingBoxAscent > 0,
            before.every(value => value === 0), painted > 20, colored === painted];
        document.getElementById('status').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
    "#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn canvas_text_invalid_font_and_state_stack_follow_canvas_contract() {
    let (dom, outcome) = run_canvas_text(
        r#"
        const context = document.querySelector('canvas').getContext('2d');
        const initial = context.font;
        context.font = 'not-a-font';
        const ignored = context.font === initial;
        context.save();
        context.font = 'italic 18px serif';
        context.textAlign = 'center';
        context.textBaseline = 'middle';
        context.direction = 'rtl';
        context.restore();
        const checks = [ignored, context.font === initial, context.textAlign === 'start',
            context.textBaseline === 'alphabetic', context.direction === 'inherit'];
        document.getElementById('status').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
    "#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn canvas_text_alignment_and_transforms_move_the_painted_ink() {
    let (dom, outcome) = run_canvas_text(
        r#"
        const context = document.querySelector('canvas').getContext('2d');
        context.font = '20px Arial';
        const width = context.measureText('A').width;
        context.textAlign = 'center';
        context.translate(60, 0);
        context.fillText('A', 0, 32);
        let left = 160, right = 0;
        const pixels = context.getImageData(0, 0, 160, 80).data;
        for (let y = 0; y < 80; y++) for (let x = 0; x < 160; x++) {
            if (!pixels[(y * 160 + x) * 4 + 3]) continue;
            left = Math.min(left, x); right = Math.max(right, x);
        }
        document.getElementById('status').textContent =
            width > 0 && left < 60 && right > 55 && right < 80 ? 'yes' :
                [width, left, right].join(',');
    "#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn canvas_stroke_text_uses_outline_and_stroke_color() {
    let (dom, outcome) = run_canvas_text(
        r#"
        const context = document.querySelector('canvas').getContext('2d');
        context.font = '32px Arial';
        context.lineWidth = 3;
        context.strokeStyle = '#0000ff';
        context.strokeText('O', 12, 46);
        const pixels = context.getImageData(0, 0, 160, 80).data;
        let blue = 0, red = 0;
        for (let index = 0; index < pixels.length; index += 4) {
            if (!pixels[index + 3]) continue;
            if (pixels[index + 2] > pixels[index]) blue++;
            else red++;
        }
        document.getElementById('status').textContent =
            blue > 20 && red === 0 ? 'yes' : [blue, red].join(',');
    "#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}
