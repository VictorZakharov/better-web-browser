use super::*;
use crate::engine::{FontSpec, Page, TextMeasurer, layout_geometry_with_style_viewport};

struct Measurer;
impl TextMeasurer for Measurer {
    fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
        (text.chars().count() as f32 * font.size / 2.0, font.size)
    }
}

#[test]
fn mathematical_translations_sample_native_geometry_after_box_and_font_changes() {
    let fixture = include_str!("../../../../../tests/canvas/css-math-transforms.js");
    // The plain script helper has no layout callback. This test must exercise
    // real invalidation and native geometry, not zero-valued placeholder boxes.
    let page = Rc::new(RefCell::new(Page::parse_scripted(
        &format!("<!doctype html><div id=results></div><script>{fixture}
        const oracleFailures=runCSSMathTransforms();
        if (oracleFailures.length) throw Error(oracleFailures.join(','));
        if (document.getElementById('results').getAttribute('data-count') !== '364') throw Error('incomplete oracle');
        </script>"),
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
        source_url: "https://example.test/transform-math.js".into(),
        code: node.text_content(),
        node,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div")
            .next()
            .unwrap()
            .attr("data-failed")
            .as_deref(),
        Some("0")
    );
}
