use super::*;

#[test]
fn svg_path_curves_and_shorthand_match_canvas_commands() {
    let (dom, outcome) = execute_html(
        r#"<canvas width=24 height=24></canvas><output>no</output><script>
            const ctx = document.querySelector('canvas').getContext('2d');
            const svg = new Path2D('M2 2 C4 2 6 4 8 4 S12 6 14 4 Q16 2 18 4 T22 4 L22 20 H2 Z');
            const manual = new Path2D();
            manual.moveTo(2, 2);
            manual.bezierCurveTo(4, 2, 6, 4, 8, 4);
            manual.bezierCurveTo(10, 4, 12, 6, 14, 4);
            manual.quadraticCurveTo(16, 2, 18, 4);
            manual.quadraticCurveTo(20, 6, 22, 4);
            manual.lineTo(22, 20); manual.lineTo(2, 20); manual.closePath();
            ctx.fill(svg);
            const first = Array.from(ctx.getImageData(0, 0, 24, 24).data);
            ctx.clearRect(0, 0, 24, 24);
            ctx.fill(manual);
            const second = Array.from(ctx.getImageData(0, 0, 24, 24).data);
            document.querySelector('output').textContent =
                first.every((value, index) => value === second[index]) ? 'yes' : 'pixels differ';
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn svg_path_relative_commands_and_compact_arc_flags_draw_pixels() {
    let (dom, outcome) = execute_html(
        r#"<canvas width=9 height=7></canvas><output>no</output><script>
            const ctx = document.querySelector('canvas').getContext('2d');
            ctx.fillStyle = '#ff0000';
            ctx.fill(new Path2D('M2 2a2 2 0 014 0l0 3h-4z'));
            const alpha = (x, y) => ctx.getImageData(x, y, 1, 1).data[3];
            const checks = [alpha(4, 0) === 255, alpha(4, 4) === 255,
                alpha(4, 5) === 0, alpha(0, 4) === 0, alpha(7, 4) === 0];
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
fn svg_path_rejects_malformed_data_and_respects_geometry_budget() {
    let (dom, outcome) = execute_html(
        r#"<output>no</output><script>
            const rejects = (source, name) => {
                try { new Path2D(source); return false; }
                catch (error) { return error.name === name; }
            };
            const checks = [rejects('L2 2', 'SyntaxError'), rejects('M2', 'SyntaxError'),
                rejects('M0 0 A2 2 0 2 1 4 0', 'SyntaxError'),
                rejects('M0 0 X2 2', 'SyntaxError'),
                rejects('M0 0 Z 1 2', 'SyntaxError'),
                rejects('M0 0' + ' L1 1'.repeat(8200), 'NotSupportedError')];
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
fn rounded_rect_and_arc_to_have_real_bounded_geometry() {
    let (dom, outcome) = execute_html(
        r#"<canvas width=10 height=10></canvas><output>no</output><script>
            const ctx = document.querySelector('canvas').getContext('2d');
            const round = new Path2D(); round.roundRect(1, 1, 6, 6, 2);
            ctx.fill(round);
            const alpha = (x, y) => ctx.getImageData(x, y, 1, 1).data[3];
            const checks = [alpha(1, 1) === 0, alpha(4, 1) === 255,
                alpha(4, 4) === 255, alpha(8, 8) === 0];
            ctx.clearRect(0, 0, 10, 10);
            const arc = new Path2D(); arc.moveTo(0, 0); arc.arcTo(4, 0, 4, 4, 2);
            ctx.lineWidth = 1; ctx.stroke(arc);
            checks.push(alpha(2, 0) === 255, alpha(3, 1) === 255,
                alpha(4, 0) === 0);
            let negative = false, empty = false;
            try { new Path2D().roundRect(0, 0, 2, 2, -1); }
            catch (error) { negative = error instanceof RangeError; }
            try { new Path2D().roundRect(0, 0, 2, 2, []); }
            catch (error) { empty = error instanceof RangeError; }
            checks.push(negative, empty);
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}
