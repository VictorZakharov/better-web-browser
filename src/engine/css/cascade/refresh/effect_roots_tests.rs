use super::*;
use crate::engine::invalidation::MutationKind;

mod benchmark;

#[test]
fn target_effect_refresh_matches_full_cascade_without_recascading_siblings() {
    let dom = dom::parse(
        "<style>main{color:red;--tone:red}main::before{content:'x';color:inherit}</style><main><span style='color:var(--tone)'>a</span></main><aside style='color:blue'>b</aside>",
    );
    let target = dom.elements_named("main").next().unwrap();
    let child = dom.elements_named("span").next().unwrap();
    let sibling = dom.elements_named("aside").next().unwrap();
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    let old_sibling = styles.get(&sibling).clone();
    for text in [
        "color:green;--tone:green",
        "transform:scale(.5);clip-path:inset(10%)",
        "display:none",
        "",
    ] {
        target.set_animation_style(text);
        let stats = styles.refresh_subtrees_after_invalidation(
            &dom.document,
            std::slice::from_ref(&target),
            &[],
            MutationKind::StyleOverlay.impact(),
        );
        let fresh = StyleSet::from_dom(&dom, &[], 800.0);
        assert_eq!(styles.styles, fresh.styles);
        assert_eq!(styles.get(&sibling), &old_sibling);
        assert_eq!(
            stats.recomputed_styles,
            Node::composed_descendants(&target).count()
        );
        assert_eq!(styles.get(&child), fresh.get(&child));
        let pseudo = PseudoElement::Before;
        assert_eq!(
            styles.pseudo_styles.get(&(target.id(), pseudo)),
            fresh.pseudo_styles.get(&(target.id(), pseudo))
        );
    }
}

#[test]
fn target_root_font_effect_recascades_all_rem_consumers() {
    let dom = dom::parse(
        "<html style='font-size:10px'><body><main style='width:2rem'></main><aside style='width:3rem'></aside></body></html>",
    );
    let target = dom.elements_named("html").next().unwrap();
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    target.set_transition_style("font-size:20px");
    let stats = styles.refresh_subtrees_after_invalidation(
        &dom.document,
        &[target],
        &[],
        MutationKind::StyleOverlay.impact(),
    );
    assert!(stats.layout_changed);
    assert_eq!(styles.styles, StyleSet::from_dom(&dom, &[], 800.0).styles);
}

#[test]
fn relational_style_dependencies_refresh_the_anchor_subtree() {
    let dom = dom::parse(
        "<style>body:has(main[style]) aside{color:red}</style><main></main><aside></aside>",
    );
    let target = dom.elements_named("main").next().unwrap();
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    target.set_animation_style("color:green");
    let stats = styles.refresh_subtrees_after_invalidation(
        &dom.document,
        std::slice::from_ref(&target),
        &[],
        MutationKind::StyleOverlay.impact(),
    );
    // An animation overlay is not a `style` attribute, so body's remembered result holds.
    assert_eq!(stats.recomputed_styles, 1);
    assert_eq!(styles.styles, StyleSet::from_dom(&dom, &[], 800.0).styles);
    target.set_attr("style", "color:blue");
    let stats = styles.refresh_subtrees_after_invalidation(
        &dom.document,
        &[target],
        &[],
        MutationKind::Attribute("style").impact(),
    );
    // The keyed body anchor's result flips, bounding the refresh to its subtree with `aside`.
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(
        stats.recomputed_styles,
        Node::composed_descendants(&body).count()
    );
    assert_eq!(styles.styles, StyleSet::from_dom(&dom, &[], 800.0).styles);
}
