use super::*;

#[test]
fn canvas_2d_exposes_bounded_real_pixels_and_standard_identity() {
    let (dom, outcome) = execute_html(
        r#"<body><output id="status">waiting</output><canvas id="canvas"></canvas><script>
            const canvas = document.getElementById('canvas');
            const context = canvas.getContext('2d');
            const checks = [
                canvas instanceof HTMLCanvasElement,
                context instanceof CanvasRenderingContext2D,
                context.canvas === canvas,
                canvas.width === 300 && canvas.height === 150,
                canvas.getContext('2d') === context,
                canvas.getContext('webgl') === null,
                context.fillStyle === '#000000'
            ];
            canvas.width = 2;
            canvas.height = 2;
            context.fillStyle = 'red';
            context.fillRect(0, 0, 2, 2);
            const red = context.getImageData(0, 0, 1, 1);
            checks.push(red instanceof ImageData, red.width === 1, red.height === 1,
                [...red.data].join(',') === '255,0,0,255');
            context.globalAlpha = 0.5;
            context.fillStyle = 'rgba(0, 0, 255, 0.5)';
            context.fillRect(0, 0, 1, 1);
            checks.push([...context.getImageData(0, 0, 1, 1).data].join(',') === '191,0,64,255');
            context.clearRect(1, 1, 1, 1);
            checks.push([...context.getImageData(1, 1, 1, 1).data].join(',') === '0,0,0,0');
            const previous = context.fillStyle;
            context.fillStyle = 'definitely-not-a-color';
            checks.push(context.fillStyle === previous);
            document.getElementById('status').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn canvas_2d_resets_on_resize_and_rejects_unbounded_pixel_reads() {
    let (dom, outcome) = execute_html(
        r#"<body><output id="status">waiting</output><canvas id="canvas" width="1" height="1"></canvas><script>
            const canvas = document.getElementById('canvas');
            const context = canvas.getContext('2d');
            context.fillStyle = '#00ff00';
            context.fillRect(0, 0, 1, 1);
            canvas.width = 2;
            const reset = context.fillStyle === '#000000' &&
                [...context.getImageData(0, 0, 1, 1).data].join(',') === '0,0,0,0';
            const image = context.createImageData(1, 1);
            image.data.set([7, 8, 9, 255]);
            context.putImageData(image, 1, 0);
            const copied = [...context.getImageData(1, 0, 1, 1).data].join(',') === '7,8,9,255';
            canvas.width = 5000;
            canvas.height = 5000;
            let error = '';
            try { context.getImageData(0, 0, 1, 1); } catch (caught) { error = caught.name; }
            document.getElementById('status').textContent = [reset, copied, error].join(',');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true,true,NotSupportedError"
    );
}

#[test]
fn canvas_serializes_real_png_jpeg_and_webp_bitmaps_with_png_fallback() {
    let (dom, outcome) = execute_html(
        r#"<body><output>waiting</output><canvas width="2" height="1"></canvas><script>
            const canvas = document.querySelector('canvas');
            const context = canvas.getContext('2d');
            context.fillStyle = '#ff0000'; context.fillRect(0, 0, 1, 1);
            context.fillStyle = '#0000ff'; context.fillRect(1, 0, 1, 1);
            const png = canvas.toDataURL();
            const jpeg = canvas.toDataURL('image/jpeg', 1);
            const webp = canvas.toDataURL('image/webp');
            const fallback = canvas.toDataURL('image/unsupported');
            const bytes = atob(png.split(',')[1]);
            const checks = [
                png.startsWith('data:image/png;base64,'),
                bytes.charCodeAt(0) === 137 && bytes.slice(1, 4) === 'PNG',
                jpeg.startsWith('data:image/jpeg;base64,'),
                webp.startsWith('data:image/webp;base64,'),
                fallback.startsWith('data:image/png;base64,'),
                png !== fallback || fallback.length > 100
            ];
            canvas.width = 0;
            checks.push(canvas.toDataURL() === 'data:,');
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn canvas_to_blob_queues_callback_with_snapshot_and_fallback_mime() {
    let (dom, outcome) = execute_html(
        r#"<body><output>waiting</output><canvas width="1" height="1"></canvas><script>
            const canvas = document.querySelector('canvas');
            canvas.getContext('2d').fillRect(0, 0, 1, 1);
            let synchronous = true;
            canvas.toBlob(blob => {
                document.querySelector('output').textContent = [
                    !synchronous, blob instanceof Blob, blob.type, blob.size > 0,
                    canvas.width === 0
                ].join(',');
            }, 'image/unsupported');
            canvas.width = 0;
            synchronous = false;
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true,true,image/png,true,true"
    );
}

#[test]
fn canvas_composite_modes_change_pixels_and_follow_state_stack() {
    let (dom, outcome) = execute_html(
        r#"<body><output>waiting</output><canvas width="1" height="1"></canvas><script>
            const context = document.querySelector('canvas').getContext('2d');
            const pixel = () => [...context.getImageData(0, 0, 1, 1).data].join(',');
            context.fillStyle = '#ffffff'; context.fillRect(0, 0, 1, 1);
            context.save();
            context.globalCompositeOperation = 'screen';
            context.fillStyle = '#000000'; context.fillRect(0, 0, 1, 1);
            const screen = pixel() === '255,255,255,255';
            context.globalCompositeOperation = 'invalid';
            const invalidIgnored = context.globalCompositeOperation === 'screen';
            context.restore();
            const restored = context.globalCompositeOperation === 'source-over';
            context.globalCompositeOperation = 'multiply';
            context.fillStyle = '#808080'; context.fillRect(0, 0, 1, 1);
            const multiply = pixel() === '128,128,128,255';
            context.globalCompositeOperation = 'copy';
            context.fillStyle = 'rgba(255, 0, 0, 0.5)'; context.fillRect(0, 0, 1, 1);
            const copy = pixel() === '255,0,0,128';
            context.globalCompositeOperation = 'destination-over';
            context.fillStyle = '#0000ff'; context.fillRect(0, 0, 1, 1);
            const destinationOver = pixel() === '128,0,127,255';
            context.globalCompositeOperation = 'lighter';
            context.fillStyle = '#008000'; context.fillRect(0, 0, 1, 1);
            const lighter = pixel() === '128,128,127,255';
            document.querySelector('output').textContent =
                [screen,invalidIgnored,restored,multiply,copy,destinationOver,lighter].join(',');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true,true,true,true,true,true,true"
    );
}

#[test]
fn canvas_paths_fill_real_pixels_with_nonzero_and_evenodd_rules() {
    let (dom, outcome) = execute_html(
        r#"<body><output>waiting</output><canvas width="12" height="12"></canvas><script>
            const context = document.querySelector('canvas').getContext('2d');
            const pixel = (x, y) => context.getImageData(x, y, 1, 1).data[3];
            const ring = new Path2D();
            ring.rect(1, 1, 10, 10);
            ring.rect(3, 3, 6, 6);
            context.fillStyle = '#ff0000';
            context.fill(ring, 'evenodd');
            const checks = [pixel(2, 2) === 255, pixel(5, 5) === 0, pixel(0, 0) === 0,
                context.isPointInPath(ring, 5, 5, 'evenodd') === false,
                context.isPointInPath(ring, 2, 2, 'evenodd') === true];
            const copy = new Path2D(ring);
            context.fill(copy, 'nonzero');
            checks.push(pixel(5, 5) === 255);
            context.clearRect(0, 0, 12, 12);
            context.beginPath();
            context.moveTo(1, 1); context.lineTo(10, 1); context.lineTo(1, 10);
            context.closePath(); context.fill();
            checks.push(pixel(2, 2) === 255, pixel(10, 10) === 0);
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn canvas_ellipse_stroke_and_dash_have_observable_geometry() {
    let (dom, outcome) = execute_html(
        r#"<body><output>waiting</output><canvas width="20" height="20"></canvas><script>
            const context = document.querySelector('canvas').getContext('2d');
            const alpha = (x, y) => context.getImageData(x, y, 1, 1).data[3];
            context.beginPath(); context.ellipse(10, 10, 6, 3, 0, 0, 2 * Math.PI);
            context.closePath(); context.fill();
            const checks = [alpha(10, 10) === 255, alpha(10, 5) === 0,
                context.isPointInPath(10, 10), !context.isPointInPath(10, 5)];
            context.clearRect(0, 0, 20, 20);
            context.beginPath(); context.moveTo(1, 2); context.lineTo(18, 2);
            context.strokeStyle = 'blue'; context.lineWidth = 2;
            context.setLineDash([4, 4]); context.stroke();
            checks.push(alpha(2, 2) === 255, alpha(7, 2) === 0,
                context.isPointInStroke(2, 2), !context.isPointInStroke(7, 2));
            context.save(); context.setLineDash([2]); context.lineWidth = 5; context.restore();
            checks.push(context.getLineDash().join(',') === '4,4', context.lineWidth === 2);
            let radiusError = '';
            try { context.ellipse(0, 0, -1, 1, 0, 0, 1); } catch (error) { radiusError = error.name; }
            checks.push(radiusError === 'IndexSizeError');
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn canvas_path2d_parses_svg_lines_and_rejects_invalid_commands() {
    let (dom, outcome) = execute_html(
        r#"<body><output>waiting</output><canvas width="8" height="8"></canvas><script>
            const context = document.querySelector('canvas').getContext('2d');
            const path = new Path2D('M1 1 h5 v5 h-5 z');
            context.fill(path);
            let error = '';
            try { new Path2D('M1 1 X2 2'); } catch (caught) { error = caught.name; }
            document.querySelector('output').textContent = [
                context.getImageData(3, 3, 1, 1).data[3] === 255,
                context.getImageData(0, 0, 1, 1).data[3] === 0,
                error === 'SyntaxError'
            ].join(',');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true,true,true"
    );
}

#[test]
fn canvas_gradients_interpolate_real_pixels_and_keep_live_color_stops() {
    let (dom, outcome) = execute_html(
        r#"<body><output>waiting</output><canvas width="10" height="10"></canvas><script>
            const context = document.querySelector('canvas').getContext('2d');
            const pixel = (x, y) => [...context.getImageData(x, y, 1, 1).data];
            const linear = context.createLinearGradient(0.5, 0, 2.5, 0);
            context.fillStyle = linear;
            linear.addColorStop(0, 'rgba(255, 0, 0, 0)');
            linear.addColorStop(1, '#0000ff');
            context.fillRect(0, 0, 3, 1);
            const midpoint = pixel(1, 0);
            const checks = [linear instanceof CanvasGradient, context.fillStyle === linear,
                pixel(0, 0)[3] === 0, midpoint[2] === 255,
                midpoint[3] >= 127 && midpoint[3] <= 128,
                pixel(2, 0).join(',') === '0,0,255,255'];
            let constructorError = '';
            try { new CanvasGradient(); } catch (error) { constructorError = error.name; }
            checks.push(constructorError === 'TypeError');
            const radial = context.createRadialGradient(4.5, 4.5, 0, 4.5, 4.5, 3);
            radial.addColorStop(0, 'red'); radial.addColorStop(1, 'blue');
            context.fillStyle = radial; context.fillRect(1, 1, 7, 7);
            checks.push(pixel(4, 4).join(',') === '255,0,0,255',
                pixel(7, 4).join(',') === '0,0,255,255');
            let rangeError = '', colorError = '', radiusError = '';
            try { radial.addColorStop(2, 'red'); } catch (error) { rangeError = error.name; }
            try { radial.addColorStop(0.5, 'bogus color'); } catch (error) { colorError = error.name; }
            try { context.createRadialGradient(0, 0, -1, 0, 0, 1); } catch (error) { radiusError = error.name; }
            checks.push(rangeError === 'IndexSizeError', colorError === 'SyntaxError', radiusError === 'IndexSizeError');
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn canvas_path2d_add_path_applies_transform_without_mutating_source() {
    let (dom, outcome) = execute_html(
        r#"<body><output>waiting</output><canvas width="8" height="8"></canvas><script>
            const context = document.querySelector('canvas').getContext('2d');
            const source = new Path2D(); source.rect(0, 0, 2, 2);
            const moved = new Path2D(); moved.addPath(source, { e: 4, f: 3 });
            context.fill(moved);
            const checks = [context.getImageData(4, 3, 1, 1).data[3] === 255,
                context.getImageData(0, 0, 1, 1).data[3] === 0,
                context.isPointInPath(source, 0.5, 0.5), !context.isPointInPath(source, 4.5, 3.5)];
            document.querySelector('output').textContent = checks.join(',');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true,true,true,true"
    );
}

#[test]
fn canvas_empty_export_is_asynchronous_and_path_geometry_is_bounded() {
    let (dom, outcome) = execute_html(
        r#"<body><output>waiting</output><canvas width="0" height="1"></canvas><script>
            const canvas = document.querySelector('canvas');
            const path = new Path2D();
            let geometryError = '';
            try {
                path.moveTo(0, 0);
                for (let point = 1; point < 9000; point++) path.lineTo(point, 0);
            } catch (error) { geometryError = error.name; }
            let synchronous = true;
            canvas.toBlob(blob => {
                document.querySelector('output').textContent = [
                    !synchronous, blob === null, canvas.toDataURL() === 'data:,',
                    geometryError === 'NotSupportedError'
                ].join(',');
            });
            synchronous = false;
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true,true,true,true"
    );
}

#[test]
fn canvas_duplicate_gradient_stops_and_invalid_dashes_follow_state_rules() {
    let (dom, outcome) = execute_html(
        r#"<body><output>waiting</output><canvas width="3" height="2"></canvas><script>
            const context = document.querySelector('canvas').getContext('2d');
            const gradient = context.createLinearGradient(0.5, 0, 2.5, 0);
            gradient.addColorStop(0, 'red');
            gradient.addColorStop(0.5, 'red');
            gradient.addColorStop(0.5, 'blue');
            gradient.addColorStop(1, 'blue');
            context.fillStyle = gradient;
            context.fillRect(0, 0, 3, 1);
            const pixel = (x, y) => [...context.getImageData(x, y, 1, 1).data].join(',');
            const checks = [pixel(0, 0) === '255,0,0,255', pixel(1, 0) === '0,0,255,255'];
            context.setLineDash([2, 1, 3]);
            checks.push(context.getLineDash().join(',') === '2,1,3,2,1,3');
            context.setLineDash([1, -1]);
            checks.push(context.getLineDash().join(',') === '2,1,3,2,1,3');
            context.save(); context.lineDashOffset = 3; context.restore();
            checks.push(context.lineDashOffset === 0);
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}
