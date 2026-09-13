use super::*;
use crate::engine::{FontSpec, Page, TextMeasurer, layout_geometry_with_style_viewport};

struct Measurer;
impl TextMeasurer for Measurer {
    fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
        (text.len() as f32 * font.size / 2.0, font.size)
    }
}

fn start(code: &str) -> (dom::Dom, ScriptRuntime) {
    let page = Rc::new(RefCell::new(Page::parse_scripted(
        &format!(
            r#"<!doctype html><style>
            #target {{ position:relative; width:100px; height:60px; padding:10px; border:5px solid; }}
            </style><body><div id=target></div><script>{code}</script></body>"#
        ),
        "https://example.test/",
    )));
    let dom = page.borrow().dom.clone();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    runtime.set_layout_flush_callback(Box::new(move |invalidation, metrics| {
        let mut page = page.borrow_mut();
        page.refresh_layout_styles_after_invalidation_for_viewport(800.0, 600.0, invalidation);
        let layout = layout_geometry_with_style_viewport(&page, 800.0, 600.0, 800.0, &mut Measurer);
        metrics.content_height = Some(layout.content_height);
        metrics.resize_boxes = Some(layout.resize_boxes);
        Some(layout.node_bounds)
    }));
    let script = dom.elements_named("script").next().unwrap();
    let result = runtime.execute_initial_before_document_completion(
        &[ScriptInput {
            source_url: "https://example.test/#fixture".into(),
            code: script.text_content(),
            node: script,
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: false,
        }],
        None,
    );
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    (dom, runtime)
}

fn notify(runtime: &mut ScriptRuntime) {
    let result = runtime.notify_layout_changed();
    assert!(result.errors.is_empty(), "{:?}", result.errors);
}

fn change_style(dom: &dom::Dom, runtime: &mut ScriptRuntime, value: &str) {
    let outcome = runtime.execute_additional_with_loader(
        &[ScriptInput {
            source_url: "https://example.test/#mutation".into(),
            code: format!("document.getElementById('target').style.cssText = {value:?};"),
            node: Node::create_element_for(&dom.document, "script"),
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: false,
        }],
        None,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

fn result(dom: &dom::Dom) -> String {
    dom.elements_named("body")
        .next()
        .unwrap()
        .attr("data-result")
        .unwrap_or_default()
}

#[test]
fn resize_observer_reports_distinct_layout_boxes_and_padding_origin() {
    let (dom, mut runtime) = start(
        r#"
        const target = document.getElementById('target');
        new ResizeObserver(entries => {
            const e = entries[0];
            document.body.dataset.result = JSON.stringify([
                e.contentRect.x, e.contentRect.y, e.contentRect.width, e.contentRect.height,
                e.borderBoxSize[0].inlineSize, e.borderBoxSize[0].blockSize,
                e.contentBoxSize[0].inlineSize, e.contentBoxSize[0].blockSize]);
        }).observe(target);
    "#,
    );
    notify(&mut runtime);
    assert_eq!(result(&dom), "[10,10,100,60,130,90,100,60]");
}

#[test]
fn resize_observer_ignores_position_changes() {
    let (dom, mut runtime) = start(
        r#"
        let calls = 0;
        new ResizeObserver(() => document.body.dataset.result = ++calls)
            .observe(document.getElementById('target'));
    "#,
    );
    notify(&mut runtime);
    change_style(&dom, &mut runtime, "left:40px");
    notify(&mut runtime);
    assert_eq!(result(&dom), "1");
}

#[test]
fn resize_observer_validates_options_and_exposes_branded_entries() {
    let (dom, mut runtime) = start(
        r#"
        const target = document.getElementById('target');
        let rejected = 0;
        const ro = new ResizeObserver(function(entries, observer) {
            const e = entries[0];
            let readOnly = false;
            try { (() => { 'use strict'; e.target = null; })(); } catch (_) { readOnly = true; }
            document.body.dataset.result = JSON.stringify([
                rejected, this === ro, observer === ro,
                e instanceof ResizeObserverEntry, e.contentBoxSize[0] instanceof ResizeObserverSize,
                Object.isFrozen(e.contentBoxSize), readOnly]);
        });
        for (const action of [() => ro.observe(target, {box:'wrong'}), () => ro.observe(target, 1),
            () => ro.unobserve({}), () => ResizeObserver.prototype.disconnect.call({})]) {
            try { action(); } catch (e) { if (e instanceof TypeError) rejected++; }
        }
        ro.observe(target, {box:'border-box'});
    "#,
    );
    notify(&mut runtime);
    assert_eq!(result(&dom), "[4,true,true,true,true,true,true]");
}

#[test]
fn resize_observer_self_resize_reports_loop_error_without_recursive_delivery() {
    let (dom, mut runtime) = start(
        r#"
        const target = document.getElementById('target');
        let calls = 0, errors = 0;
        const report = () => document.body.dataset.result = calls + ':' + errors;
        window.addEventListener('error', event => {
            if (event.message === 'ResizeObserver loop completed with undelivered notifications.') {
                errors++; event.preventDefault(); report();
            }
        });
        new ResizeObserver(() => { target.style.width = 100 + (++calls) + 'px'; report(); })
            .observe(target);
    "#,
    );
    notify(&mut runtime);
    assert_eq!(result(&dom), "1:1");
    notify(&mut runtime);
    assert_eq!(result(&dom), "2:2");
}

#[test]
fn resize_observer_observed_box_controls_activation_not_entry_contents() {
    let (dom, mut runtime) = start(
        r#"
        const target = document.getElementById('target');
        let content = 0, border = 0;
        const report = () => document.body.dataset.result = content + ':' + border;
        new ResizeObserver(() => { content++; report(); }).observe(target);
        new ResizeObserver(() => { border++; report(); }).observe(target, {box:'border-box'});
    "#,
    );
    notify(&mut runtime);
    change_style(&dom, &mut runtime, "padding:20px");
    notify(&mut runtime);
    assert_eq!(
        result(&dom),
        "1:2",
        "padding alone changes only the border box"
    );
    change_style(&dom, &mut runtime, "padding:25px;width:90px;height:50px");
    notify(&mut runtime);
    assert_eq!(
        result(&dom),
        "2:2",
        "constant border box still allows content notifications"
    );
}

#[test]
fn resize_observer_depth_loop_includes_microtasks_and_new_deeper_observers() {
    let (dom, mut runtime) = start(
        r#"
        const target = document.getElementById('target');
        const child = target.appendChild(document.createElement('div'));
        child.style.cssText = 'width:10px;height:10px';
        let trace = [];
        new ResizeObserver(() => {
            trace.push('parent');
            Promise.resolve().then(() => {
                trace.push('microtask');
                new ResizeObserver(entries => {
                    trace.push('child:' + entries[0].contentRect.width);
                    document.body.dataset.result = trace.join(',');
                }).observe(child);
                child.style.width = '20px';
            });
        }).observe(target);
    "#,
    );
    notify(&mut runtime);
    assert_eq!(result(&dom), "parent,microtask,child:20");
}

#[test]
fn resize_observer_preserves_constructor_order_and_microtask_checkpoints() {
    let (dom, mut runtime) = start(
        r#"
        const target = document.getElementById('target'), trace = [];
        const first = new ResizeObserver(() => {
            trace.push('first');
            Promise.resolve().then(() => trace.push('job'));
        });
        const second = new ResizeObserver(() => {
            trace.push('second'); document.body.dataset.result = trace.join(',');
        });
        second.observe(target);
        first.observe(target);
    "#,
    );
    notify(&mut runtime);
    assert_eq!(result(&dom), "first,job,second");
}

#[test]
fn resize_observer_zero_boxes_and_disconnection_do_not_depend_on_timers() {
    let (dom, mut runtime) = start(
        r#"
        setTimeout = () => { throw new Error('author timer must not schedule observers'); };
        const target = document.getElementById('target');
        const sizes = [];
        const observer = new ResizeObserver(entries => {
            sizes.push(entries[0].contentRect.width);
            document.body.dataset.result = sizes.join(',');
            if (sizes.length === 3) observer.disconnect();
        });
        observer.observe(target);
    "#,
    );
    assert!(runtime.has_pending_resize_observers());
    notify(&mut runtime);
    change_style(&dom, &mut runtime, "display:none");
    notify(&mut runtime);
    change_style(&dom, &mut runtime, "width:110px");
    notify(&mut runtime);
    change_style(&dom, &mut runtime, "width:120px");
    notify(&mut runtime);
    assert_eq!(result(&dom), "100,0,110");
}

#[test]
fn resize_observer_device_pixel_box_tracks_native_scale_and_cancellation() {
    let (dom, mut runtime) = start(
        r#"
        new ResizeObserver(entries => {
            const e = entries[0];
            document.body.dataset.result = [e.contentRect.width,
                e.devicePixelContentBoxSize[0].inlineSize, e.devicePixelContentBoxSize[0].blockSize].join(',');
        }).observe(document.getElementById('target'), {box:'device-pixel-content-box'});
    "#,
    );
    runtime.set_media_environment(crate::engine::MediaEnvironment::new(
        800.0, 600.0, 1.25, false,
    ));
    notify(&mut runtime);
    assert_eq!(result(&dom), "100,125,75");
    runtime.set_media_environment(crate::engine::MediaEnvironment::new(
        800.0, 600.0, 2.0, false,
    ));
    notify(&mut runtime);
    assert_eq!(result(&dom), "100,200,120");
    runtime.cancel_document();
    assert!(!runtime.has_pending_resize_observers());
    assert!(
        runtime
            .notify_layout_changed()
            .errors
            .iter()
            .any(|e| e.contains("cancelled"))
    );
    assert_eq!(
        result(&dom),
        "100,200,120",
        "cancelled realm cannot invoke retained observers"
    );
}

#[test]
fn resize_observer_callback_exception_does_not_drop_other_observers() {
    let (dom, mut runtime) = start(
        r#"
        const target = document.getElementById('target');
        let errors = 0;
        addEventListener('error', event => { errors++; event.preventDefault(); });
        new ResizeObserver(() => { throw new Error('callback failure'); }).observe(target);
        new ResizeObserver(() => { document.body.dataset.result = errors; }).observe(target);
    "#,
    );
    notify(&mut runtime);
    assert_eq!(result(&dom), "1");
}
