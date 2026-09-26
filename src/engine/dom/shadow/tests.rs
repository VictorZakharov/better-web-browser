use super::*;

#[test]
fn named_slots_define_composed_children_without_changing_light_parentage() {
    let host = Node::create_element("div");
    let named = Node::create_element_for(&host, "span");
    named.set_attr("slot", "title");
    let default = Node::create_text_for(&host, "body");
    Node::append_child(&host, named.clone());
    Node::append_child(&host, default.clone());
    let root = Node::attach_shadow(&host, ShadowRootMode::Open, false, false, false).unwrap();
    let title_slot = Node::create_element_for(&host, "slot");
    title_slot.set_attr("name", "title");
    let default_slot = Node::create_element_for(&host, "slot");
    Node::append_child(&root, title_slot.clone());
    Node::append_child(&root, default_slot.clone());

    assert_eq!(Node::assigned_nodes(&title_slot, false)[0].id(), named.id());
    assert_eq!(
        Node::assigned_nodes(&default_slot, false)[0].id(),
        default.id()
    );
    assert_eq!(named.parent().unwrap().id(), host.id());
    assert_eq!(Node::composed_parent(&named).unwrap().id(), title_slot.id());
    assert_eq!(Node::composed_children(&host)[0].id(), title_slot.id());
}

#[test]
fn nested_shadow_roots_share_the_document_only_through_shadow_including_traversal() {
    let document = Node::create_document();
    let outer = Node::create_element_for(&document, "x-outer");
    Node::append_child(&document, outer.clone());
    let outer_root =
        Node::attach_shadow(&outer, ShadowRootMode::Open, false, false, false).unwrap();
    let inner = Node::create_element_for(&document, "x-inner");
    Node::append_child(&outer_root, inner.clone());
    let inner_root =
        Node::attach_shadow(&inner, ShadowRootMode::Closed, false, false, false).unwrap();
    let content = Node::create_element_for(&document, "p");
    Node::append_child(&inner_root, content.clone());

    assert_eq!(Node::tree_root(&content).id(), inner_root.id());
    assert_eq!(Node::shadow_including_root(&content).id(), document.id());
    assert!(!Node::descendants(&document).any(|node| node.id() == content.id()));
    assert!(Node::shadow_including_descendants(&document).any(|node| node.id() == content.id()));
}

#[test]
fn manual_slots_preserve_explicit_order_and_ignore_names() {
    let host = Node::create_element("x-card");
    let first = Node::create_element_for(&host, "b");
    first.set_attr("slot", "right");
    let second = Node::create_text_for(&host, "text");
    Node::append_child(&host, first.clone());
    Node::append_child(&host, second.clone());
    let root =
        Node::attach_shadow_with_assignment(&host, ShadowRootMode::Open, false, false, false, true)
            .unwrap();
    let left = Node::create_element_for(&root, "slot");
    let right = Node::create_element_for(&root, "slot");
    right.set_attr("name", "right");
    Node::append_child(&root, left.clone());
    Node::append_child(&root, right.clone());

    assert!(Node::assigned_nodes(&left, false).is_empty());
    assert!(Node::assigned_nodes(&right, false).is_empty());
    Node::assign_manual_nodes(&left, &[second.clone(), first.clone(), second.clone()]);
    assert_eq!(
        Node::assigned_nodes(&left, false)
            .iter()
            .map(|node| node.id())
            .collect::<Vec<_>>(),
        vec![second.id(), first.id()]
    );
    assert_eq!(Node::assigned_slot(&first).unwrap().id(), left.id());
    Node::assign_manual_nodes(&right, std::slice::from_ref(&first));
    assert_eq!(Node::assigned_nodes(&left, false)[0].id(), second.id());
    assert_eq!(Node::assigned_nodes(&right, false)[0].id(), first.id());
    assert_eq!(Node::assigned_slot(&first).unwrap().id(), right.id());
    Node::assign_manual_nodes(&right, &[]);
    assert!(Node::assigned_nodes(&right, false).is_empty());
    assert!(Node::assigned_slot(&first).is_none());
}

#[test]
fn manual_assignments_wait_for_direct_host_parentage() {
    let host = Node::create_element("x-host");
    let root =
        Node::attach_shadow_with_assignment(&host, ShadowRootMode::Open, false, false, false, true)
            .unwrap();
    let slot = Node::create_element_for(&root, "slot");
    Node::append_child(&root, slot.clone());
    let detached = Node::create_element_for(&host, "span");
    Node::assign_manual_nodes(&slot, std::slice::from_ref(&detached));
    assert!(Node::assigned_nodes(&slot, false).is_empty());
    Node::append_child(&host, detached.clone());
    assert_eq!(Node::assigned_nodes(&slot, false)[0].id(), detached.id());
    let elsewhere = Node::create_element_for(&host, "div");
    Node::append_child(&elsewhere, detached.clone());
    assert!(Node::assigned_nodes(&slot, false).is_empty());
    Node::append_child(&host, detached.clone());
    assert_eq!(Node::assigned_nodes(&slot, false)[0].id(), detached.id());
}

#[test]
fn detached_and_named_slots_ignore_manual_assign_calls() {
    let host = Node::create_element("x-host");
    let light = Node::create_element_for(&host, "span");
    Node::append_child(&host, light.clone());
    let root =
        Node::attach_shadow_with_assignment(&host, ShadowRootMode::Open, false, false, false, true)
            .unwrap();
    let manual = Node::create_element_for(&root, "slot");
    Node::append_child(&root, manual.clone());
    Node::assign_manual_nodes(&manual, std::slice::from_ref(&light));
    assert_eq!(Node::assigned_slot(&light).unwrap().id(), manual.id());

    let detached = Node::create_element_for(&root, "slot");
    assert!(Node::assign_manual_nodes(&detached, std::slice::from_ref(&light)).is_empty());
    assert_eq!(Node::assigned_slot(&light).unwrap().id(), manual.id());

    let named_host = Node::create_element("x-named");
    let named_root =
        Node::attach_shadow(&named_host, ShadowRootMode::Open, false, false, false).unwrap();
    let named = Node::create_element_for(&named_root, "slot");
    Node::append_child(&named_root, named.clone());
    assert!(Node::assign_manual_nodes(&named, std::slice::from_ref(&light)).is_empty());
    assert_eq!(Node::assigned_slot(&light).unwrap().id(), manual.id());
}
