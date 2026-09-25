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

#[test]
fn scroll_into_view_aligns_nested_scroll_box_without_moving_viewport() {
    let (dom, outcome) = run(r#"
        const pane = document.getElementById('pane');
        const content = document.getElementById('content');
        const returned = content.scrollIntoView({block:'end', inline:'end', container:'nearest'});
        document.body.dataset.result = JSON.stringify([
            pane.scrollLeft, pane.scrollTop, scrollY, returned instanceof Promise,
            content.getBoundingClientRect().bottom <= pane.getBoundingClientRect().bottom,
            content.getBoundingClientRect().right <= pane.getBoundingClientRect().right
        ]);
    "#);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom), "[215,415,0,true,true,true]");
}

#[test]
fn scroll_into_view_legacy_boolean_and_viewport_alignment() {
    let (dom, outcome) = run(r#"
        const pane = document.getElementById('pane');
        pane.before(Object.assign(document.createElement('div'), {id:'spacer'}));
        document.getElementById('spacer').style.height = '900px';
        const footer = document.createElement('div');
        footer.style.height = '1600px';
        pane.after(footer);
        const top = pane.getBoundingClientRect().top;
        const height = pane.getBoundingClientRect().height;
        const maxScroll = document.documentElement.scrollHeight - innerHeight;
        pane.scrollIntoView();
        const start = scrollY;
        const afterStartTop = pane.getBoundingClientRect().top;
        pane.scrollIntoView(false);
        const end = scrollY;
        document.body.dataset.result = JSON.stringify([top, height, maxScroll, start, afterStartTop, end, typeof pane.scrollIntoView]);
    "#);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    // The runtime viewport is 720px high, though the layout fixture is 600px.
    assert_eq!(result(&dom), "[900,100,1880,900,0,280,\"function\"]");
}

#[test]
fn scroll_into_view_nearest_does_not_move_an_already_visible_target() {
    let (dom, outcome) = run(r#"
        const pane = document.getElementById('pane');
        const target = document.createElement('div');
        target.style.cssText = 'width:20px;height:20px;margin:30px';
        pane.firstElementChild.prepend(target);
        pane.scrollTo(0, 10);
        const before = [pane.scrollLeft, pane.scrollTop];
        target.scrollIntoView({block:'nearest', inline:'nearest', container:'nearest'});
        document.body.dataset.result = JSON.stringify([before, [pane.scrollLeft, pane.scrollTop]]);
    "#);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom), "[[0,10],[0,10]]");
}

#[test]
fn scroll_into_view_validates_options_without_scrolling_detached_elements() {
    let (dom, outcome) = run(r#"
        const pane = document.getElementById('pane');
        const detached = document.createElement('div');
        const failures = [];
        for (const options of [{block:'middle'}, {inline:'far'},
            {container:'invalid'}, {behavior:'warp'}]) {
            try { pane.scrollIntoView(options); } catch (error) { failures.push(error.name); }
        }
        detached.scrollIntoView();
        document.body.dataset.result = JSON.stringify([failures, pane.scrollTop, scrollY]);
    "#);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        result(&dom),
        "[[\"TypeError\",\"TypeError\",\"TypeError\",\"TypeError\"],0,0]"
    );
}

#[test]
fn scroll_into_view_converts_legacy_boolean_union_and_ignores_overridden_geometry() {
    let (dom, outcome) = run(r#"
        const pane = document.getElementById('pane');
        const target = document.getElementById('content');
        target.getBoundingClientRect = () => { throw new Error('author override'); };
        target.getClientRects = () => { throw new Error('author override'); };
        target.scrollIntoView({block:'end', container:'nearest'});
        const end = pane.scrollTop;
        pane.scrollTop = 0;
        target.scrollIntoView(0);
        const legacyFalse = pane.scrollTop;
        pane.scrollTop = 100;
        target.scrollIntoView(1);
        const legacyTrue = pane.scrollTop;
        pane.scrollTop = 100;
        target.scrollIntoView(null);
        document.body.dataset.result = JSON.stringify([end, legacyFalse, legacyTrue, pane.scrollTop]);
    "#);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom), "[415,415,0,0]");
}

#[test]
fn scroll_into_view_applies_scroll_margin_and_scroll_padding_to_both_axes() {
    let (dom, outcome) = run(r#"
        const pane = document.getElementById('pane');
        const content = document.getElementById('content');
        content.style.position = 'relative';
        content.innerHTML = '<div id="target" style="position:absolute;left:200px;top:200px;width:20px;height:20px;scroll-margin:10px 15px"></div>';
        pane.style.scrollPadding = '6px 8px';
        const target = document.getElementById('target');
        const css = getComputedStyle(target);
        const supported = [CSS.supports('scroll-margin', '10px 15px'), CSS.supports('scroll-padding', '6px 8px')];
        target.scrollIntoView({block:'start', inline:'start', container:'nearest'});
        const targetRect = target.getBoundingClientRect(), paneRect = pane.getBoundingClientRect();
        document.body.dataset.result = JSON.stringify([
            supported, css.scrollMarginTop, getComputedStyle(pane).scrollPaddingLeft,
            pane.scrollLeft, pane.scrollTop,
            targetRect.left - paneRect.left, targetRect.top - paneRect.top
        ]);
    "#);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom), "[[true,true],\"10px\",\"8px\",177,184,23,16]");
}

#[test]
fn root_scroll_padding_adjusts_viewport_scroll_into_view_position() {
    let (dom, outcome) = run(r#"
        document.documentElement.style.scrollPaddingTop = '25px';
        const spacer = document.createElement('div'); spacer.style.height = '900px';
        const target = document.createElement('div'); target.style.cssText = 'height:20px;scroll-margin-top:10px';
        const footer = document.createElement('div'); footer.style.height = '900px';
        document.body.replaceChildren(spacer, target, footer);
        target.scrollIntoView({block:'start'});
        document.body.dataset.result = JSON.stringify([scrollY, target.getBoundingClientRect().top]);
    "#);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom), "[865,35]");
}

#[test]
fn instant_element_scroll_fires_trusted_scroll_then_one_scrollend() {
    let (dom, outcome) = run(r#"
        const pane = document.getElementById('pane');
        const events = [];
        pane.addEventListener('scroll', e => events.push([e.type, e.isTrusted, e.bubbles, pane.scrollTop]));
        pane.addEventListener('scrollend', e => events.push([e.type, e.isTrusted, e.bubbles, pane.scrollTop]));
        document.addEventListener('scrollend', () => events.push(['bubbled']));
        pane.scrollTop = 20; pane.scrollTop = 30; pane.scrollTop = 30;
        setTimeout(() => document.body.dataset.result = JSON.stringify(events), 10);
    "#);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        result(&dom),
        "[[\"scroll\",true,false,30],[\"scrollend\",true,false,30]]"
    );
}

#[test]
fn scroll_listener_restarts_scrolling_before_scrollend_can_fire() {
    let (dom, outcome) = run(r#"
        const pane = document.getElementById('pane');
        const events = [];
        pane.addEventListener('scroll', () => {
            events.push('scroll:' + pane.scrollTop);
            if (pane.scrollTop === 20) pane.scrollTop = 40;
        });
        pane.addEventListener('scrollend', () => events.push('end:' + pane.scrollTop));
        pane.scrollTop = 20;
        setTimeout(() => document.body.dataset.result = events.join(','), 10);
    "#);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom), "scroll:20,scroll:40,end:40");
}

#[test]
fn no_op_scroll_does_not_fire_scroll_or_scrollend() {
    let (dom, outcome) = run(r#"
        const pane = document.getElementById('pane');
        let events = 0;
        pane.addEventListener('scroll', () => events++);
        pane.addEventListener('scrollend', () => events++);
        pane.scrollTop = 0; pane.scrollTo({top:0});
        setTimeout(() => document.body.dataset.result = String(events), 10);
    "#);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom), "0");
}
