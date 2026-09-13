use super::*;
use crate::engine::{FontSpec, Page, TextMeasurer, layout_geometry_with_style_viewport};

struct Measurer;
impl TextMeasurer for Measurer {
    fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
        (text.len() as f32 * font.size / 2.0, font.size)
    }
}

fn run(code: &str) -> (dom::Dom, ScriptOutcome) {
    let page = Rc::new(RefCell::new(Page::parse_scripted(
        &format!(
            "<!doctype html><style>body{{margin:0}}#pane{{position:relative;width:200px;height:100px;overflow:auto}}#content{{width:400px;height:500px}}</style>\
         <div id=pane><div id=content></div></div><script>{code}</script>"
        ),
        "https://example.test/",
    )));
    let dom = page.borrow().dom.clone();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
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
    let outcome = runtime.execute_initial(&[ScriptInput {
        source_url: "https://example.test/#scroll".into(),
        code: node.text_content(),
        node,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    (dom, outcome)
}

fn result(dom: &dom::Dom) -> String {
    dom.elements_named("body")
        .next()
        .unwrap()
        .attr("data-result")
        .unwrap()
}

#[test]
fn scroll_geometry_is_synchronous_and_events_are_coalesced_without_bubbling() {
    let (dom, outcome) = run(r#"
        const pane=document.getElementById('pane'), content=document.getElementById('content');
        const events=[]; let bubbled=0;
        document.addEventListener('scroll',()=>bubbled++);
        pane.addEventListener('scroll', e=>events.push([pane.scrollTop,e.isTrusted,e.bubbles]));
        pane.scrollTo({top:80.25,left:20}); pane.scrollBy(5,10);
        const rect=content.getBoundingClientRect();
        const values=[pane.scrollWidth,pane.scrollHeight,pane.scrollLeft,pane.scrollTop,
            content.offsetTop,rect.x,rect.y,window.scrollY,events.length];
        setTimeout(()=>document.body.dataset.result=JSON.stringify([values,events,bubbled]),10);
    "#);
    assert_eq!(
        result(&dom),
        "[[400,500,25,90.25,0,-25,-90.25,0,0],[[90.25,true,false]],0]"
    );
    assert!(outcome.render_requested);
    assert!(outcome.viewport_scroll_y.is_none());
}

#[test]
fn scroll_offsets_clamp_and_detached_nodes_do_not_scroll() {
    let (dom, _) = run(r#"
        const pane=document.getElementById('pane'), samples=[];
        for(const value of [10000,-3,50.5,1e100,Infinity,NaN]) { pane.scrollTop=value; samples.push(pane.scrollTop); }
        const detached=document.createElement('div'); detached.scrollTop=40; samples.push(detached.scrollTop);
        try { pane.scrollTo({behavior:'invalid'}); } catch(e) { samples.push(e.name); }
        document.body.dataset.result=JSON.stringify(samples);
    "#);
    assert_eq!(result(&dom), "[415,0,50.5,415,0,0,0,\"TypeError\"]");
}

#[test]
fn sticky_client_rects_update_synchronously_when_an_element_scrolls() {
    let (dom, _) = run(r#"
        const pane=document.getElementById('pane'), content=document.getElementById('content');
        content.innerHTML='<div id="sticky" style="position:sticky;top:10px;height:20px"></div>';
        const sticky=document.getElementById('sticky'), samples=[];
        for (const value of [0,150,400,150,0]) {
            pane.scrollTop=value;
            samples.push([sticky.getBoundingClientRect().top,sticky.offsetTop,pane.clientWidth,pane.clientHeight]);
        }
        document.body.dataset.result=JSON.stringify(samples);
    "#);
    assert_eq!(
        result(&dom),
        "[[10,0,185,85],[10,0,185,85],[10,0,185,85],[10,0,185,85],[10,0,185,85]]"
    );
}
