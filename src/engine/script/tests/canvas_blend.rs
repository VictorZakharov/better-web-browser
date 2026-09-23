use super::*;

#[test]
fn zero_area_draw_image_does_not_clear_destination_under_copy() {
    let (dom, outcome) = execute_html(
        r#"<canvas id=source width=1 height=1></canvas><canvas id=target width=2 height=1></canvas><output>no</output><script>
            const source = document.getElementById('source');
            source.getContext('2d').fillRect(0, 0, 1, 1);
            const ctx = document.getElementById('target').getContext('2d');
            ctx.fillStyle = '#ff0000'; ctx.fillRect(0, 0, 2, 1);
            ctx.globalCompositeOperation = 'copy';
            const returned = ctx.drawImage(source, 0, 0, 0, 1);
            const first = Array.from(ctx.getImageData(0, 0, 2, 1).data);
            ctx.drawImage(source, NaN, 0);
            const second = Array.from(ctx.getImageData(0, 0, 2, 1).data);
            const checks = [returned === undefined, first.join(',') === second.join(','),
                first.join(',') === '255,0,0,255,255,0,0,255'];
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
fn canvas_reset_clears_bitmap_path_and_drawing_state() {
    let (dom, outcome) = execute_html(
        r#"<canvas width=2 height=1></canvas><output>no</output><script>
            const ctx = document.querySelector('canvas').getContext('2d');
            ctx.fillStyle = 'red'; ctx.globalAlpha = 0.5; ctx.translate(1, 0);
            ctx.fillRect(0, 0, 1, 1); ctx.beginPath(); ctx.rect(0, 0, 1, 1);
            ctx.save(); ctx.reset();
            const pixel = Array.from(ctx.getImageData(1, 0, 1, 1).data);
            const checks = [pixel.join(',') === '0,0,0,0', ctx.fillStyle === '#000000',
                ctx.globalAlpha === 1, ctx.getTransform().e === 0,
                !ctx.isPointInPath(0.5, 0.5)];
            ctx.restore();
            checks.push(ctx.globalAlpha === 1, ctx.getTransform().e === 0);
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
fn porter_duff_modes_use_source_and_backdrop_alpha() {
    let (dom, outcome) = execute_html(
        r#"<canvas width=1 height=1></canvas><output>no</output><script>
            const ctx = document.querySelector('canvas').getContext('2d');
            const paint = mode => {
                ctx.clearRect(0, 0, 1, 1);
                ctx.globalCompositeOperation = 'source-over';
                ctx.fillStyle = '#ff0000'; ctx.fillRect(0, 0, 1, 1);
                ctx.globalCompositeOperation = mode;
                ctx.fillStyle = 'rgba(0, 0, 255, 0.5)'; ctx.fillRect(0, 0, 1, 1);
                return Array.from(ctx.getImageData(0, 0, 1, 1).data);
            };
            const sourceIn = paint('source-in');
            const destinationIn = paint('destination-in');
            const sourceOut = paint('source-out');
            const xor = paint('xor');
            const checks = [sourceIn[0] === 0, sourceIn[2] === 255, sourceIn[3] === 128,
                destinationIn[0] === 255, destinationIn[2] === 0, destinationIn[3] === 128,
                sourceOut[3] === 0, xor[0] === 255, xor[3] >= 127 && xor[3] <= 128];
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
fn separable_blend_modes_change_real_pixels() {
    let (dom, outcome) = execute_html(
        r#"<canvas width=1 height=1></canvas><output>no</output><script>
            const ctx = document.querySelector('canvas').getContext('2d');
            const sample = mode => {
                ctx.clearRect(0, 0, 1, 1);
                ctx.globalCompositeOperation = 'source-over';
                ctx.fillStyle = 'rgb(64, 128, 192)'; ctx.fillRect(0, 0, 1, 1);
                ctx.globalCompositeOperation = mode;
                ctx.fillStyle = 'rgb(192, 128, 64)'; ctx.fillRect(0, 0, 1, 1);
                return Array.from(ctx.getImageData(0, 0, 1, 1).data);
            };
            const dark = sample('darken'), light = sample('lighten');
            const difference = sample('difference'), exclusion = sample('exclusion');
            const checks = [dark.join(',') === '64,128,64,255',
                light.join(',') === '192,128,192,255',
                difference[0] === 128, difference[1] === 0, difference[2] === 128,
                exclusion[0] > difference[0], exclusion[1] > 0,
                ctx.globalCompositeOperation === 'exclusion'];
            ctx.globalCompositeOperation = 'unknown';
            checks.push(ctx.globalCompositeOperation === 'exclusion');
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
fn copy_and_source_in_clear_backdrop_outside_source_but_not_outside_clip() {
    let (dom, outcome) = execute_html(
        r#"<canvas width=4 height=2></canvas><output>no</output><script>
            const ctx = document.querySelector('canvas').getContext('2d');
            const pixel = (x, y) => Array.from(ctx.getImageData(x, y, 1, 1).data).join(',');
            ctx.fillStyle = '#ff0000'; ctx.fillRect(0, 0, 4, 2);
            ctx.globalCompositeOperation = 'copy';
            ctx.fillStyle = '#0000ff'; ctx.fillRect(1, 0, 1, 1);
            const checks = [pixel(0, 0) === '0,0,0,0', pixel(1, 0) === '0,0,255,255',
                pixel(3, 1) === '0,0,0,0'];
            ctx.globalCompositeOperation = 'source-over';
            ctx.fillStyle = '#ff0000'; ctx.fillRect(0, 0, 4, 2);
            ctx.save(); ctx.beginPath(); ctx.rect(0, 0, 2, 2); ctx.clip();
            ctx.globalCompositeOperation = 'source-in';
            ctx.fillStyle = '#0000ff'; ctx.fillRect(1, 0, 1, 1);
            checks.push(pixel(0, 0) === '0,0,0,0',
                pixel(1, 0) === '0,0,255,255', pixel(3, 0) === '255,0,0,255');
            ctx.restore();
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
fn source_layer_preserves_self_draw_and_stroke_rect_does_not_change_current_path() {
    let (dom, outcome) = execute_html(
        r#"<canvas width=3 height=2></canvas><output>no</output><script>
            const ctx = document.querySelector('canvas').getContext('2d');
            ctx.fillStyle = '#ff0000'; ctx.fillRect(0, 0, 1, 1);
            ctx.globalCompositeOperation = 'copy';
            ctx.drawImage(ctx.canvas, 1, 0);
            const pixel = (x, y) => Array.from(ctx.getImageData(x, y, 1, 1).data).join(',');
            const checks = [pixel(0, 0) === '0,0,0,0', pixel(1, 0) === '255,0,0,255'];
            ctx.globalCompositeOperation = 'source-over';
            ctx.beginPath(); ctx.rect(0, 0, 1, 1);
            ctx.strokeStyle = '#0000ff'; ctx.strokeRect(1, 0, 1, 1);
            ctx.fillStyle = '#00ff00'; ctx.fill();
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
fn nonseparable_blends_preserve_the_specified_luminosity() {
    let (dom, outcome) = execute_html(
        r#"<canvas width=1 height=1></canvas><output>no</output><script>
            const ctx = document.querySelector('canvas').getContext('2d');
            const paint = mode => {
                ctx.clearRect(0, 0, 1, 1);
                ctx.globalCompositeOperation = 'source-over';
                ctx.fillStyle = 'rgb(60, 120, 180)'; ctx.fillRect(0, 0, 1, 1);
                ctx.globalCompositeOperation = mode;
                ctx.fillStyle = 'rgb(200, 40, 80)'; ctx.fillRect(0, 0, 1, 1);
                return Array.from(ctx.getImageData(0, 0, 1, 1).data);
            };
            const lum = rgb => .3 * rgb[0] + .59 * rgb[1] + .11 * rgb[2];
            const original = [60, 120, 180];
            const hue = paint('hue'), color = paint('color');
            const luminosity = paint('luminosity');
            const checks = [Math.abs(lum(hue) - lum(original)) < 2,
                Math.abs(lum(color) - lum(original)) < 2,
                Math.abs(lum(luminosity) - lum([200, 40, 80])) < 2,
                hue[3] === 255, color[3] === 255,
                paint('saturation')[3] === 255];
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}
