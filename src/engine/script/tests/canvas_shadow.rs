use super::*;

#[test]
fn canvas_shadows_follow_source_alpha_and_saved_state() {
    let (dom, outcome) = execute_html(
        r#"<canvas width=6 height=4></canvas><output>no</output><script>
            const ctx = document.querySelector('canvas').getContext('2d');
            ctx.save();
            ctx.shadowColor = '#0000ff';
            ctx.shadowOffsetX = 2;
            ctx.fillStyle = '#ff0000';
            ctx.fillRect(1, 1, 1, 1);
            const pixel = (x, y) => Array.from(ctx.getImageData(x, y, 1, 1).data).join(',');
            const checks = [pixel(1, 1) === '255,0,0,255',
                pixel(3, 1) === '0,0,255,255', pixel(4, 1) === '0,0,0,0'];
            ctx.restore();
            ctx.fillRect(1, 2, 1, 1);
            checks.push(pixel(3, 2) === '0,0,0,0', ctx.shadowOffsetX === 0,
                ctx.shadowBlur === 0);
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
fn blurred_path_and_image_shadows_produce_bounded_pixels() {
    let (dom, outcome) = execute_html(
        r#"<canvas id=source width=1 height=1></canvas>
            <canvas id=target width=8 height=8></canvas><output>no</output><script>
            const source = document.getElementById('source').getContext('2d');
            source.fillStyle = '#ff0000'; source.fillRect(0, 0, 1, 1);
            const ctx = document.getElementById('target').getContext('2d');
            ctx.shadowColor = 'rgba(0, 0, 255, 0.5)';
            ctx.shadowOffsetX = 1; ctx.shadowBlur = 1;
            ctx.fillStyle = '#ff0000';
            const path = new Path2D(); path.rect(2, 2, 1, 1);
            ctx.fill(path);
            const blurred = ctx.getImageData(4, 2, 1, 1).data;
            ctx.drawImage(source.canvas, 2, 5);
            const imageShadow = ctx.getImageData(4, 5, 1, 1).data;
            const checks = [blurred[2] > 0, blurred[3] > 0,
                imageShadow[2] > 0, imageShadow[3] > 0];
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}
