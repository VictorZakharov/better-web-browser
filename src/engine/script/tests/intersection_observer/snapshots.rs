use super::*;

fn start(code: &str) -> (dom::Dom, ScriptRuntime) {
    let dom = dom::parse_with_scripting(
        &format!("<body><div id=target></div><script>{code}</script></body>"),
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    let script = dom.elements_named("script").next().unwrap();
    let result = runtime.execute_initial(&[ScriptInput {
        source_url: "https://example.test/".into(),
        code: code.into(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    (dom, runtime)
}

fn evaluate(dom: &dom::Dom, runtime: &mut ScriptRuntime, code: &str) {
    let result = runtime.execute_additional_with_loader(
        &[ScriptInput {
            source_url: "https://example.test/later".into(),
            code: code.into(),
            node: dom.elements_named("script").next().unwrap(),
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: false,
        }],
        None,
    );
    assert!(result.errors.is_empty(), "{:?}", result.errors);
}

fn sample(dom: &dom::Dom, runtime: &mut ScriptRuntime, top: f32) {
    let target = dom.elements_named("div").next().unwrap();
    runtime.set_layout_geometry(&HashMap::from([(
        target.id(),
        RectF {
            x: 10.0,
            y: top,
            width: 100.0,
            height: 50.0,
        },
    )]));
    let result = runtime.gather_intersection_observers();
    assert!(result.errors.is_empty(), "{:?}", result.errors);
}

#[test]
fn queued_intersection_snapshots_survive_later_layout_and_disconnect() {
    let (dom, mut runtime) = start(
        r#"
        const target=document.getElementById('target');
        const observer=new IntersectionObserver(entries => console.log(JSON.stringify(
            entries.map(e=>[e.boundingClientRect.top,e.isIntersecting]))));
        observer.observe(target);
        observer.takeRecords=()=>{ throw Error('overridden author method'); };
        target.getBoundingClientRect=()=>{ throw Error('overridden author geometry'); };
    "#,
    );
    sample(&dom, &mut runtime, 10.0);
    assert!(runtime.has_intersection_task());
    sample(&dom, &mut runtime, 1000.0);
    evaluate(&dom, &mut runtime, "observer.disconnect()");
    let result = runtime.notify_intersection_observers();
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(result.console, ["log: [[10,true],[1000,false]]"]);
    assert!(!runtime.has_intersection_task());
    assert!(runtime.notify_intersection_observers().console.is_empty());
}

#[test]
fn take_records_drains_queued_entries_without_callback_or_new_geometry() {
    let (dom, mut runtime) = start(
        r#"
        const observer=new IntersectionObserver(()=>console.log('unexpected callback'));
        observer.observe(document.getElementById('target'));
    "#,
    );
    sample(&dom, &mut runtime, 20.0);
    evaluate(
        &dom,
        &mut runtime,
        r#"
        const entries=observer.takeRecords();
        document.body.dataset.result=JSON.stringify([entries.length,entries[0].boundingClientRect.top,
            entries[0].boundingClientRect instanceof DOMRectReadOnly,observer.takeRecords().length]);
    "#,
    );
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .unwrap(),
        "[1,20,true,0]"
    );
    assert!(runtime.notify_intersection_observers().console.is_empty());
    sample(&dom, &mut runtime, 20.0);
    assert!(
        !runtime.has_intersection_task(),
        "draining must preserve previous thresholds"
    );
}

#[test]
fn callback_microtasks_can_drain_later_observers_and_order_is_construction_order() {
    let (dom, mut runtime) = start(
        r#"
        const target=document.getElementById('target');
        const first=new IntersectionObserver(function(entries,observer) {
            console.log('first:'+(this===observer));
            Promise.resolve().then(()=>console.log('drained:'+second.takeRecords().length));
        });
        const second=new IntersectionObserver(()=>console.log('unexpected second'));
        second.observe(target); first.observe(target);
    "#,
    );
    sample(&dom, &mut runtime, 10.0);
    let result = runtime.notify_intersection_observers();
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(result.console, ["log: first:true", "log: drained:1"]);
}

#[test]
fn observer_options_are_readonly_branded_and_validate_webidl_inputs() {
    let (_, outcome) = execute_html(
        r#"<script>
        const o=new IntersectionObserver(()=>{}, {threshold:'0.5',rootMargin:'1in',scrollMargin:''});
        o.root=null; o.rootMargin='100px'; o.thresholds=[1];
        console.log(o.rootMargin,o.scrollMargin,o.thresholds.join(','));
        for(const action of [()=>o.observe(null),()=>o.unobserve({}),
            ()=>IntersectionObserver.prototype.disconnect.call({}),
            ()=>new IntersectionObserver(()=>{}, {threshold:NaN})]) {
            try {action()} catch(e) {console.log(e.name)}
        }
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        [
            "log: 96px 96px 96px 96px 0px 0px 0px 0px 0.5",
            "log: TypeError",
            "log: TypeError",
            "log: TypeError",
            "log: TypeError"
        ]
    );
}
