use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

#[test]
fn manual_shadow_slots_render_only_explicitly_assigned_light_children() {
    let mut page = Page::parse(
        r#"<x-card><strong id=shown slot=elsewhere>Selected</strong><span>Unassigned</span></x-card>"#,
        "https://example.com/",
    );
    let host = page.dom.elements_named("x-card").next().unwrap();
    let shown = page.dom.elements_named("strong").next().unwrap();
    let root = Node::attach_shadow_with_assignment(
        &host,
        crate::engine::dom::ShadowRootMode::Open,
        false,
        false,
        false,
        true,
    )
    .unwrap();
    Node::replace_inner_html(
        &root,
        "<div><slot>Fallback</slot><slot name=other>Other fallback</slot></div>",
        true,
    );
    let slots = Node::descendants(&root)
        .filter(|node| node.tag_name() == Some("slot"))
        .collect::<Vec<_>>();
    Node::assign_manual_nodes(&slots[1], &[shown]);
    page.refresh_resources(800.0);
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);
    let text = output
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.contains("Selected"), "{text}");
    assert!(text.contains("Fallback"), "{text}");
    assert!(!text.contains("Unassigned"), "{text}");
    assert!(!text.contains("Other fallback"), "{text}");

    Node::assign_manual_nodes(&slots[0], &[]);
    Node::assign_manual_nodes(&slots[1], &[]);
    page.refresh_resources(800.0);
    let fallback = layout_page(&page, 800.0, 600.0, &mut measurer);
    let text = fallback
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(!normalized.contains("Selected"), "{text}");
    assert!(normalized.contains("Fallback"), "{text}");
    assert!(normalized.contains("Other fallback"), "{text}");
}
