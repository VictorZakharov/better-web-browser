//! CSSOM reads remain in unzoomed CSS pixels while client rectangles are physical.
use super::*;
use crate::engine::{FontSpec, Page, TextMeasurer, layout_geometry_with_style_viewport};

struct Measurer;
impl TextMeasurer for Measurer {
    fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
        (text.len() as f32 * font.size / 2.0, font.size)
    }
}

fn start(markup: &str, code: &str) -> (dom::Dom, ScriptRuntime) {
    let page = Rc::new(RefCell::new(Page::parse_scripted(
        &format!("<!doctype html><style>body{{margin:0}}</style>{markup}<script>{code}</script>"),
        "https://example.test/",
    )));
    let dom = page.borrow().dom.clone();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    runtime.set_media_environment(crate::engine::MediaEnvironment::new(
        800.0, 600.0, 1.0, false,
    ));
    runtime.set_layout_viewport(800.0, 600.0);
    runtime.set_layout_flush_callback(Box::new(move |invalidation, metrics| {
        let mut page = page.borrow_mut();
        page.refresh_layout_styles_after_invalidation_for_viewport(800.0, 600.0, invalidation);
        let layout = layout_geometry_with_style_viewport(&page, 800.0, 600.0, 800.0, &mut Measurer);
        metrics.resize_boxes = Some(layout.resize_boxes);
        metrics.scroll_boxes = Some(layout.scroll_boxes);
        metrics.sticky_offsets = Some(layout.sticky_offsets);
        metrics.content_height = Some(layout.content_height);
        Some(layout.node_bounds)
    }));
    let node = dom.elements_named("script").next().unwrap();
    let outcome = runtime.execute_initial_before_document_completion(
        &[ScriptInput {
            source_url: "https://example.test/#zoom".into(),
            code: node.text_content(),
            node,
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: false,
        }],
        None,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    (dom, runtime)
}

fn result(dom: &dom::Dom) -> String {
    dom.elements_named("body")
        .next()
        .unwrap()
        .attr("data-result")
        .unwrap()
}

#[test]
fn nested_zoom_matches_chromium_client_offset_and_rectangle_units() {
    // Independent Chrome 154 headless fixture: [210,216,42,46,17,17,"parent",23,23].
    let (dom, _) = start(
        r#"
        <div id=parent style="zoom:2;position:relative;width:200px;height:120px;border:3px solid;padding:5px">
          <div id=child style="zoom:.5;width:40px;height:30px;border:2px solid;padding:1px;margin:7px"></div>
        </div>"#,
        r#"
        const p=document.getElementById('parent'), c=document.getElementById('child');
        const r=c.getBoundingClientRect();
        document.body.dataset.result=JSON.stringify([
            p.clientWidth,p.offsetWidth,c.clientWidth,c.offsetWidth,c.offsetLeft,c.offsetTop,
            c.offsetParent.id,r.x,r.y,p.currentCSSZoom,c.currentCSSZoom,
            getComputedStyle(p).width,getComputedStyle(c).width]);
    "#,
    );
    assert_eq!(
        result(&dom),
        "[210,216,42,46,17,17,\"parent\",23,23,2,1,\"200px\",\"40px\"]"
    );
}

#[test]
fn zoom_boundary_is_an_offset_parent_even_without_positioning() {
    let (dom, _) = start(
        r#"
        <div id=outer style="zoom:2"><div id=parent style="padding:5px">
          <div id=child style="zoom:.5;width:40px;height:30px"></div>
        </div></div><div id=hidden style="display:none;zoom:3"></div>"#,
        r#"
        const c=document.getElementById('child');
        document.body.dataset.result=JSON.stringify([
            c.offsetParent.id,c.offsetLeft,c.currentCSSZoom,
            document.getElementById('hidden').currentCSSZoom,
            document.createElement('div').currentCSSZoom]);
    "#,
    );
    assert_eq!(result(&dom), "[\"parent\",10,1,1,1]");
}

#[test]
fn zoomed_scroll_into_view_and_scroll_metrics_use_css_units() {
    let (dom, _) = start(
        r#"
        <div id=pane style="zoom:2;width:100px;height:40px;overflow:auto;scrollbar-width:none">
          <div id=target style="height:20px;margin-top:100px;scroll-margin-top:5px"></div>
        </div>"#,
        r#"
        const p=document.getElementById('pane'), t=document.getElementById('target');
        t.scrollIntoView({block:'start',container:'nearest'});
        const first=[p.scrollTop,p.clientHeight,p.scrollHeight];
        p.scrollTop=25.5;
        document.body.dataset.result=JSON.stringify([first,p.scrollTop,t.getBoundingClientRect().y]);
    "#,
    );
    assert_eq!(result(&dom), "[[80,40,120],25.5,149]");
}

#[test]
fn resize_observer_separates_zoomed_device_pixels_from_css_box_units() {
    let (dom, mut runtime) = start(
        r#"
        <div id=target style="zoom:2;width:100px;height:60px;padding:10px;border:5px solid"></div>"#,
        r#"
        new ResizeObserver(entries => {
            const e=entries[0];
            document.body.dataset.result=JSON.stringify([
                e.contentRect.x,e.contentRect.y,e.contentRect.width,e.contentRect.height,
                e.borderBoxSize[0].inlineSize,e.borderBoxSize[0].blockSize,
                e.devicePixelContentBoxSize[0].inlineSize,e.devicePixelContentBoxSize[0].blockSize]);
        }).observe(document.getElementById('target'),{box:'device-pixel-content-box'});
    "#,
    );
    let outcome = runtime.notify_layout_changed();
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom), "[10,10,100,60,130,90,200,120]");
}
