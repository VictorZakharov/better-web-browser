use super::*;

fn fixture(code: &str, quirks: bool) -> (dom::Dom, ScriptRuntime, ScriptOutcome) {
    let dom = dom::parse_with_scripting(
        &format!("<!doctype html><body><div id=target></div><script>{code}</script>"),
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    runtime.set_quirks_mode(quirks);
    runtime.set_layout_viewport(800.0, 600.0);
    runtime.set_layout_content_height(2000.0);
    runtime.set_layout_geometry(
        &dom.elements_named("html")
            .chain(dom.elements_named("body"))
            .map(|node| {
                (
                    node.id(),
                    RectF {
                        width: 800.0,
                        height: 2000.0,
                        ..RectF::default()
                    },
                )
            })
            .collect(),
    );
    let node = dom.elements_named("script").next().unwrap();
    let outcome = runtime.execute_initial(&[ScriptInput {
        source_url: "https://example.com/#scroll-properties".into(),
        code: node.text_content(),
        node,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    (dom, runtime, outcome)
}

#[test]
fn scroll_properties_are_numeric_during_custom_element_activation_and_follow_native_input() {
    let (dom, mut runtime, _) = fixture(
        r#"
        class WatchView extends HTMLElement {
            connectedCallback() {
                // A missing value aborts an application's reducer before its other modules start.
                const position = document.scrollingElement.scrollTop;
                if (typeof position !== 'number') throw new Error('invalid initial scroll state');
                this.textContent = 'controller started';
            }
        }
        customElements.define('watch-view', WatchView);
        document.body.appendChild(document.createElement('watch-view'));
        const samples = [];
        document.addEventListener('scroll', () => samples.push([
            document.scrollingElement === document.documentElement,
            document.documentElement.scrollTop, document.documentElement.scrollLeft,
            document.body.scrollTop, document.getElementById('target').scrollTop,
            scrollY, pageYOffset, scrollX, pageXOffset
        ]));
    "#,
        false,
    );
    assert_eq!(
        dom.elements_named("watch-view")
            .next()
            .unwrap()
            .text_content(),
        "controller started"
    );
    scroll(&mut runtime, 12.5, 320.25);
    evaluate(
        &dom,
        &mut runtime,
        "document.body.dataset.result=JSON.stringify(samples);",
    );
    assert_eq!(
        result(&dom),
        "[[true,320.25,12.5,0,0,320.25,320.25,12.5,12.5]]"
    );
}

#[test]
fn viewport_setters_coalesce_requests_and_events_without_expando_scroll_state() {
    let (dom, _, outcome) = fixture(
        r#"
        const samples = [];
        document.addEventListener('scroll', event => samples.push([
            document.documentElement.scrollTop, event.isTrusted, event.target === document
        ]));
        document.documentElement.scrollTop = 150.5;
        scrollBy({top: 50, behavior: 'instant'});
        scrollTo({top: 250.25});
        const synchronous = document.documentElement.scrollTop;
        const earlyEvents = samples.length;
        setTimeout(() => document.body.dataset.result = JSON.stringify([
            synchronous, earlyEvents, samples, Object.hasOwn(document.documentElement, 'scrollTop')
        ]), 10);
    "#,
        false,
    );
    assert_eq!(outcome.viewport_scroll_y, Some(250.25));
    assert_eq!(result(&dom), "[250.25,0,[[250.25,true,true]],false]");
}

#[test]
fn viewport_scroll_normalizes_values_and_clamps_to_current_layout() {
    let (dom, _, outcome) = fixture(
        r#"
        const root = document.documentElement, samples = [];
        root.scrollTop = 99999; samples.push(root.scrollTop);
        root.scrollTop = -20; samples.push(root.scrollTop);
        scroll(0, 80); samples.push(root.scrollTop);
        root.scrollTop = Infinity; samples.push(root.scrollTop);
        root.scrollTop = '90.5'; samples.push(root.scrollTop);
        root.scrollTop = NaN; samples.push(root.scrollTop);
        try { scrollTo({behavior: 'invalid'}); } catch(e) { samples.push(e.name); }
        try { root.scrollTop = Symbol(); } catch(e) { samples.push(e.name); }
        document.body.dataset.result = JSON.stringify(samples);
    "#,
        false,
    );
    assert_eq!(
        result(&dom),
        "[1400,0,80,0,90.5,0,\"TypeError\",\"TypeError\"]"
    );
    assert_eq!(outcome.viewport_scroll_y, Some(0.0));
}

#[test]
fn quirks_body_scroll_mapping_is_live_and_root_is_not_a_scroll_alias() {
    let (dom, mut runtime, _) = fixture("", true);
    scroll(&mut runtime, 0.0, 200.0);
    evaluate(
        &dom,
        &mut runtime,
        r#"
        const samples = [document.scrollingElement === document.body,
            document.body.scrollTop, document.documentElement.scrollTop];
        document.documentElement.scrollTop = 30;
        samples.push(scrollY);
        document.body.scrollTop = 40;
        samples.push(scrollY);
        document.documentElement.style.overflow = 'hidden';
        document.body.style.overflow = 'auto';
        samples.push(document.scrollingElement === null, document.body.scrollTop);
        document.body.dataset.result = JSON.stringify(samples);
    "#,
    );
    assert_eq!(result(&dom), "[true,200,0,200,40,true,0]");
}

#[test]
fn scroll_accessors_reject_wrong_receivers_and_read_dictionary_members_once() {
    let (dom, _, outcome) = fixture(
        r#"
        const calls = [], errors = [];
        scrollTo({
            get behavior() { calls.push('behavior'); return 'instant'; },
            get left() { calls.push('left'); return 0; },
            get top() { calls.push('top'); return 75; }
        });
        const element = Object.getOwnPropertyDescriptor(Element.prototype, 'scrollTop');
        const doc = Object.getOwnPropertyDescriptor(Document.prototype, 'scrollingElement');
        for (const call of [() => element.get.call({}), () => element.set.call({}, 1),
            () => doc.get.call({}), () => { document.documentElement.scrollTop = 1n; }]) {
            try { call(); } catch (e) { errors.push(e.name); }
        }
        document.body.dataset.result = JSON.stringify([calls, errors, scrollY]);
    "#,
        false,
    );
    assert_eq!(outcome.viewport_scroll_y, Some(75.0));
    assert_eq!(
        result(&dom),
        "[[\"behavior\",\"left\",\"top\"],[\"TypeError\",\"TypeError\",\"TypeError\",\"TypeError\"],75]"
    );
}

#[test]
fn inactive_documents_and_detached_elements_do_not_scroll_the_viewport() {
    let (dom, mut runtime, _) = fixture("", false);
    scroll(&mut runtime, 0.0, 180.0);
    evaluate(
        &dom,
        &mut runtime,
        r#"
        const other = document.implementation.createHTMLDocument('inactive');
        other.documentElement.scrollTop = 90;
        const detached = document.createElement('div');
        detached.scrollTop = 75;
        document.body.dataset.result = JSON.stringify([
            other.scrollingElement === other.documentElement, other.documentElement.scrollTop,
            detached.scrollTop, scrollY, Object.hasOwn(detached, 'scrollTop')
        ]);
    "#,
    );
    assert_eq!(result(&dom), "[true,0,0,180,false]");
}
