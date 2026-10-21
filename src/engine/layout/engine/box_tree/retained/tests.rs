use super::*;

const WIDTH: f32 = 800.0;
const HEIGHT: f32 = 600.0;

const MARKUP: &str = "<style>
    .table{display:table} .inline-table{display:inline-table} .row{display:table-row}
    .cell{display:table-cell} .group{display:table-row-group} .caption{display:table-caption}
    .contents{display:contents} .hidden{display:none} .inline{display:inline}
    .marked::before{content:'b'} .marked::after{content:'a'} .zoomed{zoom:1.5}
</style>
<main id=n0>
  <div id=n1><span id=n2>one</span> <span id=n3>two</span></div>
  <section id=n4><p id=n5>three <b id=n6>four</b></p><i id=n7>five</i></section>
  <div id=n8> <em id=n9>six</em> <u id=n10>seven</u> </div>
  <ul id=n11><li id=n12>eight</li><li id=n13>nine</li><li id=n14>ten</li></ul>
  <aside id=n15><strong id=n16>eleven</strong><small id=n17>twelve</small></aside>
  <section id=n18><template shadowrootmode=open><div class=row><slot name=a></slot>
    <span class=cell>shadow</span><slot></slot></div></template>
    <b id=n19 slot=a>thirteen</b><i id=n20>fourteen</i></section>
</main>";

const CLASSES: &[&str] = &[
    "",
    "table",
    "inline-table",
    "row",
    "cell",
    "group",
    "caption",
    "contents",
    "hidden",
    "inline",
    "marked",
    "cell marked",
    "zoomed",
];

/// A deterministic xorshift sequence, so a failure names the seed and step that reproduce it.
struct Steps(u64);

impl Steps {
    fn below(&mut self, bound: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % bound as u64) as usize
    }
}

type Shape = Vec<(usize, String, String, ComputedStyle)>;

/// The tree's boxes in order: depth, identity (anonymous boxes by position), box-tree parent and
/// style, which must not depend on whether the tree was retained.
fn shape(tree: &BoxTree<'_>, root: &NodeRef) -> Shape {
    let name = |node: &NodeRef| {
        if tree.is_anonymous(node.id()) {
            "anonymous".to_owned()
        } else {
            format!("{:?}", node.id())
        }
    };
    let mut shape = Vec::new();
    let mut pending = vec![(root.clone(), 0)];
    while let Some((node, depth)) = pending.pop() {
        let parent = tree
            .parent(&node)
            .map_or_else(String::new, |parent| name(&parent));
        shape.push((depth, name(&node), parent, tree.get(&node).clone()));
        pending.extend(
            tree.children(&node)
                .into_iter()
                .rev()
                .map(|child| (child, depth + 1)),
        );
    }
    shape
}

fn outline(shape: Shape) -> Vec<(usize, String, String, Display)> {
    shape
        .into_iter()
        .map(|(depth, name, parent, style)| (depth, name, parent, style.display))
        .collect()
}

fn main_element(page: &Page) -> NodeRef {
    page.dom.elements_named("main").next().unwrap()
}

fn root(page: &Page) -> NodeRef {
    page.dom.elements_named("html").next().unwrap()
}

fn by_id(page: &Page, id: &str) -> NodeRef {
    Node::descendants(&page.dom.document)
        .find(|node| node.attr("id").as_deref() == Some(id))
        .unwrap()
}

fn mutate(page: &Page, steps: &mut Steps) {
    let main = main_element(page);
    let nodes = Node::descendants(&main)
        .filter(|node| node.element().is_some() && node.id() != main.id())
        .collect::<Vec<_>>();
    let node = nodes[steps.below(nodes.len())].clone();
    match steps.below(9) {
        0..=2 => {
            node.set_attr("class", CLASSES[steps.below(CLASSES.len())]);
        }
        3 => {
            let child = Node::create_element_for(&page.dom.document, "span");
            child.set_attr("class", CLASSES[steps.below(CLASSES.len())]);
            let text = if steps.below(2) == 0 { " " } else { "new" };
            Node::set_text_content(&child, text);
            Node::append_child(&node, child);
        }
        4 if nodes.len() > 6 => {
            if let Some(parent) = node.parent() {
                Node::remove_child(&parent, &node);
            }
        }
        5 => {
            // Moves the node, to `main` when the target is inside it.
            let target = nodes[steps.below(nodes.len())].clone();
            let inside = std::iter::successors(Some(target.clone()), |node| node.parent())
                .any(|ancestor| ancestor.id() == node.id());
            Node::append_child(if inside { &main } else { &target }, node);
        }
        6 => {
            // Slotting moves a light child between the shadow tree's slots.
            match steps.below(3) {
                0 => node.set_attr("slot", "a"),
                1 => node.set_attr("slot", "none"),
                _ => node.remove_attr("slot"),
            };
        }
        7 => node.set_popover_order(steps.below(2) as u64),
        _ => {
            let text = if steps.below(2) == 0 { "\n " } else { "text" };
            Node::set_text_content(&node, text);
        }
    }
}

#[test]
fn a_retained_tree_matches_a_fresh_build_after_each_mutation() {
    for seed in 1..=24u64 {
        let mut page = Page::parse(MARKUP, "https://example.test/");
        let mut steps = Steps(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15));
        page.refresh_resources_for_viewport(WIDTH, HEIGHT);
        for step in 0..30 {
            mutate(&page, &mut steps);
            page.refresh_resources_for_viewport(WIDTH, HEIGHT);
            let styles = page.cached_style_for_viewport(WIDTH, HEIGHT).unwrap();
            let root = root(&page);
            let retained = BoxTree::retained(&root, styles, &page.layout_box_tree, true);
            let fresh = BoxTree::new(&root, styles);
            let (retained_shape, fresh_shape) = (shape(&retained, &root), shape(&fresh, &root));
            if retained_shape != fresh_shape {
                let (retained, fresh) = (outline(retained_shape), outline(fresh_shape));
                let first = (0..retained.len().max(fresh.len()))
                    .find(|&index| retained.get(index) != fresh.get(index))
                    .unwrap_or(0);
                let start = first.saturating_sub(2);
                panic!(
                    "seed {seed} step {step}: boxes differ from index {first}\nretained {:?}\nfresh {:?}",
                    &retained[start.min(retained.len())..(start + 5).min(retained.len())],
                    &fresh[start.min(fresh.len())..(start + 5).min(fresh.len())]
                );
            }
            retained.retain();
        }
    }
}

#[test]
fn a_change_rebuilds_only_its_ancestors_and_keeps_other_anonymous_boxes() {
    let mut page = Page::parse(MARKUP, "https://example.test/");
    by_id(&page, "n12").set_attr("class", "cell");
    by_id(&page, "n13").set_attr("class", "cell");
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    let list = by_id(&page, "n11");
    let table = {
        let styles = page.cached_style_for_viewport(WIDTH, HEIGHT).unwrap();
        let tree = BoxTree::retained(&root(&page), styles, &page.layout_box_tree, true);
        assert!(tree.rebuilt > 20, "a first update builds every box");
        let table = tree.children(&list)[0].id();
        tree.retain();
        table
    };
    by_id(&page, "n6").set_attr("class", "hidden");
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    let styles = page.cached_style_for_viewport(WIDTH, HEIGHT).unwrap();
    let tree = BoxTree::retained(&root(&page), styles, &page.layout_box_tree, true);
    // html, body, main, the section and the paragraph that held the hidden element.
    assert_eq!(tree.rebuilt, 5);
    assert_eq!(tree.children(&list)[0].id(), table);
    assert!(!tree.is_transient_anonymous(table));
}
