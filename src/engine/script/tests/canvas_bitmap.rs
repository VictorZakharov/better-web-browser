use super::*;
use std::sync::Arc;

fn run_canvas_case(body: &str, expected: &str) {
    let (dom, outcome) = execute_html(body);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        expected
    );
}

#[test]
fn offscreen_canvas_transfers_real_pixels_and_resets_its_bitmap() {
    run_canvas_case(
        r#"<body><output>waiting</output><canvas width="2" height="1"></canvas><script>
            const offscreen = new OffscreenCanvas(2, 1);
            const context = offscreen.getContext('2d');
            const checks = [context instanceof OffscreenCanvasRenderingContext2D,
                context instanceof CanvasRenderingContext2D, context.canvas === offscreen,
                offscreen.getContext('2d') === context, offscreen.getContext('webgl') === null];
            context.fillStyle = 'red'; context.fillRect(0, 0, 1, 1);
            context.fillStyle = 'blue'; context.fillRect(1, 0, 1, 1);
            const bitmap = offscreen.transferToImageBitmap();
            checks.push(bitmap instanceof ImageBitmap, bitmap.width === 2, bitmap.height === 1,
                context.getImageData(0, 0, 1, 1).data[3] === 0);
            const target = document.querySelector('canvas').getContext('2d');
            target.drawImage(bitmap, 0, 0);
            checks.push([...target.getImageData(0, 0, 2, 1).data].join(',') ===
                '255,0,0,255,0,0,255,255');
            bitmap.close();
            checks.push(bitmap.width === 0, bitmap.height === 0);
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        </script></body>"#,
        "yes",
    );
}

#[test]
fn bitmaprenderer_consumes_imagebitmap_and_owns_canvas_pixels() {
    run_canvas_case(
        r#"<body><output>waiting</output><canvas width="2" height="1"></canvas><script>
            const source = new OffscreenCanvas(2, 1);
            source.getContext('2d').fillRect(0, 0, 2, 1);
            const bitmap = source.transferToImageBitmap();
            const canvas = document.querySelector('canvas');
            const renderer = canvas.getContext('bitmaprenderer');
            const checks = [renderer instanceof ImageBitmapRenderingContext,
                renderer.canvas === canvas, canvas.getContext('2d') === null];
            renderer.transferFromImageBitmap(bitmap);
            checks.push(bitmap.width === 0, bitmap.height === 0,
                canvas.toDataURL().startsWith('data:image/png;base64,'));
            const copy = new OffscreenCanvas(2, 1);
            copy.getContext('2d').drawImage(canvas, 0, 0);
            checks.push([...copy.getContext('2d').getImageData(0, 0, 1, 1).data].join(',') ===
                '0,0,0,255');
            renderer.transferFromImageBitmap(null);
            copy.getContext('2d').clearRect(0, 0, 2, 1);
            copy.getContext('2d').drawImage(canvas, 0, 0);
            checks.push(copy.getContext('2d').getImageData(0, 0, 1, 1).data[3] === 0);
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        </script></body>"#,
        "yes",
    );
}

#[test]
fn create_image_bitmap_crops_resizes_and_flips_canvas_snapshots() {
    run_canvas_case(
        r#"<body><output>waiting</output><canvas width="2" height="2"></canvas><script>
            const canvas = document.querySelector('canvas');
            const context = canvas.getContext('2d');
            context.fillStyle = 'red'; context.fillRect(0, 0, 1, 1);
            context.fillStyle = 'blue'; context.fillRect(1, 1, 1, 1);
            const pending = createImageBitmap(canvas, 0, 0, 2, 2,
                { resizeWidth: 4, resizeHeight: 4, resizeQuality: 'pixelated', imageOrientation: 'flipY' });
            context.clearRect(0, 0, 2, 2);
            pending.then(bitmap => {
                const target = new OffscreenCanvas(4, 4);
                const paint = target.getContext('2d');
                paint.imageSmoothingEnabled = false;
                paint.drawImage(bitmap, 0, 0);
                document.querySelector('output').textContent = [
                    bitmap.width === 4, bitmap.height === 4,
                    [...paint.getImageData(0, 3, 1, 1).data].join(',') === '255,0,0,255',
                    [...paint.getImageData(3, 0, 1, 1).data].join(',') === '0,0,255,255'
                ].join(',');
            });
        </script></body>"#,
        "true,true,true,true",
    );
}

#[test]
fn offscreen_convert_to_blob_snapshots_real_encoded_pixels() {
    run_canvas_case(
        r#"<body><output>waiting</output><script>
            const canvas = new OffscreenCanvas(1, 1);
            canvas.getContext('2d').fillStyle = 'red';
            canvas.getContext('2d').fillRect(0, 0, 1, 1);
            const pending = canvas.convertToBlob({ type: 'image/webp' });
            canvas.width = 0;
            pending.then(blob => {
                document.querySelector('output').textContent = [
                    blob instanceof Blob, blob.type === 'image/webp', blob.size > 0,
                    canvas.width === 0
                ].join(',');
            });
        </script></body>"#,
        "true,true,true,true",
    );
}

#[test]
fn create_image_bitmap_decodes_blob_and_copies_image_data() {
    run_canvas_case(
        r#"<body><output>waiting</output><canvas width="1" height="1"></canvas><script>
            const canvas = document.querySelector('canvas');
            canvas.getContext('2d').fillStyle = 'red';
            canvas.getContext('2d').fillRect(0, 0, 1, 1);
            const image = new ImageData(1, 1);
            image.data.set([0, 0, 255, 255]);
            const direct = createImageBitmap(image);
            image.data.fill(0);
            canvas.toBlob(blob => {
                Promise.all([createImageBitmap(blob), direct]).then(([decoded, copied]) => {
                    const target = new OffscreenCanvas(2, 1).getContext('2d');
                    target.drawImage(decoded, 0, 0);
                    target.drawImage(copied, 1, 0);
                    document.querySelector('output').textContent = [
                        [...target.getImageData(0, 0, 1, 1).data].join(',') === '255,0,0,255',
                        [...target.getImageData(1, 0, 1, 1).data].join(',') === '0,0,255,255'
                    ].join(',');
                });
            });
        </script></body>"#,
        "true,true",
    );
}

#[test]
fn html_canvas_placeholder_tracks_offscreen_bitmap_for_export() {
    run_canvas_case(
        r#"<body><output>waiting</output><canvas width="1" height="1"></canvas><script>
            const placeholder = document.querySelector('canvas');
            const offscreen = placeholder.transferControlToOffscreen();
            const checks = [offscreen instanceof OffscreenCanvas,
                offscreen.width === 1, placeholder.getContext('2d') === null];
            offscreen.getContext('2d').fillStyle = 'red';
            offscreen.getContext('2d').fillRect(0, 0, 1, 1);
            createImageBitmap(placeholder).then(bitmap => {
                const target = new OffscreenCanvas(1, 1).getContext('2d');
                target.drawImage(bitmap, 0, 0);
                checks.push([...target.getImageData(0, 0, 1, 1).data].join(',') === '255,0,0,255',
                    placeholder.toDataURL().startsWith('data:image/png;base64,'));
                document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
            });
        </script></body>"#,
        "yes",
    );
}

#[test]
fn context_modes_are_exclusive_and_resize_resets_same_size_canvas() {
    run_canvas_case(
        r#"<body><output>waiting</output><canvas width="1" height="1"></canvas><script>
            const canvas = document.querySelector('canvas');
            const context = canvas.getContext('2d');
            context.fillStyle = 'red'; context.fillRect(0, 0, 1, 1);
            const checks = [canvas.getContext('bitmaprenderer') === null];
            canvas.width = 1;
            checks.push(context.fillStyle === '#000000',
                context.getImageData(0, 0, 1, 1).data[3] === 0);
            let transferError = '';
            try { canvas.transferControlToOffscreen(); } catch (error) { transferError = error.name; }
            checks.push(transferError === 'InvalidStateError');
            const detached = new OffscreenCanvas(1, 1);
            let noContextError = '';
            try { detached.transferToImageBitmap(); } catch (error) { noContextError = error.name; }
            checks.push(noContextError === 'InvalidStateError');
            detached.getContext('bitmaprenderer');
            checks.push(detached.getContext('2d') === null);
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        </script></body>"#,
        "yes",
    );
}

#[test]
fn offscreen_empty_and_invalid_images_reject_with_dom_errors() {
    run_canvas_case(
        r#"<body><output>waiting</output><script>
            const empty = new OffscreenCanvas(0, 1);
            const invalid = new Blob(['not an image'], { type: 'image/png' });
            Promise.all([
                empty.convertToBlob().then(() => 'resolved', error => error.name),
                createImageBitmap(invalid).then(() => 'resolved', error => error.name),
                createImageBitmap(new ImageData(1, 1), 0, 0, 0, 1)
                    .then(() => 'resolved', error => error.name)
            ]).then(names => { document.querySelector('output').textContent = names.join(','); });
        </script></body>"#,
        "IndexSizeError,InvalidStateError,IndexSizeError",
    );
}

#[test]
fn structured_clone_copies_bitmaps_but_transfers_offscreen_canvas() {
    run_canvas_case(
        r#"<body><output>waiting</output><script>
            const canvas = new OffscreenCanvas(1, 1), context = canvas.getContext('2d');
            context.fillStyle = 'red'; context.fillRect(0, 0, 1, 1);
            const image = canvas.transferToImageBitmap();
            const copy = structuredClone(image);
            const moved = structuredClone(image, { transfer: [image] });
            context.fillStyle = 'blue'; context.fillRect(0, 0, 1, 1);
            let cloneError = '';
            try { structuredClone(canvas); } catch (error) { cloneError = error.name; }
            const destination = structuredClone(canvas, { transfer: [canvas] });
            const target = new OffscreenCanvas(2, 1).getContext('2d');
            target.drawImage(copy, 0, 0); target.drawImage(moved, 1, 0);
            let detachError = '';
            try { canvas.getContext('2d'); } catch (error) { detachError = error.name; }
            const checks = [image.width === 0, copy.width === 1, moved.width === 1,
                canvas.width === 0, destination.width === 1,
                cloneError === 'DataCloneError', detachError === 'InvalidStateError',
                [...target.getImageData(0, 0, 2, 1).data].join(',') ===
                    '255,0,0,255,255,0,0,255',
                [...destination.getContext('2d').getImageData(0, 0, 1, 1).data].join(',') ===
                    '0,0,255,255'];
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        </script></body>"#,
        "yes",
    );
}

#[test]
fn worker_receives_canvas_and_image_bitmap_pixels_from_window() {
    let (dom, outcome) = execute_html(
        r#"<body><output>waiting</output><script>
            const worker = new Worker('/bitmap-worker.js');
            const canvas = new OffscreenCanvas(1, 1), context = canvas.getContext('2d');
            context.fillStyle = 'blue'; context.fillRect(0, 0, 1, 1);
            const image = structuredClone(canvas.transferToImageBitmap());
            context.fillStyle = 'red'; context.fillRect(0, 0, 1, 1);
            worker.postMessage({canvas, image}, [canvas, image]);
            document.querySelector('output').textContent = [canvas.width, image.width].join(',');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "0,0"
    );
    let Some(ScriptWorkerAction::PostMessage { serialized, .. }) = outcome.worker_actions.get(1)
    else {
        panic!(
            "expected serialized bitmap transfer: {:?}",
            outcome.worker_actions
        )
    };
    let (runtime, started) = WorkerRuntime::start(
        "https://example.com/bitmap-worker.js",
        r#"onmessage = event => {
            const {canvas, image} = event.data;
            const red = [...canvas.getContext('2d').getImageData(0, 0, 1, 1).data];
            const target = new OffscreenCanvas(1, 1).getContext('2d');
            target.drawImage(image, 0, 0);
            const blue = [...target.getImageData(0, 0, 1, 1).data];
            postMessage({red, blue, context: canvas.getContext('2d') instanceof OffscreenCanvasRenderingContext2D});
        };"#,
        "bitmap",
        ScriptKind::Classic,
        Arc::new(|_, _| Err("unexpected import".into())),
    );
    assert!(started.errors.is_empty(), "{:?}", started.errors);
    let received = runtime.unwrap().dispatch_message(serialized);
    assert!(received.errors.is_empty(), "{:?}", received.errors);
    assert_eq!(received.messages.len(), 1, "{:?}", received);
    assert!(
        received.messages[0].contains("255"),
        "{:?}",
        received.messages
    );
    assert!(
        received.messages[0].contains("\"context\",true"),
        "{:?}",
        received.messages
    );
}
