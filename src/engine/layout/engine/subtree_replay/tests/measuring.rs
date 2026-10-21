//! Measuring layouts (block-size probes) replay the geometry painted layouts retained.
use super::*;

// A grid sizes its rows from block-size probes of its items, then lays them out for painting.
// Nested, as in Wikipedia's page layout, the outer grid's probe lays out the inner grid, which
// probes its item again. Every section establishes its own formatting context; one floats.
const GRID: &str = "<style>
    .grid{display:grid}
    section{display:flow-root;margin:6px;padding:4px}
    .side{float:right;width:30%}
</style>
<div class=grid><div><main class=grid><div id=item>
  <section id=first><p id=text>alpha beta gamma delta epsilon zeta eta theta</p></section>
  <section class=side><p>a floating section of words beside the others</p></section>
  <section><ul><li>one</li><li>two</li><li>three</li></ul><p>after the list</p></section>
  <section><p>more words in another section</p><p>and a second paragraph</p></section>
  <section><p>yet another section with words</p><p>a second paragraph here</p></section>
  <section><p>a sixth section of words</p><p>with its own second paragraph</p></section>
  <section><p>the seventh section of words</p><p>ending the item</p></section>
</div></main></div></div>";

#[test]
fn probes_inside_a_grid_replay_the_unchanged_sections() {
    let mut page = Page::parse(GRID, "https://example.test/");
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    settle(&page);
    Node::set_text_content(&element(&page, "text"), "a changed paragraph");
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    let mut warm = Counting::default();
    layout(&page, &mut warm);
    Node::set_text_content(&element(&page, "text"), "changed again, a little longer");
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    let retained = std::mem::take(&mut *page.layout_reuse.borrow_mut());
    let mut cold = Counting::default();
    layout(&page, &mut cold);
    *page.layout_reuse.borrow_mut() = retained;
    // Only the changed section is measured, by the probes and by the painted layout. Without
    // painted geometry, the outer grid's probe measures every section again (248 vs 162 of 721).
    assert!(
        warm.calls * 4 < cold.calls,
        "probes replay unchanged sections ({} vs {} measurements)",
        warm.calls,
        cold.calls
    );
    assert_warm_matches_cold(&page);
}

const ITEM: &str =
    "<div id=item><section><p id=text>alpha beta gamma delta epsilon zeta eta theta iota
  kappa lambda mu nu xi omicron pi rho sigma tau upsilon</p></section></div>";

/// Text measurements in the pass after the item's text changed.
fn measurements_after_a_change(markup: &str) -> usize {
    let mut page = Page::parse(markup, "https://example.test/");
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    settle(&page);
    Node::set_text_content(&element(&page, "text"), "a changed paragraph of words");
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    let mut changed = Counting::default();
    layout(&page, &mut changed);
    changed.calls
}

#[test]
fn nested_grids_measure_a_changed_item_once_per_pass() {
    let style = "<style>.grid{display:grid} section{display:flow-root}</style>";
    let single = measurements_after_a_change(&format!("{style}<main class=grid>{ITEM}</main>"));
    let nested = measurements_after_a_change(&format!(
        "{style}<div class=grid><div><main class=grid>{ITEM}</main></div></div>"
    ));
    // The outer grid's probe lays the inner grid out, which probes and lays out the item again;
    // the second measuring layout of the item reuses the first.
    assert!(single > 0);
    assert!(
        nested <= single,
        "nested grids measured the item {nested} times, a single grid {single}"
    );
}
