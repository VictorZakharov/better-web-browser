use super::*;
use crate::engine::dom::ShadowRootMode;
use std::rc::Rc;

#[test]
fn native_bounds_index_matches_shadow_template_adopted_and_detached_lookup_ownership() {
    let dom = parse(
        "<main id=parent><template id=template><b id=inert></b></template><div id=host></div></main>",
    );
    let parent = element(&dom, "parent");
    let template = element(&dom, "template");
    let contents = template
        .element()
        .unwrap()
        .template_contents
        .borrow()
        .clone()
        .unwrap();
    let inert = contents.children.borrow()[0].clone();
    let host = element(&dom, "host");
    let shadow = Node::attach_shadow(&host, ShadowRootMode::Closed, false, false, false).unwrap();
    let private = Node::create_element_for(&shadow, "span");
    assert!(Node::append_child(&shadow, private.clone()));
    let foreign = parse("<section id=adopted></section>");
    let adopted = element(&foreign, "adopted");
    assert!(Node::append_child(&parent, adopted.clone()));
    assert_ne!(adopted.id().document(), dom.document.id().document());
    let detached = Node::create_element_for(&parent, "div");
    let pseudo = Node::create_element_for(&parent, "breeze-pseudo-before");
    // Generated layout nodes have an ancestry link but are not DOM children.
    pseudo.parent.set(Some(Rc::downgrade(&parent)));

    let index = CurrentNodes::new(&dom.document);
    for node in [
        &dom.document,
        &contents,
        &inert,
        &shadow,
        &private,
        &adopted,
        &detached,
        &pseudo,
    ] {
        let expected = dom.find_node(node.id());
        let actual = index.nodes.get(&node.id());
        assert_eq!(actual.is_some(), expected.is_some(), "node {:?}", node.id());
        if let (Some(actual), Some(expected)) = (actual, expected) {
            assert!(Rc::ptr_eq(actual, &expected));
        }
    }
    let layout = layout_for(&[(&private, rect(0.0, 0.0, 100.0, 100.0))]);
    assert_eq!(
        node_at_point(&dom, &layout, 10.0, 10.0).unwrap().id(),
        private.id()
    );
}

#[test]
fn native_bounds_index_is_fresh_after_removal_reinsertion_and_document_replacement() {
    let first = parse("<main id=parent><div id=target></div></main>");
    let parent = element(&first, "parent");
    let target = element(&first, "target");
    let layout = layout_for(&[(&target, rect(0.0, 0.0, 100.0, 100.0))]);
    assert_eq!(
        node_at_point(&first, &layout, 10.0, 10.0).unwrap().id(),
        target.id()
    );
    assert!(Node::remove_child(&parent, &target));
    assert!(node_at_point(&first, &layout, 10.0, 10.0).is_none());
    assert!(Node::append_child(&parent, target.clone()));
    assert!(node_at_point(&first, &layout, 10.0, 10.0).is_some());
    let replacement = parse("<main id=replacement></main>");
    assert!(node_at_point(&replacement, &layout, 10.0, 10.0).is_none());
    let destination = element(&replacement, "replacement");
    assert!(Node::append_child(&destination, target.clone()));
    assert!(node_at_point(&first, &layout, 10.0, 10.0).is_none());
    assert_eq!(
        node_at_point(&replacement, &layout, 10.0, 10.0)
            .unwrap()
            .id(),
        target.id()
    );
}

#[test]
fn native_bounds_index_does_not_cache_adopted_template_content_by_document_version() {
    let dom = parse("<main id=parent></main>");
    let foreign = parse("<template><div></div></template>");
    let template = foreign.elements_named("template").next().unwrap();
    let contents = template
        .element()
        .unwrap()
        .template_contents
        .borrow()
        .clone()
        .unwrap();
    let parent = element(&dom, "parent");
    assert!(Node::append_child(&parent, template));
    let initial = CurrentNodes::new(&dom.document).nodes.len();
    let version = dom.document.subtree_mutation_version();
    let added = Node::create_element_for(&contents, "span");
    assert!(Node::append_child(&contents, added.clone()));
    assert_eq!(dom.document.subtree_mutation_version(), version);
    assert_eq!(CurrentNodes::new(&dom.document).nodes.len(), initial + 1);
    assert!(dom.find_node(added.id()).is_some());
    // Merely being searchable in inert contents creates no painted box.
    assert!(node_at_point(&dom, &LayoutOutput::default(), 10.0, 10.0).is_none());
    assert!(Node::append_child(&parent, added.clone()));
    let layout = layout_for(&[(&added, rect(0.0, 0.0, 100.0, 100.0))]);
    assert_eq!(
        node_at_point(&dom, &layout, 10.0, 10.0).unwrap().id(),
        added.id()
    );
}
