use super::*;

fn check(cache: &mut AncestorFilterCache, node: &NodeRef, input: &str, expected: bool) {
    let selector = parse_selector(input).unwrap();
    let filter = cache.for_node(node);
    let matches = selector_matches(&selector, node);
    assert_eq!(matches, expected, "full matcher: {input}");
    assert_eq!(
        filter.may_match(&selector) && matches,
        matches,
        "cached: {input}"
    );
    assert_eq!(
        filter.may_match(&selector),
        AncestorFilter::new(node).may_match(&selector),
        "fresh filter: {input}"
    );
}

#[test]
fn ancestor_mutations_and_reparenting_invalidate_cached_keys() {
    let dom = dom::parse("<main class=old><p>x</p></main><aside class=new></aside>");
    let main = dom.elements_named("main").next().unwrap();
    let aside = dom.elements_named("aside").next().unwrap();
    let node = dom.elements_named("p").next().unwrap();
    let mut cache = AncestorFilterCache::default();
    check(&mut cache, &node, ".old p", true);
    check(&mut cache, &node, ".new p", false);
    main.set_attr("class", "new");
    main.set_attr("id", "container");
    main.set_attr("data-scope", "yes");
    check(&mut cache, &node, ".new#container[data-scope] p", true);
    main.remove_attr("class");
    main.remove_attr("data-scope");
    check(&mut cache, &node, ".new p", false);
    check(&mut cache, &node, "[data-scope] p", false);
    assert!(Node::append_child(&aside, node.clone()));
    check(&mut cache, &node, "aside.new > p", true);
    check(&mut cache, &node, "main p", false);
    assert!(Node::remove_child(&aside, &node));
    check(&mut cache, &node, ".new p", false);
    assert!(Node::append_child(&main, node.clone()));
    check(&mut cache, &node, "#container p", true);
}

#[test]
fn adoption_uses_current_tree_epoch_instead_of_allocation_document() {
    let first = dom::parse("<main class=old><p>x</p></main>");
    let second = dom::parse("<main class=new></main>");
    let node = first.elements_named("p").next().unwrap();
    let destination = second.elements_named("main").next().unwrap();
    let mut cache = AncestorFilterCache::default();
    check(&mut cache, &node, ".old p", true);
    assert!(Node::append_child(&destination, node.clone()));
    check(&mut cache, &node, ".new p", true);
    destination.set_attr("class", "changed");
    check(&mut cache, &node, ".changed p", true);
    check(&mut cache, &node, ".new p", false);
    destination.set_attr("data-scope", "");
    check(&mut cache, &node, "[data-scope] p", true);
}

#[test]
fn shadow_host_keys_do_not_cross_the_selector_tree_boundary() {
    let dom = dom::parse("<main class=outside><div></div></main>");
    let host = dom.elements_named("div").next().unwrap();
    let shadow =
        Node::attach_shadow(&host, dom::ShadowRootMode::Open, false, false, false).unwrap();
    let inside = Node::create_element_for(&dom.document, "section");
    let node = Node::create_element_for(&dom.document, "p");
    inside.set_attr("class", "inside");
    assert!(Node::append_child(&shadow, inside.clone()));
    assert!(Node::append_child(&inside, node.clone()));
    let mut cache = AncestorFilterCache::default();
    check(&mut cache, &node, ".outside p", false);
    check(&mut cache, &node, ".inside p", true);
    inside.set_attr("class", "changed");
    check(&mut cache, &node, ".changed p", true);
    check(&mut cache, &node, ".inside p", false);
}

#[test]
fn filtering_never_bypasses_the_full_ancestry_or_structural_match() {
    let dom =
        dom::parse("<main class=a><section class=b><p></p><p id=target></p></section></main>");
    let node = dom.elements_named("p").last().unwrap();
    let mut cache = AncestorFilterCache::default();
    for (selector, expected) in [
        (".a .b p", true),
        (".b .a p", false),
        (".a > p", false),
        (".a p:first-of-type", false),
        (".a p:last-child", true),
        (".missing p:first-of-type", false),
        (".b p + p", true),
        (":is(.missing,.a) p", true),
        (":not(.a) > p", true),
    ] {
        check(&mut cache, &node, selector, expected);
    }
}
