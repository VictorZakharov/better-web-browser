use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

const WIDTH: f32 = 800.0;
const HEIGHT: f32 = 600.0;

/// Records measured text so a test can tell which boxes were measured again.
#[derive(Default)]
struct Recording {
    texts: Vec<String>,
}

impl Recording {
    fn measured(&self, word: &str) -> usize {
        self.texts.iter().filter(|text| text.contains(word)).count()
    }
}

impl TextMeasurer for Recording {
    fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
        self.texts.push(text.to_owned());
        FixedMeasurer.measure(text, font)
    }

    fn layout_cache_epoch(&self) -> Option<u64> {
        Some(0)
    }
}

// Auto and flexible grid tracks are sized from every item's intrinsic widths on each pass.
const MARKUP: &str = "<style>
    .grid{display:grid;grid-template-columns:auto 1fr auto}
</style>
<div class=grid>
  <nav id=menu>menuword links</nav>
  <main id=article>article text</main>
  <aside id=tools>toolword entries</aside>
</div>";

fn page() -> Page {
    let mut page = Page::parse(MARKUP, "https://example.test/");
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    page
}

fn layout(page: &Page, measurer: &mut Recording) -> LayoutOutput {
    layout_page_with_style_viewport(page, WIDTH, HEIGHT, WIDTH, measurer)
}

fn element(page: &Page, id: &str) -> NodeRef {
    Node::descendants(&page.dom.document)
        .find(|node| node.attr("id").as_deref() == Some(id))
        .unwrap()
}

fn set_text(page: &mut Page, id: &str, text: &str) {
    Node::set_text_content(&element(page, id), text);
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
}

/// Lays `page` out with what the caches hold, then again from empty caches.
fn assert_matches_cold(page: &Page, warm: &LayoutOutput) {
    let retained = std::mem::take(&mut *page.layout_reuse.borrow_mut());
    let cold = layout(page, &mut Recording::default());
    *page.layout_reuse.borrow_mut() = retained;
    assert_eq!(warm.node_bounds, cold.node_bounds);
    assert_eq!(warm.content_height, cold.content_height);
    assert_eq!(warm.items.len(), cold.items.len());
}

/// Lays `page` out after `change`, with the widths of earlier passes or without them.
fn relayout(change: impl Fn(&mut Page), keep_widths: bool) -> (Recording, LayoutOutput, Page) {
    let mut page = page();
    layout(&page, &mut Recording::default());
    change(&mut page);
    if !keep_widths {
        page.layout_reuse.borrow_mut().widths.clear();
    }
    let mut measurer = Recording::default();
    let output = layout(&page, &mut measurer);
    (measurer, output, page)
}

#[test]
fn unchanged_grid_items_keep_their_widths_across_passes() {
    let change = |page: &mut Page| set_text(page, "article", "article text changed");
    let (kept, warm, page) = relayout(change, true);
    let (cleared, _, _) = relayout(change, false);
    for word in ["menuword", "toolword"] {
        assert!(
            kept.measured(word) < cleared.measured(word),
            "{word}: {} vs {}",
            kept.measured(word),
            cleared.measured(word)
        );
    }
    assert_eq!(kept.measured("changed"), cleared.measured("changed"));
    assert_matches_cold(&page, &warm);
}

#[test]
fn a_changed_grid_item_measures_its_widths_again() {
    let change = |page: &mut Page| set_text(page, "menu", "menuword renamed");
    let (kept, warm, page) = relayout(change, true);
    let (cleared, _, _) = relayout(change, false);
    assert_eq!(kept.measured("renamed"), cleared.measured("renamed"));
    assert!(kept.measured("toolword") < cleared.measured("toolword"));
    assert_matches_cold(&page, &warm);
}
