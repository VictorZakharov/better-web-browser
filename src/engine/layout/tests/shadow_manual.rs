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

#[test]
fn declarative_named_slot_paints_its_slotted_elements_style() {
    let mut page = Page::parse(
        r#"<x-card><span slot=main class=hot>Selected</span>
           <template shadowrootmode=closed>
             <style>slot[name=main]::slotted(span.hot){color:#123456}</style>
             <slot name=main>Fallback</slot>
           </template></x-card>"#,
        "https://example.com/",
    );
    page.refresh_resources(800.0);
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);
    assert!(output.items.iter().any(|item| {
        matches!(item, DisplayItem::Text { text, color, .. }
            if text.contains("Selected") && *color == Color::rgb(0x12, 0x34, 0x56))
    }));
    assert!(!output.items.iter().any(|item| {
        matches!(item, DisplayItem::Text { text, .. } if text.contains("Fallback"))
    }));
}

#[test]
fn declarative_manual_slot_reassignment_updates_painted_content_and_style() {
    let mut page = Page::parse(
        r#"<x-card><span class=hot>Selected</span>
           <template shadowrootmode=closed shadowrootslotassignment=manual>
             <style>slot.active::slotted(.hot){color:#345678}</style>
             <slot class=active>Fallback</slot>
           </template></x-card>"#,
        "https://example.com/",
    );
    let host = page.dom.elements_named("x-card").next().unwrap();
    let light = page.dom.elements_named("span").next().unwrap();
    let root = host.shadow_root().expect("parser attached the closed root");
    let slot = Node::descendants(&root)
        .find(|node| node.tag_name() == Some("slot"))
        .unwrap();
    Node::assign_manual_nodes(&slot, &[light]);
    page.refresh_resources(800.0);
    let mut measurer = FixedMeasurer;
    let selected = layout_page(&page, 800.0, 600.0, &mut measurer);
    assert!(selected.items.iter().any(|item| {
        matches!(item, DisplayItem::Text { text, color, .. }
            if text.contains("Selected") && *color == Color::rgb(0x34, 0x56, 0x78))
    }));

    Node::assign_manual_nodes(&slot, &[]);
    page.refresh_resources(800.0);
    let fallback = layout_page(&page, 800.0, 600.0, &mut measurer);
    assert!(!fallback.items.iter().any(|item| {
        matches!(item, DisplayItem::Text { text, .. } if text.contains("Selected"))
    }));
    assert!(fallback.items.iter().any(|item| {
        matches!(item, DisplayItem::Text { text, .. } if text.contains("Fallback"))
    }));
}

#[test]
fn nested_declarative_exported_part_uses_outer_styles_in_painted_text() {
    let mut page = Page::parse(
        r#"<style>outer-box::part(public) { color: #123456 }</style>
           <outer-box><template shadowrootmode=closed>
             <inner-box exportparts="label: public">
               <template shadowrootmode=closed>
                 <span part=label>ExportedPartPaint</span>
               </template>
             </inner-box>
           </template></outer-box>"#,
        "https://example.com/",
    );
    page.refresh_resources(800.0);
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);
    assert!(output.items.iter().any(|item| {
        matches!(item, DisplayItem::Text { text, color, .. }
            if text.contains("ExportedPartPaint") && *color == Color::rgb(0x12, 0x34, 0x56))
    }));
}
