use super::*;
use std::cell::RefCell;

#[test]
fn unchanged_attributes_still_deliver_custom_element_reactions() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
        const changes = [];
        customElements.define('x-observed', class extends HTMLElement {
            static get observedAttributes() { return ['data-state']; }
            attributeChangedCallback(name, oldValue, newValue) { changes.push([oldValue,newValue]); }
        });
        const target = document.createElement('x-observed');
        document.body.appendChild(target);
        target.setAttribute('data-state','ready');
        target.setAttribute('data-state','ready');
        target.setAttributeNS(null,'data-state','ready');
        document.body.dataset.result = JSON.stringify(changes);
    </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some(r#"[[null,"ready"],["ready","ready"],["ready","ready"]]"#)
    );
}

#[test]
fn unchanged_attribute_writes_notify_observers_without_reflushing_geometry() {
    for (css, target_scope) in [("", true), ("[style]{color:red}", false)] {
        assert_unchanged_attribute_scope(css, target_scope);
    }
}

fn assert_unchanged_attribute_scope(css: &str, target_scope: bool) {
    let dom = crate::engine::dom::parse_with_scripting(
        &format!(
            "<style>{css}</style>{}",
            r#"<body class=unchanged><section><div id=target class=a style='width:100px'></div></section><script>
        const target = document.getElementById('target');
        target.getBoundingClientRect();
        const observer = new MutationObserver(() => {});
        observer.observe(target, {attributes:true, attributeOldValue:true});
        for(let i=0; i<10; i++) {
            target.setAttribute('class','a');
            target.setAttributeNS(null,'style','width:100px');
            document.body.setAttribute('class','unchanged');
            target.getBoundingClientRect();
        }
        const records = observer.takeRecords();
        target.setAttribute('style','width:200px');
        target.getBoundingClientRect();
        document.body.dataset.result = records.length + ':' + records.every(r =>
            r.oldValue === (r.attributeName === 'class' ? 'a' : 'width:100px'));
    </script></body>"#
        ),
        true,
    );
    let script = dom.elements_named("script").next().unwrap();
    let roots = Rc::new(RefCell::new(Vec::new()));
    let observed = Rc::clone(&roots);
    let section_id = dom.elements_named("section").next().unwrap().id();
    let target_id = dom.elements_named("div").next().unwrap().id();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    runtime.set_layout_flush_callback(Box::new(move |invalidation, _| {
        // Keep assertions outside the native callback, which cannot unwind.
        observed.borrow_mut().push(invalidation.roots.clone());
        Some(HashMap::new())
    }));
    let outcome = runtime.execute_initial(&[ScriptInput {
        source_url: "https://example.com/inline".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(roots.borrow().len(), 2);
    assert_eq!(
        roots.borrow()[1],
        vec![if target_scope { target_id } else { section_id }],
        "no-op ancestor writes must not widen the real dirty subtree; CSS={css:?}"
    );
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("20:true")
    );
}
