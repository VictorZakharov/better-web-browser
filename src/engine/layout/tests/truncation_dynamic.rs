//! Live-update contracts for CSS text truncation: class toggles, count edits,
//! width edits and text replacement flow through style refresh into layout
//! without stale markers or missing restored text. Uses the real page refresh
//! pipeline (`refresh_resources_after_invalidation`), not fresh parses.

use super::*;
use crate::engine::invalidation::{InvalidationImpact, MutationKind, RenderInvalidation};
use crate::engine::layout::test_support::FixedMeasurer;

const LINE: f32 = 20.0;

fn refresh(
    page: &mut Page,
    root: &NodeRef,
    impact: InvalidationImpact,
) -> crate::engine::css::StyleRefreshStats {
    page.refresh_resources_after_invalidation(
        800.0,
        &RenderInvalidation {
            roots: vec![root.id()],
            impact,
            mutation_count: 1,
            rebuild_style_rules: false,
            removed_nodes: Vec::new(),
            removals_are_local: false,
        },
    )
}

fn painted_text(output: &LayoutOutput) -> Vec<String> {
    output
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn box_height(page: &Page, output: &LayoutOutput) -> f32 {
    let boxed = page
        .dom
        .elements_named("div")
        .find(|node| node.attr("id").as_deref() == Some("box"))
        .unwrap();
    output.node_bounds[&boxed.id()].height
}

#[test]
fn dynamic_updates_recompute_truncation_without_stale_markers() {
    let body = "The quick brown fox jumps over the lazy dog near the riverbank at dusk while birds return home to rest. ".repeat(2);
    let mut page = Page::parse(
        &format!(
            "<style>body{{margin:0}}#box{{font:16px Arial;line-height:20px;width:320px;overflow:hidden;}}\
             .clamped{{display:-webkit-box;-webkit-box-orient:vertical;-webkit-line-clamp:2;}}\
             .clamped3{{display:-webkit-box;-webkit-box-orient:vertical;-webkit-line-clamp:3;}}</style>\
             <div id=\"box\" class=\"clamped\">{body}</div>"
        ),
        "https://example.test/",
    );
    page.refresh_resources(800.0);
    let boxed = page
        .dom
        .elements_named("div")
        .find(|node| node.attr("id").as_deref() == Some("box"))
        .unwrap();
    let mut measurer = FixedMeasurer;

    let output = layout_page(&page, 800.0, 600.0, &mut measurer);
    assert_eq!(box_height(&page, &output), 2.0 * LINE);
    assert!(painted_text(&output).iter().any(|text| text.ends_with('…')));

    // Removing the clamp restores every line.
    boxed.set_attr("class", "");
    let stats = refresh(&mut page, &boxed, MutationKind::Attribute("class").impact());
    assert!(stats.layout_changed, "dropping the clamp must relayout");
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);
    assert!(box_height(&page, &output) > 2.0 * LINE);
    assert!(!painted_text(&output).iter().any(|text| text.ends_with('…')));

    // Changing the count re-clamps to the new budget.
    boxed.set_attr("class", "clamped3");
    let stats = refresh(&mut page, &boxed, MutationKind::Attribute("class").impact());
    assert!(stats.layout_changed, "changing the count must relayout");
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);
    assert_eq!(box_height(&page, &output), 3.0 * LINE);
    assert!(painted_text(&output).iter().any(|text| text.ends_with('…')));

    // Narrowing the box keeps the clamp while reflowing inside it.
    boxed.set_attr("style", "width:160px");
    let stats = refresh(&mut page, &boxed, MutationKind::Attribute("style").impact());
    assert!(stats.layout_changed, "width edits must relayout");
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);
    assert_eq!(box_height(&page, &output), 3.0 * LINE);
    assert!(painted_text(&output).iter().any(|text| text.ends_with('…')));

    // Replacing the text with a short string drops the marker.
    let text_node = boxed.children.borrow()[0].clone();
    Node::set_text_content(&text_node, "short");
    refresh(&mut page, &boxed, MutationKind::CharacterData.impact());
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);
    assert_eq!(box_height(&page, &output), LINE);
    assert!(!painted_text(&output).iter().any(|text| text.ends_with('…')));
    assert_eq!(painted_text(&output).join(""), "short");
}

#[test]
fn dynamic_overflow_flip_repaints_without_relayout() {
    let mut page = Page::parse(
        "<style>body{margin:0}#box{font:16px Arial;line-height:20px;width:320px;\
         white-space:nowrap;overflow:hidden;text-overflow:ellipsis;}</style>\
         <div id=\"box\">The quick brown fox jumps over the lazy dog near the riverbank at dusk.</div>",
        "https://example.test/",
    );
    page.refresh_resources(800.0);
    let boxed = page
        .dom
        .elements_named("div")
        .find(|node| node.attr("id").as_deref() == Some("box"))
        .unwrap();
    let mut measurer = FixedMeasurer;

    let output = layout_page(&page, 800.0, 600.0, &mut measurer);
    assert!(painted_text(&output).iter().any(|text| text.ends_with('…')));

    // Swapping the marker kind is paint-only, yet the rerun must drop it.
    boxed.set_attr("style", "text-overflow:clip");
    let stats = refresh(&mut page, &boxed, MutationKind::Attribute("style").impact());
    assert!(
        !stats.layout_changed,
        "swapping the overflow marker alone must stay paint-only"
    );
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);
    assert!(!painted_text(&output).iter().any(|text| text.ends_with('…')));
}
