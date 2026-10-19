use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

const WIDTH: f32 = 800.0;
const HEIGHT: f32 = 600.0;

/// Counts measurements so a test can tell recomputed probes from reused ones.
#[derive(Default)]
struct Counting {
    calls: usize,
}

impl TextMeasurer for Counting {
    fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
        self.calls += 1;
        FixedMeasurer.measure(text, font)
    }

    fn layout_cache_epoch(&self) -> Option<u64> {
        Some(0)
    }
}

// Flex columns, table cells and inline-blocks all size themselves through probes.
const MARKUP: &str = "<style>
    .column{display:flex;flex-direction:column;width:300px}
    .wide{width:200px}
    .cell{padding:2px}
    .chip{display:inline-block;width:120px}
</style>
<main>
  <section class=column><p id=first>alpha beta gamma delta</p><p>epsilon zeta</p></section>
  <table><tr><td class=cell id=cell>cell words that wrap a little</td><td>b</td></tr></table>
  <p><span class=chip id=chip>chip text <img id=icon src=icon.png></span> trailing</p>
</main>";

fn page() -> Page {
    let mut page = Page::parse(MARKUP, "https://example.test/");
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    page
}

fn layout(page: &Page, measurer: &mut Counting) -> LayoutOutput {
    layout_page_with_style_viewport(page, WIDTH, HEIGHT, WIDTH, measurer)
}

/// Lays out `page` with whatever the cache holds, then again from an empty cache.
fn warm_and_cold(page: &Page) -> (LayoutOutput, LayoutOutput) {
    let warm = layout(page, &mut Counting::default());
    let retained = std::mem::take(&mut *page.layout_reuse.borrow_mut());
    let cold = layout(page, &mut Counting::default());
    *page.layout_reuse.borrow_mut() = retained;
    (warm, cold)
}

/// Replayed subtrees are translated by float offsets, so compare to a hundredth of a pixel.
fn assert_same_geometry(warm: &LayoutOutput, cold: &LayoutOutput) {
    let close = |a: f32, b: f32| (a - b).abs() < 0.01;
    assert_eq!(warm.node_bounds.len(), cold.node_bounds.len());
    for (id, rect) in &warm.node_bounds {
        let other = cold.node_bounds[id];
        assert!(
            close(rect.x, other.x)
                && close(rect.y, other.y)
                && close(rect.width, other.width)
                && close(rect.height, other.height),
            "{id:?}: {rect:?} vs {other:?}"
        );
    }
    assert!(close(warm.content_height, cold.content_height));
    assert_eq!(warm.items.len(), cold.items.len());
}

fn element(page: &Page, id: &str) -> NodeRef {
    Node::descendants(&page.dom.document)
        .find(|node| node.attr("id").as_deref() == Some(id))
        .unwrap()
}

#[test]
fn an_unchanged_page_reuses_probes_with_identical_geometry() {
    let page = page();
    let mut first = Counting::default();
    let cold = layout(&page, &mut first);
    assert!(
        page.layout_reuse.borrow().len().0 > 0,
        "the page exercises probes"
    );
    let mut second = Counting::default();
    let warm = layout(&page, &mut second);
    assert!(
        second.calls < first.calls,
        "reused probes skip their measurements ({} vs {})",
        second.calls,
        first.calls
    );
    assert_same_geometry(&warm, &cold);
}

#[test]
fn text_style_and_image_changes_inside_probed_boxes_match_a_cold_layout() {
    let mut page = page();
    layout(&page, &mut Counting::default());

    Node::set_text_content(
        &element(&page, "first"),
        "a much longer replacement sentence that wraps onto more lines",
    );
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    let (warm, cold) = warm_and_cold(&page);
    assert_same_geometry(&warm, &cold);

    element(&page, "cell").set_attr("class", "cell wide");
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    let (warm, cold) = warm_and_cold(&page);
    assert_same_geometry(&warm, &cold);

    // An image decode changes no DOM or style, only the recorded natural size.
    let url = page.image_url(&element(&page, "icon")).unwrap();
    page.images.insert(
        url,
        crate::engine::DecodedImage {
            width: 40,
            height: 90,
            bgra: vec![0; 40 * 90 * 4].into(),
        },
    );
    let (warm, cold) = warm_and_cold(&page);
    assert_same_geometry(&warm, &cold);
    let icon = element(&page, "icon").id();
    assert!(
        warm.node_bounds[&icon].height >= 90.0,
        "the decoded image resizes its box"
    );
}

#[test]
fn environment_changes_discard_every_result() {
    let page = page();
    layout(&page, &mut Counting::default());
    let narrow =
        layout_page_with_style_viewport(&page, 500.0, HEIGHT, WIDTH, &mut Counting::default());
    let retained = std::mem::take(&mut *page.layout_reuse.borrow_mut());
    let cold =
        layout_page_with_style_viewport(&page, 500.0, HEIGHT, WIDTH, &mut Counting::default());
    drop(retained);
    assert_same_geometry(&narrow, &cold);

    struct Epoch(u64);
    impl TextMeasurer for Epoch {
        fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
            let (width, height) = FixedMeasurer.measure(text, font);
            (width * self.0 as f32, height)
        }
        fn layout_cache_epoch(&self) -> Option<u64> {
            Some(self.0)
        }
    }
    let one = layout_page_with_style_viewport(&page, WIDTH, HEIGHT, WIDTH, &mut Epoch(1));
    let two = layout_page_with_style_viewport(&page, WIDTH, HEIGHT, WIDTH, &mut Epoch(2));
    std::mem::take(&mut *page.layout_reuse.borrow_mut());
    let cold = layout_page_with_style_viewport(&page, WIDTH, HEIGHT, WIDTH, &mut Epoch(2));
    assert_same_geometry(&two, &cold);
    assert_ne!(one.node_bounds, two.node_bounds);
}

#[test]
fn measurers_without_an_epoch_and_shadow_trees_are_never_cached() {
    let page = page();
    layout_page_with_style_viewport(
        &page,
        WIDTH,
        HEIGHT,
        WIDTH,
        &mut crate::engine::layout::test_support::CountingMeasurer::default(),
    );
    assert_eq!(page.layout_reuse.borrow().len().0, 0);

    let mut page = Page::parse(
        "<div id=host><template shadowrootmode=open><style>section{display:flex;flex-direction:column}</style><section><p>shadow</p><slot></slot></section></template><p id=light>light</p></div>",
        "https://example.test/",
    );
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    layout(&page, &mut Counting::default());
    // Slotted light content changes without touching the shadow tree's versions.
    Node::set_text_content(
        &element(&page, "light"),
        "a much longer slotted replacement that wraps onto several lines of text",
    );
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    let (warm, cold) = warm_and_cold(&page);
    assert_same_geometry(&warm, &cold);
}
