use super::*;

#[test]
fn canvas_matrix_methods_transform_real_pixels_and_restore_state() {
    let (dom, outcome) = execute_html(
        r#"<canvas width=6 height=6></canvas><output>no</output><script>
            const ctx = document.querySelector('canvas').getContext('2d');
            ctx.fillStyle = '#ff0000';
            ctx.save();
            ctx.translate(2, 1);
            const snapshot = ctx.getTransform();
            ctx.fillRect(0, 0, 1, 1);
            ctx.restore();
            ctx.fillStyle = '#00ff00';
            ctx.fillRect(0, 0, 1, 1);
            const pixel = (x, y) => Array.from(ctx.getImageData(x, y, 1, 1).data).join(',');
            const checks = [snapshot instanceof DOMMatrix,
                snapshot.e === 2 && snapshot.f === 1 && snapshot !== ctx.getTransform(),
                ctx.getTransform().isIdentity,
                pixel(2, 1) === '255,0,0,255', pixel(0, 0) === '0,255,0,255',
                pixel(1, 1) === '0,0,0,0'];
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
fn canvas_paths_capture_current_transform_and_path2d_transforms_at_paint() {
    let (dom, outcome) = execute_html(
        r#"<canvas width=6 height=3></canvas><output>no</output><script>
            const ctx = document.querySelector('canvas').getContext('2d');
            ctx.fillStyle = '#ff0000';
            ctx.beginPath(); ctx.rect(0, 0, 1, 1);
            ctx.translate(2, 0); ctx.fill();
            const path = new Path2D(); path.rect(0, 0, 1, 1);
            ctx.fillStyle = '#0000ff'; ctx.fill(path);
            const pixel = (x, y) => Array.from(ctx.getImageData(x, y, 1, 1).data).join(',');
            document.querySelector('output').textContent =
                pixel(0, 0) === '255,0,0,255' && pixel(2, 0) === '0,0,255,255' ? 'yes' :
                pixel(0, 0) + '|' + pixel(2, 0);
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn geometry_matrix_inverse_and_points_follow_column_vector_order() {
    let (dom, outcome) = execute_html(
        r#"<output>no</output><script>
            const matrix = new DOMMatrix().translate(10, 20).scale(2, 3);
            const point = new DOMPoint(4, 5);
            const mapped = matrix.transformPoint(point);
            const unmapped = matrix.inverse().transformPoint(mapped);
            const values = matrix.toFloat64Array();
            const checks = [mapped.x === 18, mapped.y === 35,
                Math.abs(unmapped.x - 4) < 1e-9, Math.abs(unmapped.y - 5) < 1e-9,
                values.length === 16, values[12] === 10, values[13] === 20,
                new DOMMatrix('matrix(1, 0, 0, 1, 3, 4)').e === 3];
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
fn geometry_matrix_3d_round_trips_points_and_array_forms() {
    let (dom, outcome) = execute_html(
        r#"<output>no</output><script>
            const matrix = new DOMMatrix().translate(3, 4, 5).scale(2, 3, 4);
            const point = matrix.transformPoint(new DOMPoint(1, 2, 3));
            const restored = matrix.inverse().transformPoint(point);
            const fromArray = DOMMatrix.fromFloat64Array(matrix.toFloat64Array());
            const checks = [!matrix.is2D, point.x === 5, point.y === 10, point.z === 17,
                Math.abs(restored.x - 1) < 1e-9, Math.abs(restored.y - 2) < 1e-9,
                Math.abs(restored.z - 3) < 1e-9, !fromArray.is2D,
                fromArray.m43 === 5, fromArray.toString().startsWith('matrix3d('),
                new DOMMatrix([1, 0, 0, 1, 2, 3]).is2D];
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
fn geometry_matrix_3d_rotation_and_mutation_obey_column_vector_order() {
    let (dom, outcome) = execute_html(
        r#"<output>no</output><script>
            const rotate = new DOMMatrix().rotateAxisAngle(0, 1, 0, 90);
            const point = rotate.transformPoint({x: 1, y: 0, z: 0});
            const matrix = new DOMMatrix();
            matrix.m34 = 0.5;
            const projected = new DOMPoint(0, 0, 2).matrixTransform(matrix);
            const singular = new DOMMatrix([0, 0, 0, 0, 0, 0]).inverse();
            const checks = [Math.abs(point.x) < 1e-9, Math.abs(point.z + 1) < 1e-9,
                !matrix.is2D, projected.w === 2, Number.isNaN(singular.m11),
                !singular.is2D, new DOMMatrix().skewX(45).c > 0.99];
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
fn geometry_quad_bounds_follow_mutated_corner_points() {
    let (dom, outcome) = execute_html(
        r#"<output>no</output><script>
            const quad = DOMQuad.fromRect({x: 2, y: 3, width: 4, height: 5});
            const initial = quad.getBounds();
            quad.p1.x = -1;
            quad.p3.y = 20;
            const current = quad.getBounds();
            const copy = DOMQuad.fromQuad(quad);
            const checks = [initial.x === 2, initial.y === 3, initial.width === 4,
                initial.height === 5, current.left === -1, current.bottom === 20,
                current.width === 7, current.height === 17,
                copy.p1 !== quad.p1, copy.p1.x === -1,
                JSON.stringify(quad).includes('"p4"')];
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
fn geometry_and_image_data_structured_clone_preserves_values_and_identity() {
    let (dom, outcome) = execute_html(
        r#"<output>no</output><script>
            const matrix = new DOMMatrix().translate(1, 2, 3);
            const point = new DOMPoint(NaN, -0, Infinity, 1);
            const quad = DOMQuad.fromRect({x: 2, y: 3, width: 4, height: 5});
            const pixels = new ImageData(2, 1);
            pixels.data.set([255, 0, 0, 255], 0);
            const copied = structuredClone({matrix, point, quad, pixels, again: matrix});
            const checks = [copied.matrix instanceof DOMMatrix, !copied.matrix.is2D,
                copied.matrix.m43 === 3, copied.matrix === copied.again,
                copied.matrix !== matrix, copied.point instanceof DOMPoint,
                Number.isNaN(copied.point.x), Object.is(copied.point.y, -0),
                copied.point.z === Infinity, copied.quad instanceof DOMQuad,
                copied.quad.getBounds().width === 4,
                copied.pixels instanceof ImageData, copied.pixels.data[0] === 255,
                copied.pixels.data !== pixels.data];
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
fn clipping_limits_fill_clear_and_image_drawing_to_the_saved_region() {
    let (dom, outcome) = execute_html(
        r#"<canvas id=target width=4 height=4></canvas><canvas id=source width=2 height=2></canvas>
            <output>no</output><script>
            const target = document.getElementById('target').getContext('2d');
            const source = document.getElementById('source').getContext('2d');
            source.fillStyle = '#0000ff'; source.fillRect(0, 0, 2, 2);
            target.fillStyle = '#ff0000'; target.fillRect(0, 0, 4, 4);
            target.save(); target.beginPath(); target.rect(1, 1, 2, 2); target.clip();
            target.drawImage(source.canvas, 0, 0, 4, 4);
            target.clearRect(2, 1, 1, 1);
            const pixel = (x, y) => Array.from(target.getImageData(x, y, 1, 1).data).join(',');
            const checks = [pixel(0, 0) === '255,0,0,255',
                pixel(1, 1) === '0,0,255,255', pixel(2, 1) === '0,0,0,0',
                pixel(3, 3) === '255,0,0,255'];
            target.restore(); target.fillStyle = '#00ff00'; target.fillRect(0, 0, 1, 1);
            checks.push(pixel(0, 0) === '0,255,0,255');
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
fn canvas_patterns_repeat_real_source_pixels_and_respect_pattern_matrix() {
    let (dom, outcome) = execute_html(
        r#"<canvas id=source width=2 height=1></canvas>
            <canvas id=target width=5 height=2></canvas><output>no</output><script>
            const source = document.getElementById('source').getContext('2d');
            source.fillStyle = '#ff0000'; source.fillRect(0, 0, 1, 1);
            source.fillStyle = '#0000ff'; source.fillRect(1, 0, 1, 1);
            const target = document.getElementById('target').getContext('2d');
            const pattern = target.createPattern(source.canvas, 'repeat-x');
            target.fillStyle = pattern; target.fillRect(0, 0, 5, 2);
            const pixel = (x, y) => Array.from(target.getImageData(x, y, 1, 1).data).join(',');
            const checks = [pattern instanceof CanvasPattern, target.fillStyle === pattern,
                pixel(0, 0) === '255,0,0,255', pixel(1, 0) === '0,0,255,255',
                pixel(2, 0) === '255,0,0,255', pixel(0, 1) === '0,0,0,0'];
            const shifted = target.createPattern(source.canvas, 'repeat');
            shifted.setTransform({e: 1});
            target.fillStyle = shifted; target.fillRect(0, 1, 5, 1);
            checks.push(pixel(1, 1) === '255,0,0,255', pixel(2, 1) === '0,0,255,255');
            let invalid = false;
            try { target.createPattern(source.canvas, 'invalid'); }
            catch (error) { invalid = error.name === 'SyntaxError'; }
            checks.push(invalid);
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
fn conic_gradient_paints_clockwise_and_transforms_with_the_context() {
    let (dom, outcome) = execute_html(
        r#"<canvas width=8 height=8></canvas><output>no</output><script>
            const ctx = document.querySelector('canvas').getContext('2d');
            const gradient = ctx.createConicGradient(0, 2, 2);
            gradient.addColorStop(0, '#ff0000');
            gradient.addColorStop(.25, '#00ff00');
            gradient.addColorStop(.5, '#0000ff');
            gradient.addColorStop(.75, '#ffff00');
            gradient.addColorStop(1, '#ff0000');
            ctx.fillStyle = gradient;
            ctx.fillRect(0, 0, 5, 5);
            const channel = (x, y, index) => ctx.getImageData(x, y, 1, 1).data[index];
            const checks = [channel(4, 2, 0) > 200, channel(2, 4, 1) > 200,
                channel(0, 2, 2) > 200, channel(2, 0, 0) > 200];
            ctx.clearRect(0, 0, 8, 8);
            ctx.translate(3, 0); ctx.fillRect(0, 0, 5, 5);
            checks.push(channel(7, 2, 0) > 200, channel(4, 2, 0) < 200);
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
fn put_image_data_dirty_rectangle_ignores_transform_clip_and_alpha() {
    let (dom, outcome) = execute_html(
        r#"<canvas width=4 height=2></canvas><output>no</output><script>
            const ctx = document.querySelector('canvas').getContext('2d');
            const data = new ImageData(3, 1);
            data.data.set([255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255]);
            ctx.translate(2, 0); ctx.globalAlpha = 0;
            ctx.beginPath(); ctx.rect(0, 1, 1, 1); ctx.clip();
            ctx.putImageData(data, 0, 0, 2, 0, -1, 1);
            const pixel = (x, y) => Array.from(ctx.getImageData(x, y, 1, 1).data).join(',');
            const checks = [pixel(0, 0) === '0,0,0,0',
                pixel(1, 0) === '0,255,0,255', pixel(2, 0) === '0,0,0,0'];
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}
