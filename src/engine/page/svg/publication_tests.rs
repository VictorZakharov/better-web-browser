//! Pending SVG rasters survive paint budgeting until their publication is acknowledged.
use super::*;

fn page() -> (Page, NodeRef, String) {
    let mut page = Page::parse_scripted(
        "<svg width=10 height=10><rect width=10 height=10 fill=red /></svg>",
        "https://example.test/",
    );
    page.refresh_resources(800.0);
    let svg = page.dom.elements_named("svg").next().unwrap();
    let key = inline_svg_key(&svg);
    (page, svg, key)
}

#[test]
fn same_key_paint_change_remains_pending_across_unchanged_refreshes_until_acknowledged() {
    let (mut page, svg, key) = page();
    page.acknowledge_image_updates(std::slice::from_ref(&key));
    assert!(!page.has_image_update(&key));
    let red = page.images[&key].bgra.clone();
    svg.children.borrow()[0].set_attr("fill", "blue");
    page.refresh_resources(800.0);
    assert_ne!(page.images[&key].bgra, red);
    assert!(page.has_image_update(&key));
    page.refresh_resources(800.0);
    assert!(
        page.has_image_update(&key),
        "refresh must not acknowledge discarded paint"
    );
    page.acknowledge_image_updates(std::slice::from_ref(&key));
    page.refresh_resources(800.0);
    assert!(
        !page.has_image_update(&key),
        "unchanged SVG does not requeue pixels"
    );
}

#[test]
fn removal_retires_pending_pixels_and_reattachment_admits_a_fresh_raster() {
    let (mut page, svg, key) = page();
    assert!(Page::is_dynamic_image_key(&key));
    let parent = svg.parent().unwrap();
    assert!(Node::remove_child(&parent, &svg));
    page.refresh_resources(800.0);
    assert!(!page.images.contains_key(&key));
    assert!(!page.has_image_update(&key));
    assert!(Node::append_child(&parent, svg));
    page.refresh_resources(800.0);
    assert!(page.images.contains_key(&key));
    assert!(page.has_image_update(&key));
}

#[test]
fn rejecting_mutated_svg_retires_old_pixels_instead_of_leaving_stale_content() {
    let (mut page, svg, key) = page();
    let text = Node::create_element_for(&svg, "text");
    Node::set_text_content(&text, &"x".repeat(16_385));
    assert!(Node::append_child(&svg, text));
    page.refresh_resources(800.0);
    assert!(!page.images.contains_key(&key));
    assert!(!page.has_image_update(&key));
}

#[test]
fn aggregate_image_budget_rejection_does_not_silently_keep_the_previous_svg() {
    let (mut page, svg, key) = page();
    let block: std::sync::Arc<[u8]> = vec![0; 1024 * 1024].into();
    // Share the fixture allocation; the decoder correctly budgets retained
    // resource bytes, not allocation identity, across distinct resource keys.
    for index in 0..crate::limits::MAX_PAGE_DECODED_IMAGE_BYTES / block.len() {
        page.images.insert(
            format!("test-image:{index}"),
            DecodedImage {
                width: 1024,
                height: 256,
                bgra: block.clone(),
            },
        );
    }
    svg.children.borrow()[0].set_attr("fill", "blue");
    page.refresh_resources(800.0);
    assert!(!page.images.contains_key(&key));
    assert!(!page.has_image_update(&key));
    assert!(
        page.diagnostics
            .iter()
            .any(|message| message.contains("document limit"))
    );
}
