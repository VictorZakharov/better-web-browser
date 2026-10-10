use super::*;

fn ids(children: Vec<NodeRef>) -> Vec<NodeId> {
    children.iter().map(|node| node.id()).collect()
}

#[test]
fn ordinary_cached_children_equal_fresh_composed_pseudo_contents_and_popover_traversal() {
    let page = Page::parse(
        r#"<style>
        main::before {content:'before'} main::after {content:'after'}
        .contents {display:contents} .hidden {display:none}
        </style><main><b>first</b><section class=contents><i>nested</i>
        <div class=contents><em>deeper</em></div></section><u class=hidden>hidden</u>
        <aside popover>top layer</aside><strong>last</strong></main>"#,
        "https://example.test/",
    );
    let main = page.dom.elements_named("main").next().unwrap();
    let popover = page.dom.elements_named("aside").next().unwrap();
    popover.set_popover_order(1);
    let styles = page.style(800.0);
    let mut tree = BoxTree::new(&page.dom.document, &styles);
    let before = styles
        .generated_pseudo(&main, PseudoElement::Before)
        .unwrap();
    let after = styles
        .generated_pseudo(&main, PseudoElement::After)
        .unwrap();
    let expected = vec![
        before.id(),
        page.dom.elements_named("b").next().unwrap().id(),
        page.dom.elements_named("i").next().unwrap().id(),
        page.dom.elements_named("em").next().unwrap().id(),
        page.dom.elements_named("strong").next().unwrap().id(),
        after.id(),
    ];
    let complete = ids(tree.children(&main));
    let elements = tree
        .children(&main)
        .into_iter()
        .filter(|node| node.element().is_some())
        .collect();
    assert_eq!(ids(elements), expected);
    let cached = tree
        .children
        .remove(&main.id())
        .expect("ordinary parent cached");
    assert_eq!(ids(tree.children(&main)), complete);
    tree.children.insert(main.id(), cached);
    assert_eq!(ids(tree.children(&main)), complete);
}

#[test]
fn slot_assignment_order_is_cached_without_substituting_light_dom_children() {
    let page = Page::parse(
        r#"<section id=host><template shadowrootmode=open>
        <main><slot name=second></slot><slot name=first></slot></main>
        </template><b slot=first>first</b><i slot=second>second</i></section>"#,
        "https://example.test/",
    );
    let host = page.dom.elements_named("section").next().unwrap();
    let root = host.shadow_root().unwrap();
    let main = Node::descendants(&root)
        .find(|node| node.tag_name() == Some("main"))
        .unwrap();
    let styles = page.style(800.0);
    let mut tree = BoxTree::new(&page.dom.document, &styles);
    let cached = tree
        .children
        .remove(&main.id())
        .expect("composed parent cached");
    assert_eq!(ids(cached.clone()), ids(tree.children(&main)));
    tree.children.insert(main.id(), cached);
    let assigned = tree
        .children(&main)
        .into_iter()
        .filter_map(|node| node.tag_name().map(str::to_owned))
        .collect::<Vec<_>>();
    assert_eq!(assigned, ["i", "b"]);
}

#[test]
fn empty_table_column_cache_preserves_pruning_without_mutating_dom_identity() {
    let page = Page::parse(
        "<style>main{display:table-column}</style><main><b>not a column child</b></main>",
        "https://example.test/",
    );
    let main = page.dom.elements_named("main").next().unwrap();
    let original = ids(main.children.borrow().clone());
    let version = page.dom.document.document_mutation_version();
    let styles = page.style(800.0);
    let tree = BoxTree::new(&page.dom.document, &styles);
    assert!(tree.children.contains_key(&main.id()));
    assert!(tree.children(&main).is_empty());
    assert_eq!(ids(main.children.borrow().clone()), original);
    assert_eq!(page.dom.document.document_mutation_version(), version);
}

#[test]
fn each_new_layout_pass_observes_new_topology_and_styles() {
    let page = Page::parse(
        "<style>.hidden{display:none}</style><main><b>old</b></main>",
        "https://example.test/",
    );
    let main = page.dom.elements_named("main").next().unwrap();
    let old = page.dom.elements_named("b").next().unwrap();
    let styles = page.style(800.0);
    let first = BoxTree::new(&page.dom.document, &styles);
    assert_eq!(ids(first.children(&main)), [old.id()]);
    drop(first);
    let next = Node::create_element("span");
    Node::append_child(&main, next.clone());
    old.set_attr("class", "hidden");
    let styles = page.style(800.0);
    let second = BoxTree::new(&page.dom.document, &styles);
    assert_eq!(ids(second.children(&main)), [next.id()]);
}
