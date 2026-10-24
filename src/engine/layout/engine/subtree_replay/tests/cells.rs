//! The cells of a changed table replay unless they changed themselves, however few items each.
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
fn single_item_cells_of_a_changed_table_replay() {
    let rows = (0..40)
        .map(|row| format!("<tr><td id=c{row}>c{row}</td><td>d{row}</td></tr>"))
        .collect::<String>();
    let mut page = Page::parse(
        &format!("<style>.on{{color:red}}</style><table>{rows}</table>"),
        "https://example.test/",
    );
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    settle(&page);
    assert!(
        retained(&page, "c1"),
        "a cell painting one text run is retained"
    );
    // A colour change keeps every cell's size; the other cells replay and stay retained.
    element(&page, "c0").set_attr("class", "on");
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    assert_warm_matches_cold(&page);
    assert!(retained(&page, "c1"));
    // The changed cell is laid out again and, having replayed before, retained from that layout.
    assert!(retained(&page, "c0"));
    let mut replayed = Counting::default();
    layout(&page, &mut replayed);
    assert_eq!(replayed.calls, 0, "every cell replays in the next pass");
}
