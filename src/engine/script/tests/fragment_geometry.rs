use super::*;
use crate::engine::{FontSpec, Page, TextMeasurer, layout_geometry_with_style_viewport};

struct Measurer;
impl TextMeasurer for Measurer {
    fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
        (text.chars().count() as f32 * font.size / 2.0, font.size)
    }
}

fn check(markup: &str, code: &str) {
    let page = Rc::new(RefCell::new(Page::parse_scripted(
        &format!(
            "<!doctype html><style>body{{margin:0;font:20px Arial;line-height:24px}}</style>{markup}<script>{code}</script>"
        ),
        "https://example.test/",
    )));
    let dom = page.borrow().dom.clone();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    runtime.set_layout_flush_callback(Box::new(move |invalidation, metrics| {
        let mut page = page.borrow_mut();
        page.refresh_layout_styles_after_invalidation_for_viewport(800.0, 600.0, invalidation);
        let layout = layout_geometry_with_style_viewport(&page, 800.0, 600.0, 800.0, &mut Measurer);
        metrics.fragments = Some(layout.fragments);
        metrics.scroll_boxes = Some(layout.scroll_boxes);
        metrics.sticky_offsets = Some(layout.sticky_offsets);
        Some(layout.node_bounds)
    }));
    let node = dom.elements_named("script").next().unwrap();
    let outcome = runtime.execute_initial(&[ScriptInput {
        source_url: "https://example.test/geometry.js".into(),
        code: node.text_content(),
        node,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("true"),
        "{:?}",
        outcome.console
    );
}

#[test]
fn wrapped_inlines_and_partial_text_ranges_use_actual_line_fragments() {
    check(
        "<div style='width:100px;transform:translate(30px,40px)'><span id=s>alpha beta gamma</span></div>",
        r#"
        const s = document.getElementById('s'), text = s.firstChild;
        const rects = s.getClientRects(), r = document.createRange();
        r.setStart(text,1); r.setEnd(text,4);
        const partial = r.getClientRects();
        const all = document.createRange(); all.selectNodeContents(s);
        const checks = [rects.length === 2, all.getClientRects().length === 2,
            rects[0].x === 30, rects[0].y >= 40, rects[1].y > rects[0].y,
            partial.length === 1, partial[0].x === 40, partial[0].width === 30];
        r.collapse(true);
        checks.push(r.getClientRects().length === 1, r.getClientRects()[0].width === 0);
        document.body.dataset.result = checks.every(Boolean);
        if (!checks.every(Boolean)) throw Error(JSON.stringify({checks,rects:[...rects],partial:[...partial]}));
    "#,
    );
}

#[test]
fn source_offsets_survive_surrogates_collapsed_whitespace_and_mutation() {
    check(
        "<div><span id=s>🌠a🌠</span><span id=w>  ab   cd</span></div>",
        r#"
        const s = document.getElementById('s'), w = document.getElementById('w');
        const r = document.createRange(); r.setStart(s.firstChild,1); r.setEnd(s.firstChild,4);
        const old = r.getBoundingClientRect();
        const checks = [old.width === 30];
        r.setStart(w.firstChild,3); r.setEnd(w.firstChild,4);
        checks.push(r.getBoundingClientRect().width === 10);
        w.firstChild.data = 'xxxx'; r.selectNodeContents(w);
        checks.push(r.getBoundingClientRect().width === 40, old.width === 30);
        w.remove(); checks.push(r.getClientRects().length === 0);
        document.body.dataset.result = checks.every(Boolean);
        if (!checks.every(Boolean)) throw Error(JSON.stringify(checks));
    "#,
    );
}

#[test]
fn atomic_descendants_do_not_set_inline_font_height_and_nested_scroll_moves_ranges() {
    check(
        "<div id=p style='width:200px;height:80px;overflow:auto'><span id=s><i style='display:inline-block;width:10px;height:100px'></i></span><div style='height:400px'>abc</div></div>",
        r#"
        const p = document.getElementById('p'), s = document.getElementById('s');
        const r = document.createRange(); r.selectNodeContents(p.lastChild);
        const before = r.getBoundingClientRect(); p.scrollTop = 40;
        const after = r.getBoundingClientRect();
        const checks = [s.getClientRects().length === 1, s.getClientRects()[0].height === 20,
            before.y - after.y === 40];
        document.body.dataset.result = checks.every(Boolean);
        if (!checks.every(Boolean)) throw Error(JSON.stringify({checks,before,after,span:[...s.getClientRects()]}));
    "#,
    );
}

#[test]
fn element_and_range_expose_static_branded_rectangle_lists() {
    let dom = dom::parse_with_scripting(
        r#"<!doctype html><div id=box></div><script>
        const box = document.getElementById('box');
        const list = box.getClientRects();
        const range = document.createRange();
        range.selectNode(box);
        const selected = range.getClientRects();
        const bounding = range.getBoundingClientRect();
        const checks = [list instanceof DOMRectList, list.length === 1,
            list.item(0) === list[0], list.item(9) === null, list[9] === undefined,
            list[0] instanceof DOMRect, list[0] instanceof DOMRectReadOnly,
            selected instanceof DOMRectList, selected.length === 1,
            bounding instanceof DOMRect, bounding.x === 10, bounding.width === 80];
        list[0].x = 999;
        checks.push(box.getClientRects()[0].x === 10, bounding.x === 10);
        box.getClientRects = () => {throw Error('author override must not run')};
        range.getClientRects = box.getClientRects;
        checks.push(box.getBoundingClientRect().width === 80,
            range.getBoundingClientRect().width === 80);
        document.body.dataset.result = checks.every(Boolean);
        </script>"#,
        true,
    );
    let target = dom.elements_named("div").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    runtime.set_layout_geometry(&HashMap::from([(
        target.id(),
        RectF {
            x: 10.0,
            y: 20.0,
            width: 80.0,
            height: 30.0,
        },
    )]));
    let script = dom.elements_named("script").next().unwrap();
    let outcome = runtime.execute_initial(&[ScriptInput {
        source_url: "https://example.test/geometry.js".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("true")
    );
}

#[test]
fn hidden_boxes_remain_measurable_and_nonbreaking_spaces_keep_source_positions() {
    check(
        "<div><span id=h style='visibility:hidden'>hidden</span><span id=n style='display:none'>none</span><span id=w>a&nbsp;b</span><span id=e></span></div>",
        r#"
        const h = document.getElementById('h'), n = document.getElementById('n');
        const r = document.createRange(); r.selectNodeContents(h);
        const checks = [h.getClientRects().length === 1, h.getClientRects()[0].width === 60,
            r.getBoundingClientRect().width === 60, n.getClientRects().length === 0];
        const w = document.getElementById('w'); r.setStart(w.firstChild,1); r.setEnd(w.firstChild,2);
        checks.push(r.getBoundingClientRect().width === 10,
            document.getElementById('e').getClientRects().length === 1);
        document.body.dataset.result = checks.every(Boolean);
        if (!checks.every(Boolean)) throw Error(JSON.stringify(checks));
    "#,
    );
}

#[test]
fn fragment_unions_have_stable_subpixel_coordinates_after_fractional_leading() {
    check(
        "<div style='font:16px Arial;line-height:19.2px'><div id=before>before</div><div style='display:contents'><div style='height:30px'>one</div><div style='height:30px'>two</div></div><div id=after>after</div></div>",
        r#"
        const before = document.getElementById('before'), after = document.getElementById('after');
        const range = document.createRange(); range.setStartAfter(before); range.setEndBefore(after);
        const measured = range.getBoundingClientRect();
        document.body.dataset.result = measured.height === 60 &&
            measured.height === after.getBoundingClientRect().top-before.getBoundingClientRect().bottom &&
            new DOMRect(.0001,0,.0002,0).x === .0001;
    "#,
    );
}
