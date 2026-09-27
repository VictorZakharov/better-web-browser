use super::*;
use crate::engine::dom::ShadowRootMode;

#[test]
fn clone_preflight_counts_shallow_shadow_descendants_and_template_fragments() {
    let document = Node::create_document();
    let host = Node::create_element_for(&document, "x-host");
    Node::append_child(&host, Node::create_text_for(&host, "light"));
    let shadow = Node::attach_shadow(&host, ShadowRootMode::Open, false, false, true).unwrap();
    let template = Node::create_element_for(&shadow, "template");
    let contents = template
        .element()
        .unwrap()
        .template_contents
        .borrow()
        .clone()
        .unwrap();
    Node::append_child(&contents, Node::create_text_for(&contents, "template text"));
    Node::append_child(&shadow, template);

    // Host + cloned shadow root + template + template fragment + template text.
    assert_eq!(clone_size(&host, false), 5);
    assert_eq!(clone_size(&host, true), 6);
    let clone = Node::clone_for(&document, &host, false);
    assert_eq!(clone.shadow_root().unwrap().text_content(), "");
}

#[test]
fn clone_preflight_excludes_nonclonable_roots_and_unvisited_light_descendants() {
    let document = Node::create_document();
    let host = Node::create_element_for(&document, "x-host");
    let light = Node::create_element_for(&host, "span");
    Node::append_child(&host, light);
    let shadow = Node::attach_shadow(&host, ShadowRootMode::Closed, false, false, false).unwrap();
    Node::append_child(&shadow, Node::create_text_for(&shadow, "secret"));

    assert_eq!(clone_size(&host, false), 1);
    assert_eq!(clone_size(&host, true), 2);
}

#[test]
fn registered_subtree_size_includes_closed_nested_shadow_and_template_nodes() {
    let document = Node::create_document();
    let host = Node::create_element_for(&document, "x-host");
    Node::append_child(&host, Node::create_element_for(&document, "span"));
    let shadow = Node::attach_shadow(&host, ShadowRootMode::Closed, false, false, false).unwrap();
    let template = Node::create_element_for(&shadow, "template");
    let contents = template
        .element()
        .unwrap()
        .template_contents
        .borrow()
        .clone()
        .unwrap();
    Node::append_child(&contents, Node::create_text_for(&contents, "template text"));
    Node::append_child(&shadow, template);
    let nested_host = Node::create_element_for(&shadow, "x-nested");
    let nested_shadow =
        Node::attach_shadow(&nested_host, ShadowRootMode::Open, false, false, false).unwrap();
    Node::append_child(
        &nested_shadow,
        Node::create_text_for(&nested_shadow, "nested text"),
    );
    Node::append_child(&shadow, nested_host);

    // Host, light child, two roots, two shadow children, template fragment,
    // and two text children all occupy native node slots.
    assert_eq!(subtree_size(&host), 9);
}
