//! Origin-clean is bitmap ownership, never a saved drawing-state attribute.
use super::*;

pub(super) fn check(script: &str) {
    let (dom, outcome) = execute_html(&format!(
        "<canvas width=16 height=12></canvas><output>no</output><script>{script}</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn canvas_origin_clean_color_filters_taint_on_draw_not_assignment() {
    check(
        r#"
        const canvas = document.querySelector('canvas'), ctx = canvas.getContext('2d');
        const denied = callback => {try{callback(); return false;}catch(error){return error.name === 'SecurityError';}};
        const checks = [];
        for (const filter of ['drop-shadow(1px 0 currentColor)', 'drop-shadow(1px 0)']) {
            canvas.width = 16; ctx.save(); ctx.filter = filter;
            checks.push(ctx.getImageData(0, 0, 1, 1).data[3] === 0);
            ctx.fillRect(0, 0, 1, 1); ctx.restore();
            checks.push(denied(() => ctx.getImageData(0, 0, 1, 1)),
                denied(() => canvas.toDataURL()), denied(() => canvas.toBlob(() => {})));
            ctx.clearRect(0, 0, 16, 12); ctx.putImageData(new ImageData(16, 12), 0, 0);
            checks.push(denied(() => ctx.getImageData(0, 0, 1, 1)));
        }
        canvas.width = 16; checks.push(ctx.getImageData(0, 0, 1, 1).data[3] === 0);
        ctx.filter = 'drop-shadow(1px 0 blue)'; ctx.fillRect(0, 0, 1, 1);
        checks.push(ctx.getImageData(1, 0, 1, 1).data[2] === 255);
        document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
    "#,
    );
}

#[test]
fn canvas_origin_clean_tainted_sources_propagate_to_draw_and_pattern_assignment() {
    check(
        r#"
        const source = document.querySelector('canvas'), original = source.getContext('2d');
        original.filter = 'drop-shadow(1px 0 currentColor)'; original.fillRect(0, 0, 2, 2);
        const denied = callback => {try{callback();return false;}catch(error){return error.name === 'SecurityError';}};
        const target = document.createElement('canvas'), ctx = target.getContext('2d');
        const checks = [];
        ctx.drawImage(source, 0, 0); checks.push(denied(() => ctx.getImageData(0, 0, 1, 1)));
        target.width = 16;
        const pattern = ctx.createPattern(source, 'repeat');
        checks.push(ctx.getImageData(0, 0, 1, 1).data[3] === 0);
        // Patterns own a snapshot; resetting their source cannot clear its taint.
        source.width = 16;
        ctx.fillStyle = pattern; checks.push(denied(() => target.toDataURL()));
        target.width = 16; ctx.strokeStyle = pattern; checks.push(denied(() => target.toDataURL()));
        // An author-owned field cannot relabel a native bitmap or pattern.
        source.originClean = true; pattern.originClean = true;
        checks.push(denied(() => target.toDataURL()));
        document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
    "#,
    );
}

#[test]
fn canvas_origin_clean_bitmap_renderer_replaces_instead_of_combining_ownership() {
    check(
        r#"
        const source = document.querySelector('canvas'), ctx = source.getContext('2d');
        ctx.filter = 'drop-shadow(1px 0 currentColor)'; ctx.fillRect(0, 0, 2, 2);
        const target = document.createElement('canvas'), renderer = target.getContext('bitmaprenderer');
        const denied = () => {try{target.toDataURL();return false;}catch(error){return error.name === 'SecurityError';}};
        createImageBitmap(source).then(async bitmap => {
            renderer.transferFromImageBitmap(bitmap);
            const checks = [denied()];
            renderer.transferFromImageBitmap(null);
            checks.push(!denied());
            renderer.transferFromImageBitmap(await createImageBitmap(source));
            checks.push(denied());
            renderer.transferFromImageBitmap(await createImageBitmap(new ImageData(2, 2)));
            checks.push(!denied());
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        });
    "#,
    );
}

#[test]
fn canvas_origin_clean_bitmap_creation_keeps_taint_and_clone_cannot_export_pixels() {
    check(
        r#"
        const source = document.querySelector('canvas'), ctx = source.getContext('2d');
        ctx.filter = 'drop-shadow(1px 0 currentColor)'; ctx.fillRect(0, 0, 2, 2);
        createImageBitmap(source, 0, 0, 2, 2, {resizeWidth:4, resizeHeight:4}).then(async bitmap => {
            const copy = await createImageBitmap(bitmap);
            const checks = [bitmap.width === 4, copy.width === 4];
            for (const image of [bitmap, copy]) {
                for (const options of [{}, {transfer:[image]}]) {
                    try {structuredClone(image, options); checks.push(false);}
                    catch(error) {checks.push(error.name === 'DataCloneError');}
                    checks.push(image.width === 4);
                }
                const target = document.createElement('canvas');
                target.getContext('bitmaprenderer').transferFromImageBitmap(image);
                try {target.toDataURL(); checks.push(false);}catch(error){checks.push(error.name === 'SecurityError');}
                checks.push(image.width === 0);
            }
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        });
    "#,
    );
}

#[test]
fn canvas_origin_clean_offscreen_transfer_moves_taint_and_new_backing_is_clean() {
    check(
        r#"
        const source = new OffscreenCanvas(4, 4), ctx = source.getContext('2d');
        ctx.filter = 'drop-shadow(1px 0 currentColor)'; ctx.fillRect(0, 0, 1, 1);
        source.convertToBlob().then(() => {throw Error('tainted blob');}, error => {
            const checks = [error.name === 'SecurityError'];
            const bitmap = source.transferToImageBitmap();
            checks.push(ctx.getImageData(0, 0, 1, 1).data[3] === 0);
            const target = document.querySelector('canvas');
            target.getContext('2d').drawImage(bitmap, 0, 0);
            try {target.toDataURL(); checks.push(false);}catch(error){checks.push(error.name === 'SecurityError');}
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        });
    "#,
    );
}
