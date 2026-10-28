//! Absolutely positioned descendants of retained subtrees.
use super::*;

// The section is static, so its absolute child resolves against the relative ancestor's padding
// box, outside the section's subtree.
const ESCAPING: &str = "<style>
    section{display:flow-root;padding:4px}
    .abs{position:absolute;top:5px;left:7px;width:40px}
</style>
<div style=position:relative><p id=spacer>short</p>
  <section id=root><p>words in the retained section</p><div class=abs>abs</div></section>
</div>";

fn retained(page: &Page, id: &str) -> bool {
    let node = element(page, id).id();
    page.layout_reuse
        .borrow()
        .subtrees
        .keys()
        .any(|key| key.node == node)
}

/// Settles `markup`, then moves the section down by a line.
fn moved(markup: &str) -> Page {
    let mut page = Page::parse(markup, "https://example.test/");
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    settle(&page);
    Node::set_text_content(
        &element(&page, "spacer"),
        "a much longer paragraph that now wraps across several more lines than before it did \
         and keeps going so that the section below it moves down by a line",
    );
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    page
}

#[test]
fn absolute_children_resolving_outside_a_moved_subtree_stay_put() {
    let page = moved(ESCAPING);
    // A copy of the section moved down would move its absolute child with it.
    assert!(!retained(&page, "root"));
    assert_warm_matches_cold(&page);
}

#[test]
fn absolute_children_of_a_positioned_subtree_move_with_it() {
    // The relatively positioned section is the containing block of its absolute child.
    let page = moved(&ESCAPING.replace("padding:4px}", "padding:4px;position:relative}"));
    assert!(retained(&page, "root"));
    assert_warm_matches_cold(&page);
}
