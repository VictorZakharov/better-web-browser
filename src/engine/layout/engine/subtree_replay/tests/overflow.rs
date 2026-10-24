//! A replayed subtree moved by a change above it keeps its scrollable overflow.
use super::*;

#[test]
fn replayed_subtrees_keep_their_overflow_when_moved() {
    let sections = "<section><p style='height:20px'>s</p></section>".repeat(12);
    for wrapper in ["", "overflow:hidden;height:900px"] {
        let mut page = Page::parse(
            &format!(
                "<style>body{{margin:0}}</style><div id=top style='height:100px'></div>\
                 <div style='{wrapper}'><section id=tall style='height:50px'>\
                 <div style='height:1500px'>tall</div></section>{sections}</div>"
            ),
            "https://example.test/",
        );
        page.refresh_resources_for_viewport(WIDTH, HEIGHT);
        settle(&page);
        for size in [400, 150, 700] {
            element(&page, "top").set_attr("style", &format!("height:{size}px"));
            page.refresh_resources_for_viewport(WIDTH, HEIGHT);
            // Compares content height with a layout from nothing.
            let moved = assert_warm_matches_cold(&page);
            let expected = if wrapper.is_empty() {
                size + 1500
            } else {
                size + 900
            };
            assert_eq!(
                moved.content_height, expected as f32,
                "{wrapper:?} at {size}px"
            );
        }
        assert!(
            !page.layout_reuse.borrow().overflow.is_empty(),
            "{wrapper:?}: moved sections reuse their retained extents"
        );
    }
}
