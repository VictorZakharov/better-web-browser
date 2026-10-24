use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

mod arrivals;
mod cells;
mod measuring;
mod settling;

const WIDTH: f32 = 800.0;
const HEIGHT: f32 = 600.0;

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

// Each section establishes its own formatting context, so each can be retained and replayed.
const MARKUP: &str = "<style>
    section{display:flow-root;margin:6px;padding:4px;background:#eef}
    .scroller{overflow:auto;height:40px}
    .raised{position:relative;z-index:2}
    .tall{height:120px}
    .dark{background:#222}
</style>
<main>
  <section id=intro><h2>Intro</h2><p id=text>alpha beta gamma delta epsilon zeta eta theta</p></section>
  <section id=list><ul><li>one</li><li>two</li><li>three</li></ul><p>after the list</p></section>
  <section id=media><p>image <img id=icon src=icon.png> inline</p><p>more words here</p></section>
  <section id=scroll class=scroller><p>a</p><p>b</p><p>c</p><p>d</p><p>e</p><p>f</p></section>
  <section id=raised class=raised><p>raised words</p><p>second raised line</p></section>
  <div id=backdrop><section id=form><p>field</p><input placeholder=hint><p>after</p></section></div>
  <a id=link href=one.html><section id=linked><p>linked words</p><p>more linked</p></section></a>
</main>";

fn page() -> Page {
    let mut page = Page::parse(MARKUP, "https://example.test/");
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    page
}

fn layout(page: &Page, measurer: &mut Counting) -> LayoutOutput {
    layout_page_with_style_viewport(page, WIDTH, HEIGHT, WIDTH, measurer)
}

/// Lays the page out twice: a subtree is retained when a second pass lays it out again.
fn settle(page: &Page) {
    layout(page, &mut Counting::default());
    layout(page, &mut Counting::default());
}

fn element(page: &Page, id: &str) -> NodeRef {
    Node::descendants(&page.dom.document)
        .find(|node| node.attr("id").as_deref() == Some(id))
        .unwrap()
}

/// Debug output with every number rounded to hundredths: a replayed subtree is translated by a
/// float offset and can differ from a cold layout in the last bit.
fn normalized(value: &impl std::fmt::Debug) -> String {
    let text = format!("{value:?}");
    let mut output = String::new();
    let mut number = String::new();
    let flush = |number: &mut String, output: &mut String| {
        if !number.is_empty() {
            match number.parse::<f64>() {
                Ok(value) if number.contains('.') => {
                    output.push_str(&format!("{:.2}", (value * 100.0).round() / 100.0 + 0.0))
                }
                _ => output.push_str(number),
            }
            number.clear();
        }
    };
    for character in text.chars() {
        if character.is_ascii_digit() || character == '.' || (character == '-' && number.is_empty())
        {
            number.push(character);
        } else {
            flush(&mut number, &mut output);
            output.push(character);
        }
    }
    flush(&mut number, &mut output);
    output
}

fn sorted<V: std::fmt::Debug>(map: &HashMap<NodeId, V>) -> Vec<String> {
    let mut entries = map
        .iter()
        .map(|(id, value)| normalized(&(id, value)))
        .collect::<Vec<_>>();
    entries.sort();
    entries
}

fn assert_same_output(warm: &LayoutOutput, cold: &LayoutOutput) {
    assert_eq!(
        normalized(&warm.items),
        normalized(&cold.items),
        "display items"
    );
    assert_eq!(warm.node_paint_order, cold.node_paint_order);
    assert_eq!(sorted(&warm.node_bounds), sorted(&cold.node_bounds));
    assert_eq!(sorted(&warm.resize_boxes), sorted(&cold.resize_boxes));
    assert_eq!(sorted(&warm.scroll_boxes), sorted(&cold.scroll_boxes));
    assert_eq!(sorted(&warm.clip_paths), sorted(&cold.clip_paths));
    assert_eq!(
        normalized(
            &warm
                .fragments
                .text
                .iter()
                .collect::<std::collections::BTreeMap<_, _>>()
        ),
        normalized(
            &cold
                .fragments
                .text
                .iter()
                .collect::<std::collections::BTreeMap<_, _>>()
        )
    );
    assert_eq!(
        normalized(
            &warm
                .fragments
                .elements
                .iter()
                .collect::<std::collections::BTreeMap<_, _>>()
        ),
        normalized(
            &cold
                .fragments
                .elements
                .iter()
                .collect::<std::collections::BTreeMap<_, _>>()
        )
    );
    assert_eq!(
        normalized(&warm.content_height),
        normalized(&cold.content_height)
    );
    assert_eq!(
        normalized(&warm.sticky_layers),
        normalized(&cold.sticky_layers)
    );
}

/// Lays out with whatever the page retained, then from nothing, and compares the two.
fn assert_warm_matches_cold(page: &Page) -> LayoutOutput {
    let warm = layout(page, &mut Counting::default());
    let retained = std::mem::take(&mut *page.layout_reuse.borrow_mut());
    let cold = layout(page, &mut Counting::default());
    *page.layout_reuse.borrow_mut() = retained;
    assert_same_output(&warm, &cold);
    warm
}

#[test]
fn unchanged_sections_replay_without_measuring_text() {
    let page = page();
    let mut first = Counting::default();
    let cold = layout(&page, &mut first);
    assert!(
        page.layout_reuse.borrow().len().1 >= 5,
        "a first layout retains the sections"
    );
    let mut second = Counting::default();
    let warm = layout(&page, &mut second);
    assert!(
        second.calls * 4 < first.calls,
        "replayed sections skip their text ({} vs {})",
        second.calls,
        first.calls
    );
    assert_same_output(&warm, &cold);
}

#[test]
fn a_change_in_one_section_relayouts_it_and_translates_the_rest() {
    let mut page = page();
    settle(&page);
    // A taller first section moves every later section down.
    Node::set_text_content(
        &element(&page, "text"),
        "a much longer paragraph that now wraps across several more lines than before it did",
    );
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    assert_warm_matches_cold(&page);

    element(&page, "list").set_attr("class", "tall");
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    assert_warm_matches_cold(&page);
}

#[test]
fn recorded_inputs_outside_dom_and_style_invalidate_replays() {
    let mut page = page();
    settle(&page);

    // An image decode changes only the recorded natural size.
    let url = page.image_url(&element(&page, "icon")).unwrap();
    page.images.insert(
        url,
        crate::engine::DecodedImage {
            width: 30,
            height: 70,
            bgra: vec![0; 30 * 70 * 4].into(),
        },
    );
    let warm = assert_warm_matches_cold(&page);
    assert!(warm.node_bounds[&element(&page, "icon").id()].height >= 70.0);

    // A nested scroll changes only the recorded scroll offset.
    element(&page, "scroll").scroll_offset.set((0.0, 15.0));
    assert_warm_matches_cold(&page);

    // Ancestors outside a retained section: the backdrop a control composites and the link
    // promoted content inherits.
    element(&page, "backdrop").set_attr("class", "dark");
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    assert_warm_matches_cold(&page);
    element(&page, "link").set_attr("href", "two.html");
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    assert_warm_matches_cold(&page);
    // History navigation moves the URL a relative link resolves against without mutating the
    // document.
    settle(&page);
    page.source_url = "https://example.test/moved/".into();
    let warm = assert_warm_matches_cold(&page);
    assert!(format!("{:?}", warm.items).contains("https://example.test/moved/two.html"));

    // Zoom on an ancestor scales the used styles inside every section.
    let body = page.dom.elements_named("body").next().unwrap();
    body.set_attr("style", "zoom:2");
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    assert_warm_matches_cold(&page);
}

#[test]
fn relatively_positioned_sections_keep_their_stacking_order_when_replayed() {
    let mut page = page();
    settle(&page);
    Node::set_text_content(&element(&page, "text"), "short");
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    let warm = assert_warm_matches_cold(&page);
    assert!(
        warm.node_paint_order
            .contains(&element(&page, "raised").id())
    );
}

#[test]
fn subtrees_with_fixed_boxes_are_never_retained() {
    let mut page = Page::parse(
        "<style>section{display:flow-root}.pin{position:fixed;top:0}</style>
         <section id=fixed><p>a</p><p>b</p><div class=pin><p>pinned</p></div></section>",
        "https://example.test/",
    );
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    settle(&page);
    assert!(!is_retained(&page, "fixed"));
}

fn is_retained(page: &Page, id: &str) -> bool {
    let node = element(page, id).id();
    page.layout_reuse
        .borrow()
        .subtrees
        .keys()
        .any(|key| key.node == node)
}

#[test]
fn sticky_boxes_replay_with_their_subtree_and_stick_at_the_current_scroll() {
    // A sticky box's offset follows the scroll position, which is not a layout input.
    let mut page = Page::parse(
        "<style>section{display:flow-root}p{height:200px;margin:0}.stick{position:sticky;top:10px}</style>
         <section id=changing><p id=text>changing</p></section>
         <section id=holder><div class=stick id=stuck>stuck</div><p>alpha beta</p><p>gamma delta</p>
           <p>epsilon zeta</p><p>eta theta</p><p>iota kappa</p><p>lambda mu</p></section>",
        "https://example.test/",
    );
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    page.dom.document.scroll_offset.set((0.0, 400.0));
    settle(&page);
    assert!(is_retained(&page, "holder"));
    Node::set_text_content(&element(&page, "text"), "changed");
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    let mut replayed = Counting::default();
    let warm = layout(&page, &mut replayed);
    let retained = std::mem::take(&mut *page.layout_reuse.borrow_mut());
    let mut cold = Counting::default();
    layout(&page, &mut cold);
    *page.layout_reuse.borrow_mut() = retained;
    assert!(
        replayed.calls * 3 < cold.calls,
        "the sticky box's section replays ({} vs {} measurements)",
        replayed.calls,
        cold.calls
    );
    let stuck = element(&page, "stuck").id();
    // In flow at 208px (body margin and the first section); stuck 10px below the viewport's
    // top edge at a 400px scroll.
    assert_eq!(warm.sticky_offsets[&stuck], (0.0, 202.0));
    assert_warm_matches_cold(&page);
}

#[test]
fn blocks_sharing_a_float_context_replay_clear_of_earlier_floats_and_restore_their_own() {
    let mut page = Page::parse(
        "<style>div{background:#eee}p{margin:4px 0;background:#ddd}
            .side{float:left;width:200px;height:0}.tall{height:300px}
            .pull{float:right;width:100px;height:160px}</style>
         <div class=side id=side></div>
         <div id=lead><p>alpha beta</p></div>
         <div id=beside><p>words beside a float</p><p>more words</p><p>still more</p><p>and more</p></div>
         <div id=owner><div class=pull></div><p>next to a float</p><p>that overhangs</p><p>below it</p></div>
         <div id=after><p>these words wrap</p><p>around the float</p><p>the block left</p></div>",
        "https://example.test/",
    );
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    settle(&page);
    let retained = |page: &Page, id: &str| {
        let node = element(page, id).id();
        page.layout_reuse
            .borrow()
            .subtrees
            .keys()
            .any(|key| key.node == node)
    };
    assert!(retained(&page, "beside"), "a block clear of earlier floats");
    assert!(retained(&page, "owner"), "a block with its own float");

    // Moving a block that owns an overhanging float carries the float to its new position.
    element(&page, "lead").set_attr("class", "tall");
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    assert_warm_matches_cold(&page);
    assert!(retained(&page, "owner"));

    // A float growing over a retained block must not replay its unwrapped lines.
    element(&page, "side").set_attr("class", "side tall");
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    assert_warm_matches_cold(&page);
}

#[test]
fn sections_with_table_fixup_boxes_replay_and_keep_those_boxes() {
    let mut page = Page::parse(
        "<style>section{display:flow-root;background:#eee}p{background:#ddd}
            figure{display:table;margin:0;background:#ccc}figcaption{display:table-caption}
            .tall{height:200px}</style>
         <section id=lead><p>lead</p></section>
         <section id=figure><p>before</p><figure><p>image</p><p>stand-in</p>
            <figcaption>caption words</figcaption></figure><p>after</p><p>more</p></section>",
        "https://example.test/",
    );
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    let mut first = Counting::default();
    layout(&page, &mut first);
    layout(&page, &mut Counting::default());
    let node = element(&page, "figure").id();
    assert!(
        page.layout_reuse
            .borrow()
            .subtrees
            .keys()
            .any(|key| key.node == node)
    );

    element(&page, "lead").set_attr("class", "tall");
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    let mut second = Counting::default();
    layout(&page, &mut second);
    assert!(
        second.calls * 2 < first.calls,
        "{} vs {}",
        second.calls,
        first.calls
    );
    assert_warm_matches_cold(&page);
}
