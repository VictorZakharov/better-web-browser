use super::*;

#[test]
fn canvas_line_caps_extend_stroke_ends_and_hit_testing() {
    let (dom, outcome) = execute_html(
        r#"<canvas width=10 height=6></canvas><output>no</output><script>
            const ctx = document.querySelector('canvas').getContext('2d');
            const path = new Path2D(); path.moveTo(2, 2); path.lineTo(6, 2);
            ctx.lineWidth = 2;
            const alpha = (x, y) => ctx.getImageData(x, y, 1, 1).data[3];
            ctx.lineCap = 'butt'; ctx.stroke(path);
            const checks = [alpha(1, 2) === 0, !ctx.isPointInStroke(path, 1.5, 2.5)];
            ctx.clearRect(0, 0, 10, 6);
            ctx.lineCap = 'round'; ctx.stroke(path);
            checks.push(alpha(1, 2) === 255, ctx.isPointInStroke(path, 1.5, 2.5));
            ctx.clearRect(0, 0, 10, 6);
            ctx.lineCap = 'square'; ctx.stroke(path);
            checks.push(alpha(1, 2) === 255, ctx.isPointInStroke(path, 1.5, 2.5));
            ctx.lineCap = 'invalid'; checks.push(ctx.lineCap === 'square');
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn miter_limit_controls_outer_corner_pixels_and_state_restores() {
    let (dom, outcome) = execute_html(
        r#"<canvas width=10 height=10></canvas><output>no</output><script>
            const ctx = document.querySelector('canvas').getContext('2d');
            const path = new Path2D(); path.moveTo(1, 4); path.lineTo(4, 4); path.lineTo(4, 7);
            ctx.lineWidth = 4; ctx.save();
            ctx.lineJoin = 'bevel'; ctx.stroke(path);
            const alpha = (x, y) => ctx.getImageData(x, y, 1, 1).data[3];
            const checks = [alpha(5, 2) === 0, !ctx.isPointInStroke(path, 5.5, 2.5)];
            ctx.clearRect(0, 0, 10, 10);
            ctx.lineJoin = 'miter'; ctx.miterLimit = 10; ctx.stroke(path);
            checks.push(alpha(5, 2) === 255, ctx.isPointInStroke(path, 5.5, 2.5));
            ctx.clearRect(0, 0, 10, 10);
            ctx.miterLimit = 1; ctx.stroke(path);
            checks.push(alpha(5, 2) === 0);
            ctx.restore();
            checks.push(ctx.lineJoin === 'miter', ctx.miterLimit === 10);
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}
