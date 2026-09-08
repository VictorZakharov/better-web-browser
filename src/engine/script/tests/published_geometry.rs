//! A renderer checkpoint consumes content layout work, not pending style-cache work.
use super::*;
use std::cell::Cell;

#[test]
fn published_geometry_does_not_replay_old_text_layout_but_new_text_still_invalidates() {
    let dom = dom::parse_with_scripting(
        r#"<body><div id=target>initial text</div>
        <script>
            document.getElementById('target').firstChild.data = 'already rendered text';
        </script>
        <script>
            const target = document.getElementById('target');
            target.setAttribute('aria-label', 'ready');
            document.body.dataset.result = target.getBoundingClientRect().width;
        </script>
        <script>
            target.firstChild.data = 'new text after the renderer checkpoint';
            document.body.dataset.result = target.getBoundingClientRect().width;
        </script>"#,
        true,
    );
    let scripts = dom.elements_named("script").collect::<Vec<_>>();
    let target = dom.elements_named("div").next().unwrap();
    let body = dom.elements_named("body").next().unwrap();
    let target_id = target.id();
    let observed = Rc::new(RefCell::new(Vec::<RenderInvalidation>::new()));
    let callback_observed = Rc::clone(&observed);
    let builds = Rc::new(Cell::new(0));
    let callback_builds = Rc::clone(&builds);
    let geometry = move |width| {
        HashMap::from([(
            target_id,
            RectF {
                width,
                height: 20.0,
                ..RectF::default()
            },
        )])
    };
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    runtime.set_layout_flush_callback(Box::new(move |invalidation, _| {
        callback_observed.borrow_mut().push(invalidation.clone());
        // These text and ARIA mutations leave computed box styles unchanged. Model
        // the renderer's post-style gate: only new intrinsic work needs a layout.
        if !invalidation.impact.affects_intrinsic_size() {
            return None;
        }
        callback_builds.set(callback_builds.get() + 1);
        Some(geometry(300.0))
    }));
    let input = |node: &NodeRef| ScriptInput {
        source_url: "https://example.com/#checkpoint".into(),
        code: node.text_content(),
        node: node.clone(),
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    };
    let initial = runtime.execute_initial(&[input(&scripts[0])]);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(initial.invalidation.impact.affects_intrinsic_size());
    assert!(observed.borrow().is_empty());

    runtime.set_layout_geometry(&geometry(200.0));
    let after_publication = runtime.execute_additional_with_loader(&[input(&scripts[1])], None);
    assert!(
        after_publication.errors.is_empty(),
        "{:?}",
        after_publication.errors
    );
    assert_eq!(body.attr("data-result").as_deref(), Some("200"));
    assert_eq!(builds.get(), 0, "old text changes were already laid out");
    assert_eq!(observed.borrow().len(), 1);
    assert!(observed.borrow()[0].impact.affects_style());
    assert!(!observed.borrow()[0].roots.is_empty());
    assert!(!observed.borrow()[0].impact.affects_intrinsic_size());

    let changed_again = runtime.execute_additional_with_loader(&[input(&scripts[2])], None);
    assert!(
        changed_again.errors.is_empty(),
        "{:?}",
        changed_again.errors
    );
    assert_eq!(body.attr("data-result").as_deref(), Some("300"));
    assert_eq!(builds.get(), 1, "new text must still request layout");
    assert_eq!(observed.borrow().len(), 2);
    assert!(observed.borrow()[1].impact.affects_intrinsic_size());
}
