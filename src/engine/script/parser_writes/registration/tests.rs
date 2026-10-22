//! A parser step tells the script layout flush what it changed, not just that rules may have.
use super::*;

fn host(html: &str) -> HostState {
    let dom = crate::engine::dom::parse(html);
    let mut state = HostState::new(
        dom.document.clone(),
        "https://example.com/",
        "UTF-8",
        Rc::new(module_loader::WebModuleLoader::new()),
    );
    state.register_parser_changes(None);
    state.pending_layout_invalidation.take(0);
    state
}

fn element(state: &HostState, tag: &str) -> NodeRef {
    Node::descendants(&state.document)
        .find(|node| node.tag_name() == Some(tag))
        .expect(tag)
}

#[test]
fn journaled_insertions_restyle_their_parents_not_the_document() {
    let mut state = host("<main><p>old</p></main>");
    let main = element(&state, "main");
    let inserted = Node::create_element_for(&state.document, "section");
    assert!(Node::append_child(&main, inserted.clone()));
    state.register_parser_changes(Some(&[inserted]));
    let flushed = state.pending_layout_invalidation.take(0);
    assert!(!flushed.rebuild_style_rules);
    assert!(!flushed.roots.contains(&state.document.id()), "{flushed:?}");
    assert_eq!(flushed.child_list_roots, vec![main.id()]);
}

#[test]
fn journaled_style_sheets_and_changed_elements_rebuild_rules_below_their_parents() {
    let mut state = host("<head></head><body><main><p>old</p></main></body>");
    let head = element(&state, "head");
    let style = Node::create_element_for(&state.document, "style");
    assert!(Node::append_child(&head, style.clone()));
    state.register_parser_changes(Some(&[style]));
    let flushed = state.pending_layout_invalidation.take(0);
    assert!(flushed.rebuild_style_rules);
    assert_eq!(flushed.roots, vec![head.id()]);

    // An element the realm already knew had its attributes or shadow root changed.
    let main = element(&state, "main");
    let body = element(&state, "body");
    state.register_parser_changes(Some(&[main]));
    let flushed = state.pending_layout_invalidation.take(0);
    assert!(flushed.rebuild_style_rules);
    assert_eq!(flushed.roots, vec![body.id()]);
}
