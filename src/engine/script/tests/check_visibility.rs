use super::*;
use crate::engine::{FontSpec, Page, TextMeasurer, layout_geometry_with_style_viewport};

struct Measurer;
impl TextMeasurer for Measurer {
    fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
        (text.chars().count() as f32 * font.size / 2.0, font.size)
    }
}

fn run(markup: &str) -> (dom::Dom, ScriptOutcome) {
    let page = Rc::new(RefCell::new(Page::parse_scripted(
        markup,
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
    let script = dom.elements_named("script").next().unwrap();
    let outcome = runtime.execute_initial(&[ScriptInput {
        source_url: "https://example.test/#visibility".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    (dom, outcome)
}

#[test]
fn check_visibility_uses_boxes_and_opt_in_style_checks() {
    let (dom, outcome) = run(r#"<style>
            #none { display: none }
            #contents { display: contents }
            #hidden { visibility: hidden }
            #opaque { opacity: 0 }
            #skipped { content-visibility: hidden; width: 40px; height: 30px }
        </style>
        <div id=none></div><div id=contents><span id=child>child</span></div>
        <div id=hidden></div><div id=opaque><span id=transparent-child>x</span></div>
        <div id=skipped><span id=skipped-child>x</span></div>
        <span style='content-visibility:hidden'><b id=inline-child>inline</b></span>
        <output></output><script>
            const byId = id => document.getElementById(id);
            document.querySelector('output').textContent = [
                !byId('none').checkVisibility(),
                !byId('contents').checkVisibility(),
                byId('child').checkVisibility(),
                byId('hidden').checkVisibility(),
                !byId('hidden').checkVisibility({visibilityProperty:true}),
                byId('opaque').checkVisibility(),
                !byId('transparent-child').checkVisibility({opacityProperty:true}),
                byId('skipped').checkVisibility(),
                !byId('skipped-child').checkVisibility(),
                byId('inline-child').checkVisibility(),
                getComputedStyle(byId('skipped')).contentVisibility === 'hidden',
                getComputedStyle(byId('hidden')).visibility === 'hidden',
                CSS.supports('content-visibility', 'hidden'),
                !CSS.supports('content-visibility', 'auto')
            ].join(',');
        </script>"#);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true,true,true,true,true,true,true,true,true,true,true,true,true,true"
    );
}

#[test]
fn revealing_skipped_content_rebuilds_its_child_boxes() {
    let (dom, outcome) = run(
        r#"<style>section{content-visibility:hidden;width:80px;height:40px}</style>
        <section><span id=child>child</span></section><output></output><script>
            const section = document.querySelector('section');
            const child = document.querySelector('#child');
            const before = [section.checkVisibility(), child.checkVisibility()];
            section.style.contentVisibility = 'visible';
            const after = [section.checkVisibility(), child.checkVisibility()];
            document.querySelector('output').textContent = JSON.stringify([before, after]);
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "[[true,false],[true,true]]"
    );
}
