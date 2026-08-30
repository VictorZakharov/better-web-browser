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
