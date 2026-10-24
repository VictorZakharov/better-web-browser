use crate::engine::layout::test_support::FixedMeasurer;
use crate::engine::layout::{LayoutStats, TextMeasurer, layout_page};
use crate::engine::page::Page;

/// A measurer whose results may be retained across passes.
struct Retaining;

impl TextMeasurer for Retaining {
    fn measure(&mut self, text: &str, font: &crate::engine::layout::FontSpec) -> (f32, f32) {
        FixedMeasurer.measure(text, font)
    }

    fn layout_cache_epoch(&self) -> Option<u64> {
        Some(0)
    }
}

fn pass(page: &Page) -> (LayoutStats, usize) {
    page.take_layout_stats();
    let items = layout_page(page, 800.0, 600.0, &mut Retaining).items.len();
    (page.take_layout_stats(), items)
}

#[test]
fn counts_follow_the_work_each_pass_does() {
    let mut page = Page::parse(
        "<style>section{display:flow-root;margin:4px} .wide{width:300px}</style>\
         <main><section id=a><p>alpha beta</p></section><section id=b><p>gamma</p></section>\
         <section id=c><p>delta</p></section></main>",
        "https://example.test/",
    );
    page.refresh_resources_for_viewport(800.0, 600.0);
    let (cold, items) = pass(&page);
    assert_eq!(cold.passes, 1);
    assert_eq!((cold.subtrees_replayed, cold.items_replayed), (0, 0));
    assert!(cold.items_emitted >= items as u64, "{cold:?}");
    let emitted = cold.items_emitted;
    assert!(
        cold.boxes_rebuilt >= 8 && cold.blocks_laid_out >= 7,
        "{cold:?}"
    );

    // A subtree is retained once a second pass lays it out unchanged.
    pass(&page);
    let (idle, items) = pass(&page);
    assert_eq!(
        idle.boxes_rebuilt, 0,
        "an unchanged document rebuilds no boxes"
    );
    assert!(idle.subtrees_replayed >= 1, "{idle:?}");
    assert_eq!(idle.items_emitted, emitted, "{idle:?}");
    assert!(
        idle.items_replayed >= items as u64 && idle.items_replayed < emitted,
        "{idle:?}"
    );
    assert!(
        idle.blocks_laid_out < cold.blocks_laid_out,
        "{idle:?} vs {cold:?}"
    );

    let section = crate::engine::dom::Node::descendants(&page.dom.document)
        .find(|node| node.attr("id").as_deref() == Some("b"))
        .unwrap();
    section.set_attr("class", "wide");
    page.refresh_resources_for_viewport(800.0, 600.0);
    let (changed, _) = pass(&page);
    assert!(
        changed.boxes_rebuilt > 0 && changed.boxes_rebuilt < cold.boxes_rebuilt,
        "{changed:?}"
    );
    assert!(
        changed.subtrees_replayed >= 2,
        "the other sections replay: {changed:?}"
    );
    assert!(
        page.take_layout_stats() == LayoutStats::default(),
        "taking resets the sum"
    );
}
