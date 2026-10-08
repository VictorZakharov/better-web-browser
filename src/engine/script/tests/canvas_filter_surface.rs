//! Exercise the whole drawing pipeline, not just Gaussian byte admission.
use super::*;

fn check(script: &str) {
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
fn canvas_filter_off_canvas_source_is_blurred_back_inside() {
    check(
        r#"
        const ctx = document.querySelector('canvas').getContext('2d');
        ctx.filter = 'blur(1px)'; ctx.fillStyle = 'red';
        ctx.fillRect(-2, 3, 2, 4);
        const pixel = ctx.getImageData(0, 4, 1, 1).data;
        const far = ctx.getImageData(12, 4, 1, 1).data;
        document.querySelector('output').textContent =
            pixel[0] === 255 && pixel[3] > 0 && far[3] === 0 ? 'yes' : [...pixel].join(',');
    "#,
    );
}

#[test]
fn canvas_filter_offset_shadow_can_return_off_canvas_ink() {
    check(
        r#"
        const ctx = document.querySelector('canvas').getContext('2d');
        ctx.filter = 'drop-shadow(4px 0 0 blue)'; ctx.fillStyle = 'red';
        ctx.fillRect(-3, 3, 2, 2);
        const pixel = ctx.getImageData(1, 3, 1, 1).data;
        document.querySelector('output').textContent =
            [...pixel].join(',') === '0,0,255,255' ? 'yes' : [...pixel].join(',');
    "#,
    );
}

#[test]
fn canvas_filter_halo_preserves_current_path_transform_and_final_clip() {
    check(
        r#"
        const ctx = document.querySelector('canvas').getContext('2d');
        ctx.translate(2, 0); ctx.beginPath(); ctx.rect(0, 2, 2, 2);
        ctx.save(); ctx.beginPath(); ctx.rect(0, 0, 1, 12); ctx.clip();
        ctx.beginPath(); ctx.rect(-3, 2, 2, 2);
        ctx.filter = 'drop-shadow(3px 0 0 blue)'; ctx.fillStyle = 'red'; ctx.fill();
        const drawn = ctx.getImageData(2, 2, 1, 1).data;
        const clipped = ctx.getImageData(3, 2, 1, 1).data;
        const matrix = ctx.getTransform();
        ctx.restore(); ctx.filter = 'none'; ctx.fillStyle = 'lime'; ctx.fill();
        const originalPath = ctx.getImageData(0, 2, 1, 1).data;
        const checks = [drawn[2] === 255, drawn[3] === 255, clipped[3] === 0,
            matrix.e === 2, originalPath[1] === 255, originalPath[3] === 255];
        document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
    "#,
    );
}

#[test]
fn canvas_filter_path2d_and_transformed_rectangle_use_same_halo_space() {
    check(
        r#"
        const canvas = document.querySelector('canvas'), ctx = canvas.getContext('2d');
        ctx.setTransform(1, 0, 0, 1, 2, 1);
        ctx.filter = 'blur(1px)'; ctx.fillStyle = '#3571b3';
        const path = new Path2D(); path.rect(-3, 3, 2, 2); ctx.fill(path);
        const first = [...ctx.getImageData(0, 0, 16, 12).data].join(',');
        ctx.clearRect(-2, -1, 16, 12); ctx.fillRect(-3, 3, 2, 2);
        const second = [...ctx.getImageData(0, 0, 16, 12).data].join(',');
        document.querySelector('output').textContent = first === second &&
            ctx.getTransform().e === 2 ? 'yes' : 'mismatched';
    "#,
    );
}

#[test]
fn canvas_filter_self_draw_reads_original_bitmap_not_temporary_surface() {
    check(
        r#"
        const canvas = document.querySelector('canvas'), ctx = canvas.getContext('2d');
        ctx.fillStyle = 'red'; ctx.fillRect(2, 2, 2, 2);
        ctx.filter = 'drop-shadow(3px 0 0 blue)';
        ctx.drawImage(canvas, 0, 0);
        const source = ctx.getImageData(2, 2, 1, 1).data;
        const shadow = ctx.getImageData(5, 2, 1, 1).data;
        document.querySelector('output').textContent = source[0] === 255 &&
            source[3] === 255 && shadow[2] === 255 && shadow[3] === 255 ? 'yes' : 'mismatched';
    "#,
    );
}

#[test]
fn canvas_filter_resource_decline_is_atomic_and_restores_drawing_state() {
    check(
        r#"
        const ctx = document.querySelector('canvas').getContext('2d');
        ctx.fillStyle = 'red'; ctx.fillRect(1, 1, 2, 2);
        ctx.translate(2, 1); ctx.beginPath(); ctx.rect(2, 2, 2, 2);
        const before = [...ctx.getImageData(0, 0, 16, 12).data].join(',');
        for (const filter of ['blur(65px)', 'drop-shadow(1e9px 0 blue)']) {
            ctx.filter = filter;
            let rejected = false;
            try { ctx.fill(); } catch (error) { rejected = error.name === 'NotSupportedError'; }
            const after = [...ctx.getImageData(0, 0, 16, 12).data].join(',');
            if (!rejected || before !== after || ctx.getTransform().e !== 2) throw Error(filter);
        }
        ctx.filter = 'none'; ctx.fillStyle = 'blue'; ctx.fill();
        const pixel = ctx.getImageData(4, 3, 1, 1).data;
        document.querySelector('output').textContent = pixel[2] === 255 &&
            pixel[3] === 255 ? 'yes' : 'path lost';
    "#,
    );
}

#[test]
fn canvas_filter_css_font_relative_lengths_are_snapshotted_at_assignment() {
    check(
        r#"
        const canvas = document.querySelector('canvas'), ctx = canvas.getContext('2d');
        canvas.style.fontSize = '2px'; canvas.style.color = 'blue';
        ctx.font = '100px sans-serif';
        ctx.filter = 'drop-shadow(1em 0 blue)';
        canvas.style.fontSize = '9px'; canvas.style.color = 'lime';
        ctx.fillStyle = 'red'; ctx.fillRect(0, 0, 1, 1);
        const old = [...ctx.getImageData(2, 0, 1, 1).data].join(',');
        const absent = ctx.getImageData(9, 0, 1, 1).data[3];
        ctx.filter = 'drop-shadow(1em 0 lime)'; ctx.fillRect(0, 3, 1, 1);
        const fresh = [...ctx.getImageData(9, 3, 1, 1).data].join(',');
        document.querySelector('output').textContent = old === '0,0,255,255' &&
            absent === 0 && fresh === '0,255,0,255' ? 'yes' : [old, absent, fresh].join(';');
    "#,
    );
}

#[test]
fn canvas_filter_parser_preserves_original_string_invalid_assignment_and_brand_order() {
    check(
        r#"
        const ctx = document.querySelector('canvas').getContext('2d');
        const text = ' /**/BrIgHtNeSs(calc(2 - 1))/**/ ';
        ctx.filter = text; ctx.save(); ctx.filter = 'none'; ctx.restore();
        const checks = [ctx.filter === text];
        for (const invalid of ['blur(-1px)', 'blur(1%)', 'hue-rotate(1)', 'brightness(1) bogus()']) {
            ctx.filter = invalid; checks.push(ctx.filter === text);
        }
        const setter = Object.getOwnPropertyDescriptor(CanvasRenderingContext2D.prototype, 'filter').set;
        let conversions = 0;
        try { setter.call({}, {toString(){conversions++; return 'none';}}); }
        catch (error) { checks.push(error instanceof TypeError); }
        checks.push(conversions === 0);
        try { ctx.filter = Symbol(); } catch (error) { checks.push(error instanceof TypeError); }
        checks.push(ctx.filter === text);
        document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
    "#,
    );
}
