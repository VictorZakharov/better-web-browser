use super::*;

fn generations(styles: &StyleSet, node: &NodeRef) -> (Option<u64>, Option<u64>) {
    (
        styles.own_layout_generation(node.id()),
        styles.subtree_layout_generation(node.id()),
    )
}

#[test]
fn a_style_change_advances_its_own_and_ancestor_generations_only() {
    let dom = dom::parse(
        "<style>.wide{width:200px}</style><main><section><p>a</p></section><aside><p>b</p></aside></main>",
    );
    let main = dom.elements_named("main").next().unwrap();
    let section = dom.elements_named("section").next().unwrap();
    let aside = dom.elements_named("aside").next().unwrap();
    let changed = dom.elements_named("p").next().unwrap();
    let sibling = dom.elements_named("p").nth(1).unwrap();
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    let before =
        [&main, &section, &aside, &changed, &sibling].map(|node| generations(&styles, node));
    assert!(
        before
            .iter()
            .all(|(own, subtree)| own.is_some() && subtree.is_some())
    );

    changed.set_attr("class", "wide");
    styles.refresh_subtrees(&dom.document, std::slice::from_ref(&changed), &[]);
    let after =
        [&main, &section, &aside, &changed, &sibling].map(|node| generations(&styles, node));

    // The changed element and every composed ancestor's subtree advance.
    assert_ne!(after[3], before[3]);
    assert_ne!(after[1].1, before[1].1);
    assert_ne!(after[0].1, before[0].1);
    // Ancestors' own styles did not change.
    assert_eq!(after[1].0, before[1].0);
    assert_eq!(after[0].0, before[0].0);
    // Unrelated subtrees keep their generations.
    assert_eq!(after[2], before[2]);
    assert_eq!(after[4], before[4]);
}

#[test]
fn an_unchanged_recalculation_keeps_generations() {
    let dom = dom::parse("<main><p class=x>a</p></main>");
    let main = dom.elements_named("main").next().unwrap();
    let p = dom.elements_named("p").next().unwrap();
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    let before = [&main, &p].map(|node| generations(&styles, node));
    p.set_attr("class", "y");
    styles.refresh_subtrees(&dom.document, std::slice::from_ref(&main), &[]);
    assert_eq!([&main, &p].map(|node| generations(&styles, node)), before);
}

#[test]
fn observed_generations_never_repeat_after_a_later_change() {
    let dom = dom::parse("<style>.a{width:1px}.b{width:2px}</style><main><p>a</p><p>b</p></main>");
    let main = dom.elements_named("main").next().unwrap();
    let first = dom.elements_named("p").next().unwrap();
    let second = dom.elements_named("p").nth(1).unwrap();
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);

    first.set_attr("class", "a");
    styles.refresh_subtrees(&dom.document, std::slice::from_ref(&first), &[]);
    // A layout records the subtree after the first change...
    let recorded = styles.subtree_layout_generation(main.id());
    // ...so a second change in the same document must not reuse that value.
    second.set_attr("class", "b");
    styles.refresh_subtrees(&dom.document, std::slice::from_ref(&second), &[]);
    assert_ne!(styles.subtree_layout_generation(main.id()), recorded);

    // Separately built style sets never share values for the same node.
    let other = StyleSet::from_dom(&dom, &[], 800.0);
    assert_ne!(
        other.subtree_layout_generation(main.id()),
        styles.subtree_layout_generation(main.id())
    );
}

#[test]
fn removed_and_cleared_styles_have_unknown_generations() {
    let dom = dom::parse("<main><p>a</p></main>");
    let main = dom.elements_named("main").next().unwrap();
    let p = dom.elements_named("p").next().unwrap();
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    assert!(Node::remove_child(&main, &p));
    styles.refresh_subtrees(&dom.document, std::slice::from_ref(&main), &[p.id()]);
    assert_eq!(generations(&styles, &p), (None, None));

    styles.clear_computed_styles();
    assert_eq!(generations(&styles, &main), (None, None));
}

#[test]
fn a_forked_style_set_shares_no_generation_with_its_source() {
    // The script layout snapshot forks the page's styles; a layout result recorded against
    // the page must not be reused against the fork, or the other way around.
    let dom = dom::parse("<main><section><p>a</p></section></main>");
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let fork = styles.fork();
    for node in Node::descendants(&dom.document).filter(|node| node.element().is_some()) {
        let (own, subtree) = generations(&styles, &node);
        let (fork_own, fork_subtree) = generations(&fork, &node);
        assert!(fork_own.is_some() && fork_subtree.is_some());
        assert_ne!(own, fork_own);
        assert_ne!(subtree, fork_subtree);
    }
}
