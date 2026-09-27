use super::*;
use crate::engine::dom::{self, ShadowRootMode};

#[test]
fn deep_import_allocates_the_target_documents_identity() {
    let source = dom::parse("<section id='source'><span>text</span></section>");
    let target = Node::create_document();
    let section = source.elements_named("section").next().unwrap();

    let clone = Node::clone_for(&target, &section, true);

    assert_eq!(clone.id().document(), target.id().document());
    assert_ne!(clone.id(), section.id());
    assert_eq!(clone.attr("id").as_deref(), Some("source"));
    assert_eq!(clone.text_content(), "text");
}

#[test]
fn shallow_import_copies_the_full_clonable_closed_shadow_tree_not_light_children() {
    let source_document = Node::create_document();
    let host = Node::create_element_for(&source_document, "x-card");
    Node::append_child(&host, Node::create_element_for(&host, "light"));
    let shadow =
        Node::attach_shadow_with_assignment(&host, ShadowRootMode::Closed, true, true, true, true)
            .unwrap();
    let span = Node::create_element_for(&shadow, "span");
    span.set_attr("id", "shadow-child");
    Node::append_child(&span, Node::create_text_for(&span, "shadow text"));
    Node::append_child(&shadow, span.clone());

    let target_document = Node::create_document();
    let copy = Node::clone_for(&target_document, &host, false);
    let copied_shadow = copy.shadow_root().unwrap();
    let NodeData::ShadowRoot(metadata) = &copied_shadow.data else {
        panic!("copied root must remain a ShadowRoot");
    };
    assert!(copy.children.borrow().is_empty());
    assert_ne!(copy.id(), host.id());
    assert_ne!(copied_shadow.id(), shadow.id());
    assert_eq!(copy.id().document(), target_document.id().document());
    assert_eq!(
        copied_shadow.id().document(),
        target_document.id().document()
    );
    assert_eq!(copied_shadow.shadow_host().unwrap().id(), copy.id());
    assert_eq!(metadata.mode, ShadowRootMode::Closed);
    assert!(metadata.delegates_focus);
    assert!(metadata.serializable);
    assert!(metadata.clonable);
    assert!(metadata.manual_slot_assignment);
    assert_eq!(copied_shadow.text_content(), "shadow text");
    let copied_span = copied_shadow.children.borrow()[0].clone();
    copied_span.set_attr("id", "independent");
    assert_eq!(span.attr("id").as_deref(), Some("shadow-child"));
}

#[test]
fn nested_shadow_roots_copy_only_when_each_root_is_clonable() {
    let document = Node::create_document();
    let outer = Node::create_element_for(&document, "x-outer");
    let light_host = Node::create_element_for(&outer, "x-light");
    Node::append_child(&outer, light_host.clone());
    let light_shadow =
        Node::attach_shadow(&light_host, ShadowRootMode::Open, false, false, true).unwrap();
    Node::append_child(
        &light_shadow,
        Node::create_text_for(&light_shadow, "light shadow"),
    );

    let root = Node::attach_shadow(&outer, ShadowRootMode::Open, false, false, true).unwrap();
    let copied_host = Node::create_element_for(&root, "x-copied");
    let skipped_host = Node::create_element_for(&root, "x-skipped");
    Node::append_child(&root, copied_host.clone());
    Node::append_child(&root, skipped_host.clone());
    let nested =
        Node::attach_shadow(&copied_host, ShadowRootMode::Closed, false, false, true).unwrap();
    Node::append_child(&nested, Node::create_text_for(&nested, "nested shadow"));
    let skipped =
        Node::attach_shadow(&skipped_host, ShadowRootMode::Open, false, false, false).unwrap();
    Node::append_child(&skipped, Node::create_text_for(&skipped, "not copied"));

    let shallow = Node::clone_for(&document, &outer, false);
    assert!(shallow.children.borrow().is_empty());
    let shallow_root = shallow.shadow_root().unwrap();
    let shallow_children = shallow_root.children.borrow();
    assert_eq!(shallow_children.len(), 2);
    assert_eq!(
        shallow_children[0].shadow_root().unwrap().text_content(),
        "nested shadow"
    );
    assert!(shallow_children[1].shadow_root().is_none());

    let deep = Node::clone_for(&document, &outer, true);
    let copied_light = deep.children.borrow()[0].clone();
    assert_eq!(
        copied_light.shadow_root().unwrap().text_content(),
        "light shadow"
    );
    assert_eq!(deep.shadow_root().unwrap().children.borrow().len(), 2);
}

#[test]
fn nonclonable_host_root_is_not_copied_on_either_depth() {
    let document = Node::create_document();
    let host = Node::create_element_for(&document, "x-host");
    let root = Node::attach_shadow(&host, ShadowRootMode::Closed, false, true, false).unwrap();
    Node::append_child(&root, Node::create_text_for(&root, "private"));

    assert!(
        Node::clone_for(&document, &host, false)
            .shadow_root()
            .is_none()
    );
    assert!(
        Node::clone_for(&document, &host, true)
            .shadow_root()
            .is_none()
    );
}
