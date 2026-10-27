//! A retained subtree records the link its inline content inherits from above it once.
use super::*;
use crate::engine::layout::engine::layout_reads::LayoutRead;

fn inherited_link_reads(page: &Page, id: &str) -> Vec<usize> {
    let node = element(page, id).id();
    page.layout_reuse
        .borrow()
        .subtrees
        .iter()
        .filter(|(key, _)| key.node == node)
        .map(|(_, entry)| {
            entry
                .reads
                .reads()
                .iter()
                .filter(|read| matches!(read, LayoutRead::InheritedLink(..)))
                .count()
        })
        .collect()
}

#[test]
fn links_inherited_from_above_a_subtree_are_one_read_of_its_root() {
    let page = page();
    settle(&page);
    // Both paragraphs of the section under the anchor are inline formatting roots.
    assert_eq!(inherited_link_reads(&page, "linked"), [1]);
}

#[test]
fn links_from_anchors_inside_a_subtree_stay_reads_of_their_content() {
    let mut page = Page::parse(
        "<style>section{display:flow-root}</style><main><section id=anchored>\
         <a id=anchor href=inner.html><div><p>first</p><p>second</p></div></a><p>outside</p>\
         </section></main>",
        "https://example.test/",
    );
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    settle(&page);
    // The paragraphs under the anchor read its link; the one outside it shares the root's read.
    assert_eq!(inherited_link_reads(&page, "anchored"), [3]);
    element(&page, "anchor").set_attr("href", "changed.html");
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    let warm = assert_warm_matches_cold(&page);
    assert!(format!("{:?}", warm.items).contains("https://example.test/changed.html"));
}
