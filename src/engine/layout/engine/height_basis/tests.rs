use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

const WIDTH: f32 = 800.0;
const HEIGHT: f32 = 600.0;

/// Records measured text so a test can tell which boxes were laid out again.
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

// The aside is stretched to its row, which grows with the article, and passes its height on. The
// float keeps the paragraphs from replaying on their own, so only a replayed section skips them.
const MARKUP: &str = "<style>
    .grid{display:grid;grid-template-columns:1fr 200px}
    .half{height:50%}
    .tall{height:400px}
    .float{float:right;width:40px;height:40px}
</style>
<main class=grid>
  <article id=article><div class=tall></div><p>article words</p></article>
  <aside id=aside>
    <section><div class=float></div><p>sideword one two three four five six seven</p><p>sideword again</p></section>
    <section><p>sideword more text in a second section</p></section>
    <div id=half class=half>halfword</div>
  </aside>
</main>";

fn element(page: &Page, id: &str) -> NodeRef {
    Node::descendants(&page.dom.document)
        .find(|node| node.attr("id").as_deref() == Some(id))
        .unwrap()
}

fn layout(page: &Page, measurer: &mut Recording) -> LayoutOutput {
    layout_page_with_style_viewport(page, WIDTH, HEIGHT, WIDTH, measurer)
}

fn append_paragraph(page: &mut Page) {
    let paragraph = Node::create_element_for(&page.dom.document, "p");
    Node::set_text_content(&paragraph, "an appended paragraph");
    Node::append_child(&element(page, "article"), paragraph);
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
}

fn height(page: &Page, output: &LayoutOutput, id: &str) -> f32 {
    output.node_bounds[&element(page, id).id()].height
}

#[test]
fn blocks_below_a_growing_stretched_item_replay_with_percentage_children_resized() {
    let mut page = Page::parse(MARKUP, "https://example.test/");
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    let first = layout(&page, &mut Recording::default());
    layout(&page, &mut Recording::default());
    for round in 0..2 {
        append_paragraph(&mut page);
        let mut measurer = Recording::default();
        let warm = layout(&page, &mut measurer);
        assert_eq!(
            measurer.measured("sideword"),
            0,
            "round {round}: {:?}",
            measurer.texts
        );
        assert!(height(&page, &warm, "aside") > height(&page, &first, "aside"));
        assert_eq!(
            height(&page, &warm, "half") * 2.0,
            height(&page, &warm, "aside")
        );

        let retained = std::mem::take(&mut *page.layout_reuse.borrow_mut());
        let cold = layout(&page, &mut Recording::default());
        *page.layout_reuse.borrow_mut() = retained;
        assert_eq!(warm.node_bounds, cold.node_bounds);
        assert_eq!(warm.items.len(), cold.items.len());
    }
}

#[test]
fn tables_flex_boxes_and_flow_roots_below_a_stretched_item_replay() {
    let markup = MARKUP.replace(
        "<div id=half class=half>halfword</div>",
        "<table><tr><td>tableword one</td><td>tableword two</td></tr></table>\
         <div style=display:flex><span>flexword one</span><span>flexword two</span></div>\
         <div style=display:flow-root><p>rootword in its own context</p></div>\
         <div id=transformed style=transform:scale(1)><div>\
         <span id=pinned style=position:absolute;bottom:0>pinned</span></div></div>",
    );
    let mut page = Page::parse(&markup, "https://example.test/");
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    layout(&page, &mut Recording::default());
    layout(&page, &mut Recording::default());
    append_paragraph(&mut page);
    let mut measurer = Recording::default();
    let warm = layout(&page, &mut measurer);
    for word in ["tableword", "flexword", "rootword"] {
        assert_eq!(measurer.measured(word), 0, "{word}: {:?}", measurer.texts);
    }
    // A transformed box is the containing block of absolutely positioned descendants at any
    // depth, which are laid out against its height before it is known: the basis.
    assert_eq!(
        warm.node_bounds[&element(&page, "pinned").id()].bottom(),
        warm.node_bounds[&element(&page, "transformed").id()].y + height(&page, &warm, "aside")
    );
    let retained = std::mem::take(&mut *page.layout_reuse.borrow_mut());
    let cold = layout(&page, &mut Recording::default());
    *page.layout_reuse.borrow_mut() = retained;
    assert_eq!(warm.node_bounds, cold.node_bounds);
    assert_eq!(warm.items.len(), cold.items.len());
}
