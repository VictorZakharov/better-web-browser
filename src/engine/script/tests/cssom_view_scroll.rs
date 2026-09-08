use super::*;
use std::cell::Cell;

mod pointer_coordinates;
mod properties;
mod timing;

fn initialize(
    html: &str,
    geometry: impl FnOnce(&dom::Dom) -> HashMap<NodeId, RectF>,
) -> (dom::Dom, ScriptRuntime) {
    let dom = dom::parse_with_scripting(html, true);
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    runtime.set_layout_geometry(&geometry(&dom));
    let script = dom.elements_named("script").next().unwrap();
    let outcome = runtime.execute_initial(&[ScriptInput {
        source_url: "https://example.com/#viewport-fixture".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    (dom, runtime)
}

fn evaluate(dom: &dom::Dom, runtime: &mut ScriptRuntime, code: &str) {
    let script = dom.elements_named("script").next().unwrap();
    let outcome = runtime.execute_additional_with_loader(
        &[ScriptInput {
            source_url: "https://example.com/#read-fixture-result".into(),
            code: code.into(),
            node: script,
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: false,
        }],
        None,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

fn scroll(runtime: &mut ScriptRuntime, x: f32, y: f32) {
    let result = runtime.dispatch_user_input(UserInputEvent::Scroll { x, y });
    assert!(
        result.outcome.errors.is_empty(),
        "{:?}",
        result.outcome.errors
    );
    // Embedders deliver the dedicated observer task after native input's microtask checkpoint.
    let notified = runtime.notify_layout_changed();
    assert!(notified.errors.is_empty(), "{:?}", notified.errors);
}

fn result(dom: &dom::Dom) -> String {
    dom.elements_named("body")
        .next()
        .unwrap()
        .attr("data-result")
        .unwrap()
}

fn box_at(x: f32, y: f32) -> RectF {
    RectF {
        x,
        y,
        width: 50.0,
        height: 30.0,
    }
}

#[test]
fn native_scroll_moves_client_rects_without_changing_offset_geometry_or_old_snapshots() {
    let (dom, mut runtime) = initialize(
        r#"<!doctype html><body>
        <main style='position:relative'><span id=target></span></main><script>
            const target = document.getElementById('target');
            const saved = target.getBoundingClientRect();
            const samples = [];
            const sample = () => {
                const r = target.getBoundingClientRect();
                samples.push([r.x,r.y,r.right,r.bottom,r.width,r.height,
                    target.offsetLeft,target.offsetTop]);
            };
            sample();
            addEventListener('scroll', sample);
        </script></body>"#,
        |dom| {
            HashMap::from([
                (
                    dom.elements_named("main").next().unwrap().id(),
                    box_at(50.0, 40.0),
                ),
                (
                    dom.elements_named("span").next().unwrap().id(),
                    box_at(90.0, 110.0),
                ),
            ])
        },
    );
    scroll(&mut runtime, 25.0, 75.0);
    evaluate(
        &dom,
        &mut runtime,
        "document.body.dataset.result = JSON.stringify([samples,saved.x,saved.y]);",
    );
    assert_eq!(
        result(&dom),
        "[[[90,110,140,140,50,30,40,70],[65,35,115,65,50,30,40,70]],90,110]"
    );
}

#[test]
fn native_scroll_distinguishes_viewport_fixed_subtrees_from_fixed_containing_blocks() {
    let (dom, mut runtime) = initialize(
        r#"<!doctype html><body>
        <span id=ordinary></span>
        <span id=fixed style='position:fixed'></span>
        <div style='position:fixed'><span id=descendant></span></div>
        <div style='transform:translateX(10px)'><span id=trapped style='position:fixed'></span></div>
        <div style='position:fixed'><div style='transform:translateX(10px)'>
            <span id=nested style='position:fixed'></span></div></div>
        <div style='display:contents;transform:translateX(10px)'>
            <span id=boxless style='position:fixed'></span></div>
        <script>const ids = ['ordinary','fixed','descendant','trapped','nested','boxless'];</script>
        </body>"#,
        |dom| {
            dom.elements_named("span")
                .map(|node| (node.id(), box_at(90.0, 110.0)))
                .collect()
        },
    );
    scroll(&mut runtime, 25.0, 75.0);
    evaluate(
        &dom,
        &mut runtime,
        "document.body.dataset.result = ids.map(id => { const r = document.getElementById(id).getBoundingClientRect(); return r.x+':'+r.y; }).join(',');",
    );
    assert_eq!(result(&dom), "65:35,90:110,90:110,65:35,90:110,90:110");
}

#[test]
fn scrolled_client_rects_distinguish_missing_boxes_from_positioned_zero_size_boxes() {
    let (dom, mut runtime) = initialize(
        r#"<!doctype html><body>
        <span id=hidden style='display:none'></span><span id=detached></span>
        <span id=empty style='position:absolute;width:0;height:0'></span><script>
            const detached = document.getElementById('detached');
            detached.remove();
            const targets = [document.getElementById('hidden'), detached,
                document.createElement('div'), document.getElementById('empty')];
        </script></body>"#,
        |dom| {
            HashMap::from([
                (
                    dom.elements_named("span").nth(1).unwrap().id(),
                    box_at(90.0, 110.0),
                ),
                (
                    dom.elements_named("span").nth(2).unwrap().id(),
                    RectF {
                        x: 90.0,
                        y: 110.0,
                        width: 0.0,
                        height: 0.0,
                    },
                ),
            ])
        },
    );
    // Retain the old snapshot deliberately: disconnected nodes must not expose stale boxes.
    scroll(&mut runtime, 25.0, 75.0);
    evaluate(
        &dom,
        &mut runtime,
        "document.body.dataset.result = targets.map(target => { const r = target.getBoundingClientRect(); return [r.x,r.y,r.top,r.left,r.right,r.bottom,r.width,r.height].join(':'); }).join(',');",
    );
    assert_eq!(
        result(&dom),
        "0:0:0:0:0:0:0:0,0:0:0:0:0:0:0:0,0:0:0:0:0:0:0:0,65:35:35:65:65:35:0:0"
    );
}

#[test]
fn zero_scroll_client_rect_reads_do_not_build_computed_position_styles() {
    let (dom, mut runtime) = initialize(
        r#"<!doctype html><body>
        <span id=target style='position:fixed'></span><script>
            const target = document.getElementById('target');
            for (let i=0; i<100; i++) target.getBoundingClientRect();
            // Public property overrides must not change the native viewport used for geometry.
            window.scrollX = window.scrollY = 999;
        </script></body>"#,
        |dom| {
            HashMap::from([(
                dom.elements_named("span").next().unwrap().id(),
                box_at(90.0, 110.0),
            )])
        },
    );
    assert!(runtime.host.borrow().offset_parent_styles.is_none());
    evaluate(
        &dom,
        &mut runtime,
        "const r = target.getBoundingClientRect(); document.body.dataset.result = r.x+':'+r.y;",
    );
    assert_eq!(result(&dom), "90:110");
    assert!(runtime.host.borrow().offset_parent_styles.is_none());
}

#[test]
fn native_scroll_observations_ignore_author_event_cancellation_without_relayout_or_duplicates() {
    let (dom, mut runtime) = initialize(
        r#"<!doctype html><body><main id=root>
        <span id=target></span></main><script>
            const target = document.getElementById('target');
            const observations = [];
            let explicitRootEntries = 0, documentScrolls = 0, windowScrolls = 0;
            new IntersectionObserver(entries => {
                for(const entry of entries) observations.push([
                    entry.isIntersecting, entry.intersectionRatio, entry.boundingClientRect.top]);
            }, { threshold: [0,0.5,1] }).observe(target);
            new IntersectionObserver(entries => explicitRootEntries += entries.length,
                { root: document.getElementById('root') }).observe(target);
            document.addEventListener('scroll', event => {
                documentScrolls++;
                event.stopPropagation();
                event.stopImmediatePropagation();
            });
            window.addEventListener('scroll', () => windowScrolls++);
            document.dispatchEvent(new Event('scroll', {bubbles:true}));
        </script></body>"#,
        |dom| {
            HashMap::from([
                (
                    dom.elements_named("main").next().unwrap().id(),
                    RectF {
                        x: 0.0,
                        y: 990.0,
                        width: 200.0,
                        height: 300.0,
                    },
                ),
                (
                    dom.elements_named("span").next().unwrap().id(),
                    RectF {
                        x: 10.0,
                        y: 1000.0,
                        width: 100.0,
                        height: 100.0,
                    },
                ),
            ])
        },
    );
    let flushed = Rc::new(Cell::new(0));
    let observed_flushes = Rc::clone(&flushed);
    runtime.set_layout_flush_callback(Box::new(move |_, _| {
        observed_flushes.set(observed_flushes.get() + 1);
        None
    }));
    let notified = runtime.notify_layout_changed();
    assert!(notified.errors.is_empty(), "{:?}", notified.errors);
    for y in [330.0, 340.0, 400.0, 0.0] {
        scroll(&mut runtime, 0.0, y);
    }
    let unchanged = runtime.notify_layout_changed();
    assert!(unchanged.errors.is_empty(), "{:?}", unchanged.errors);
    assert_eq!(
        flushed.get(),
        0,
        "scroll-only observations must reuse the published geometry"
    );
    evaluate(
        &dom,
        &mut runtime,
        "document.body.dataset.result = JSON.stringify([observations,explicitRootEntries,documentScrolls,windowScrolls]);",
    );
    assert_eq!(
        result(&dom),
        "[[[false,0,1000],[true,0.5,670],[true,1,600],[false,0,1000]],1,5,0]"
    );
}
