use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

const WIDTH: f32 = 800.0;
const HEIGHT: f32 = 600.0;

const MARKUP: &str = r#"<style>
    .sticky{position:sticky;top:0} .ghost{pointer-events:none} .hidden{visibility:hidden}
    .inline{display:inline} .gone{display:none}
</style><main>
  <header class=sticky id=bar>bar <a href=x>link <b>bold</b></a></header>
  <section id=one><p>alpha <span id=span>span <i>it</i></span> beta</p>
    <div class=ghost id=ghost>ghost <em>text</em></div></section>
  <section id=two><p class=hidden id=hid>hidden <span>words</span></p>
    <nav class=sticky><span>nav</span></nav></section>
  <section id=host><template shadowrootmode=open><div class=sticky><slot></slot></div>
    <span class=ghost>shadow <u>text</u></span></template><b id=slotted>slotted</b>
    <i slot=none>unslotted</i></section>
</main>"#;

fn page() -> Page {
    let mut page = Page::parse(MARKUP, "https://example.test/");
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    page
}

fn element(page: &Page, id: &str) -> NodeRef {
    Node::descendants(&page.dom.document)
        .find(|node| node.attr("id").as_deref() == Some(id))
        .unwrap()
}

fn ids(nodes: &[NodeRef]) -> Vec<NodeId> {
    nodes.iter().map(|node| node.id()).collect()
}

/// The whole-document walks the index replaces.
fn walked(page: &Page, styles: &StyleSet) -> (Vec<NodeId>, Vec<NodeId>, HashSet<NodeId>) {
    fn inlines(node: &NodeRef, styles: &StyleSet, out: &mut Vec<NodeId>) {
        for child in Node::composed_children(node) {
            inlines(&child, styles, out);
        }
        if node.element().is_some()
            && styles
                .styles
                .get(&node.id())
                .is_some_and(|style| style.display == Display::Inline)
        {
            out.push(node.id());
        }
    }
    let mut inline = Vec::new();
    inlines(&page.dom.document, styles, &mut inline);
    let sticky = Node::composed_descendants(&page.dom.document)
        .filter(|node| styles.get(node).position == Position::Sticky)
        .map(|node| node.id())
        .collect();
    let mut excluded = HashSet::new();
    for node in Node::shadow_including_descendants(&page.dom.document) {
        let eligible = styles
            .styles
            .get(&node.id())
            .map(|style| style.pointer_events && style.visibility)
            .or_else(|| Node::composed_parent(&node).map(|parent| !excluded.contains(&parent.id())))
            .unwrap_or(true);
        if !eligible {
            excluded.insert(node.id());
        }
    }
    (inline, sticky, excluded)
}

fn indexed(page: &Page, styles: &StyleSet) -> (Vec<NodeId>, Vec<NodeId>, HashSet<NodeId>) {
    let mut index = page.layout_index.borrow_mut();
    let composed = index.composed(&page.dom.document, styles);
    let excluded = index.excluded(&page.dom.document, styles);
    (
        ids(&composed.inlines()),
        ids(&composed.sticky()),
        excluded.nodes().collect(),
    )
}

type Mutation = Box<dyn Fn(&Page)>;

#[test]
fn retained_lists_equal_whole_document_walks_across_mutations() {
    let mut page = page();
    let mutations: Vec<Mutation> = vec![
        Box::new(|page| {
            element(page, "span").set_attr("class", "ghost");
        }),
        Box::new(|page| {
            element(page, "ghost").set_attr("class", "ghost inline");
        }),
        Box::new(|page| {
            element(page, "two").set_attr("class", "gone");
        }),
        Box::new(|page| {
            element(page, "two").set_attr("class", "");
            Node::remove_child(&element(page, "two"), &element(page, "hid"));
        }),
        Box::new(|page| {
            let added = Node::create_element_for(&page.dom.document, "span");
            added.set_attr("class", "sticky ghost");
            Node::append_child(&added, Node::create_text_for(&page.dom.document, "added"));
            Node::append_child(&element(page, "one"), added);
        }),
        Box::new(|page| {
            element(page, "bar").set_attr("class", "sticky hidden");
        }),
        Box::new(|page| {
            element(page, "slotted").set_attr("class", "ghost");
        }),
        Box::new(|page| {
            Node::append_child(&element(page, "two"), element(page, "span"));
        }),
    ];
    for (step, mutate) in std::iter::once(None)
        .chain(mutations.iter().map(Some))
        .enumerate()
    {
        if let Some(mutate) = mutate {
            mutate(&page);
            page.refresh_resources_for_viewport(WIDTH, HEIGHT);
        }
        let styles = page.cached_style_for_viewport(WIDTH, HEIGHT).unwrap();
        let expected = walked(&page, styles);
        assert_eq!(indexed(&page, styles), expected, "after mutation {step}");
        let opened = page.layout_index.borrow().opened;
        assert_eq!(indexed(&page, styles), expected, "unchanged, step {step}");
        assert_eq!(
            page.layout_index.borrow().opened,
            opened,
            "an unchanged document reuses its lists, step {step}"
        );
    }
    let styles = page.cached_style_for_viewport(WIDTH, HEIGHT).unwrap();
    let (inlines, sticky, excluded) = indexed(&page, styles);
    assert!(!inlines.is_empty() && sticky.len() >= 3 && !excluded.is_empty());
}

#[test]
fn a_class_change_rebuilds_only_the_changed_path() {
    // Sibling sections large enough to be retained around the one that changes.
    let siblings =
        "<section><p>alpha <a href=x>link</a> <b>bold</b> <i>it</i> omega</p></section>".repeat(20);
    let markup = MARKUP.replace("</main>", &format!("{siblings}</main>"));
    let mut page = Page::parse(&markup, "https://example.test/");
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    let styles = page.cached_style_for_viewport(WIDTH, HEIGHT).unwrap();
    indexed(&page, styles);
    let full = std::mem::take(&mut page.layout_index.borrow_mut().opened);
    element(&page, "ghost").set_attr("class", "");
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    let styles = page.cached_style_for_viewport(WIDTH, HEIGHT).unwrap();
    assert_eq!(indexed(&page, styles), walked(&page, styles));
    let changed = page.layout_index.borrow().opened;
    // The path from the document to the div with its small sibling subtrees, for each of the
    // two traversals; the twenty sibling sections are reused.
    assert!(
        changed * 4 < full,
        "rebuilt {changed} of {full} nodes for one class change"
    );
}

#[test]
fn layout_with_retained_lists_equals_a_fresh_index() {
    let mut page = page();
    let lay = |page: &Page| layout_page(page, WIDTH, HEIGHT, &mut FixedMeasurer);
    lay(&page);
    element(&page, "span").set_attr("class", "ghost inline");
    element(&page, "bar").set_attr("class", "");
    Node::append_child(&element(&page, "two"), element(&page, "bar"));
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    let warm = lay(&page);
    let retained = std::mem::take(&mut *page.layout_index.borrow_mut());
    let cold = lay(&page);
    *page.layout_index.borrow_mut() = retained;
    let sorted = |output: &LayoutOutput| {
        let mut bounds = output
            .node_bounds
            .iter()
            .map(|(id, rect)| format!("{id:?} {rect:?}"))
            .collect::<Vec<_>>();
        bounds.sort();
        let mut excluded = output.hit_excluded.iter().copied().collect::<Vec<_>>();
        excluded.sort();
        let mut fragments = output
            .fragments
            .elements
            .keys()
            .copied()
            .collect::<Vec<_>>();
        fragments.sort();
        (
            bounds,
            excluded,
            fragments,
            format!("{:?}", output.sticky_layers),
        )
    };
    assert_eq!(sorted(&warm), sorted(&cold));
    assert!(!warm.hit_excluded.is_empty() && !warm.sticky_layers.is_empty());
}
