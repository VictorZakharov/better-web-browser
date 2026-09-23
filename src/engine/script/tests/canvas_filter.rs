use super::*;

#[test]
fn canvas_color_filters_apply_in_order_to_drawn_pixels() {
    let (dom, outcome) = execute_html(
        r#"<canvas width=3 height=1></canvas><output>no</output><script>
            const ctx = document.querySelector('canvas').getContext('2d');
            ctx.save();
            ctx.filter = 'brightness(50%) invert(1)';
            ctx.fillStyle = '#ff0000'; ctx.fillRect(0, 0, 1, 1);
            const first = ctx.getImageData(0, 0, 1, 1).data;
            const checks = [first[0] >= 126 && first[0] <= 128,
                first[1] === 255, first[2] === 255, first[3] === 255];
            ctx.filter = 'not-a-filter(1)';
            checks.push(ctx.filter === 'brightness(50%) invert(1)');
            ctx.restore();
            checks.push(ctx.filter === 'none');
            ctx.filter = 'opacity(0.5)';
            ctx.fillRect(1, 0, 1, 1);
            const second = ctx.getImageData(1, 0, 1, 1).data;
            checks.push(second[0] === 0, second[3] >= 127 && second[3] <= 128);
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
fn canvas_blur_filter_expands_source_alpha_without_dark_fringes() {
    let (dom, outcome) = execute_html(
        r#"<canvas width=7 height=3></canvas><output>no</output><script>
            const ctx = document.querySelector('canvas').getContext('2d');
            ctx.fillStyle = '#ff0000'; ctx.filter = 'blur(1px)';
            ctx.fillRect(3, 1, 1, 1);
            const center = ctx.getImageData(3, 1, 1, 1).data;
            const edge = ctx.getImageData(2, 1, 1, 1).data;
            const outside = ctx.getImageData(0, 1, 1, 1).data;
            const checks = [center[0] === 255, center[3] > 0,
                edge[0] === 255, edge[3] > 0, outside[3] === 0];
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
fn drop_shadow_filter_keeps_source_and_offsets_its_tinted_alpha() {
    let (dom, outcome) = execute_html(
        r#"<canvas width=6 height=3></canvas><output>no</output><script>
            const ctx = document.querySelector('canvas').getContext('2d');
            ctx.filter = 'drop-shadow(2px 0px 0px #0000ff)';
            ctx.fillStyle = '#ff0000'; ctx.fillRect(1, 1, 1, 1);
            const pixel = (x, y) => Array.from(ctx.getImageData(x, y, 1, 1).data).join(',');
            const checks = [pixel(1, 1) === '255,0,0,255',
                pixel(3, 1) === '0,0,255,255', pixel(0, 1) === '0,0,0,0'];
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
fn filter_contrast_can_exceed_one_and_drop_shadow_color_can_precede_lengths() {
    let (dom, outcome) = execute_html(
        r#"<canvas width=6 height=2></canvas><output>no</output><script>
            const ctx = document.querySelector('canvas').getContext('2d');
            ctx.fillStyle = '#404040'; ctx.filter = 'contrast(200%)';
            ctx.fillRect(0, 0, 1, 1);
            const contrasted = Array.from(ctx.getImageData(0, 0, 1, 1).data);
            const checks = [contrasted[0] <= 1, contrasted[1] <= 1,
                contrasted[2] <= 1, contrasted[3] === 255];
            ctx.filter = 'drop-shadow(rgba(0, 0, 255, 1) 2px 0 0)';
            ctx.fillStyle = '#ff0000'; ctx.fillRect(1, 1, 1, 1);
            const shadow = Array.from(ctx.getImageData(3, 1, 1, 1).data);
            checks.push(shadow.join(',') === '0,0,255,255');
            ctx.filter = 'drop-shadow(red 1px 0 -1px)';
            checks.push(ctx.filter === 'drop-shadow(rgba(0, 0, 255, 1) 2px 0 0)');
            ctx.filter = 'hue-rotate(0)';
            checks.push(ctx.filter === 'hue-rotate(0)');
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}
