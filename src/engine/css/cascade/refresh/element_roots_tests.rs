use super::*;
use crate::engine::invalidation::MutationKind;

/// Changed elements nested inside each other recompute the outer subtree once; it already
/// recomputes every element inside it.
#[test]
fn nested_changed_element_roots_are_recomputed_once() {
    let dom = dom::parse(
        "<style>.on{color:red}</style><main class=x><section class=x><p class=x>a<b>b</b></p></section></main><aside>c</aside>",
    );
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    let mut elements = ["p", "main", "section"]
        .map(|name| dom.elements_named(name).next().unwrap())
        .to_vec();
    for element in &elements {
        element.set_attr("class", "x on");
    }
    let stats = styles.refresh_subtrees_after_invalidation(
        &dom.document,
        &[],
        &elements,
        &[],
        MutationKind::Attribute("class").impact(),
    );
    let main = elements.remove(1);
    let subtree = Node::composed_descendants(&main).count();
    assert_eq!(stats.recomputed_styles, subtree);
    let fresh = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(styles.styles, fresh.styles);
}

/// A changed element root above subtree roots recomputes them once, as part of its subtree.
#[test]
fn subtree_roots_inside_a_changed_element_root_are_recomputed_once() {
    let dom = dom::parse("<style>.on{color:red}</style><main class=x><p>a</p><p>b</p></main>");
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    let main = dom.elements_named("main").next().unwrap();
    main.set_attr("class", "x on");
    let paragraphs = dom.elements_named("p").collect::<Vec<_>>();
    let stats = styles.refresh_subtrees_after_invalidation(
        &dom.document,
        &paragraphs,
        std::slice::from_ref(&main),
        &[],
        MutationKind::Attribute("class").impact(),
    );
    assert_eq!(
        stats.recomputed_styles,
        Node::composed_descendants(&main).count()
    );
    assert_eq!(styles.styles, StyleSet::from_dom(&dom, &[], 800.0).styles);
}
