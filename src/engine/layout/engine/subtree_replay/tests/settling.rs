//! Subtrees are retained once their DOM and styles hold for a pass, not while they keep changing.
use super::*;

fn retained(page: &Page, id: &str) -> bool {
    let node = element(page, id).id();
    page.layout_reuse
        .borrow()
        .subtrees
        .keys()
        .any(|key| key.node == node)
}

#[test]
fn a_section_that_keeps_changing_is_retained_once_it_holds_for_a_pass() {
    let mut page = page();
    settle(&page);
    assert!(retained(&page, "intro"));
    for text in ["first change", "second change", "third change"] {
        Node::set_text_content(&element(&page, "text"), text);
        page.refresh_resources_for_viewport(WIDTH, HEIGHT);
        assert_warm_matches_cold(&page);
        assert!(
            !retained(&page, "intro"),
            "a section changed since the previous pass is not copied"
        );
        assert!(retained(&page, "list"), "unchanged sections stay retained");
    }
    // The first pass without a change lays the section out again and retains it; the next replays.
    layout(&page, &mut Counting::default());
    assert!(retained(&page, "intro"));
    let mut replayed = Counting::default();
    layout(&page, &mut replayed);
    assert_eq!(
        replayed.calls, 0,
        "every section replays without measuring text"
    );
    assert_warm_matches_cold(&page);
}
