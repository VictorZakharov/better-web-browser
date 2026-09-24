use super::*;

#[test]
fn canvas_focus_ring_only_paints_for_focused_fallback_content() {
    let (dom, outcome) = execute_html(
        r#"<body><canvas id=surface width=24 height=24><button id=inside>Inside</button></canvas>
        <button id=outside>Outside</button><script>
        const canvas = document.getElementById('surface');
        const inside = document.getElementById('inside');
        const outside = document.getElementById('outside');
        const ctx = canvas.getContext('2d');
        const alpha = (x, y) => ctx.getImageData(x, y, 1, 1).data[3];
        ctx.beginPath(); ctx.rect(4, 4, 12, 12);
        ctx.drawFocusIfNeeded(inside);
        const unfocused = alpha(4, 4) === 0;
        outside.focus(); ctx.drawFocusIfNeeded(outside);
        const unrelated = alpha(4, 4) === 0;
        inside.focus();
        ctx.strokeStyle = '#ff0000'; ctx.globalAlpha = .1;
        ctx.globalCompositeOperation = 'copy';
        ctx.setLineDash([1, 5]);
        ctx.drawFocusIfNeeded(inside);
        const pixel = ctx.getImageData(4, 4, 1, 1).data;
        const painted = pixel[2] > pixel[0] && pixel[3] === 255;
        const unchanged = ctx.strokeStyle === '#ff0000' && ctx.globalAlpha === .1 &&
            ctx.globalCompositeOperation === 'copy' && ctx.getLineDash().join(',') === '1,5';
        document.body.setAttribute('data-result',
            [unfocused, unrelated, painted, unchanged].join(','));
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("true,true,true,true")
    );
}

#[test]
fn canvas_focus_ring_respects_path2d_and_rejects_non_elements() {
    let (dom, outcome) = execute_html(
        r#"<body><canvas width=24 height=24><input id=control></canvas><script>
        const canvas = document.querySelector('canvas');
        const ctx = canvas.getContext('2d');
        const control = document.getElementById('control');
        control.focus();
        const path = new Path2D(); path.rect(3, 3, 8, 8);
        ctx.drawFocusIfNeeded(path, control);
        const painted = ctx.getImageData(3, 3, 1, 1).data[3] > 0;
        let invalid = false;
        try { ctx.drawFocusIfNeeded(path, {}); }
        catch (error) { invalid = error instanceof TypeError; }
        document.body.setAttribute('data-result', painted + ':' + invalid);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("true:true")
    );
}
