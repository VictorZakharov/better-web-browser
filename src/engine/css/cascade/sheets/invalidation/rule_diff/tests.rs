//! A stylesheet change restyles only the subjects of changed rules, and always equals a fresh
//! whole-document cascade.
use super::super::*;
use crate::engine::dom::Dom;

fn environment() -> MediaEnvironment {
    MediaEnvironment::new(800.0, 800.0, 1.0, false)
}

const BODY: &str = "<main><p class=y><span>inherits</span></p><p class=z>other</p>\
    <b id=target>t</b></main><aside><p>far</p><p>far</p><p>far</p><p>far</p></aside>";

/// Starts from `<style>` elements with `sheets` and a trailing empty one, applies `mutate` (which returns the mutation
/// root), refreshes rules, and compares every style with a fresh cascade.
fn refresh(sheets: &[&str], mutate: impl FnOnce(&Dom) -> NodeRef) -> (StyleRefreshStats, usize) {
    let styles_html = sheets
        .iter()
        .map(|css| format!("<style>{css}</style>"))
        .collect::<String>()
        + "<style></style>";
    let dom = dom::parse(&format!("<head>{styles_html}</head><body>{BODY}</body>"));
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    let root = mutate(&dom);
    let stats = styles.refresh_rules_after_invalidation(
        &dom,
        "",
        &[],
        environment(),
        &RenderInvalidation {
            roots: vec![root.id()],
            impact: crate::engine::invalidation::MutationKind::ChildList.impact(),
            mutation_count: 1,
            rebuild_style_rules: true,
            ..Default::default()
        },
    );
    let fresh = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(styles.styles, fresh.styles, "{sheets:?}");
    for node in Node::descendants(&dom.document).filter(|node| node.element().is_some()) {
        for pseudo in [PseudoElement::Before, PseudoElement::After] {
            assert_eq!(
                styles
                    .generated_pseudo(&node, pseudo)
                    .map(|node| node.text_content()),
                fresh
                    .generated_pseudo(&node, pseudo)
                    .map(|node| node.text_content()),
            );
        }
    }
    (stats, fresh.styles.len())
}

fn append_sheet(css: &'static str) -> impl FnOnce(&Dom) -> NodeRef {
    move |dom| {
        let style = dom.elements_named("style").last().unwrap();
        Node::set_text_content(&style, css);
        style.parent().unwrap()
    }
}

fn sheet(dom: &Dom, index: usize) -> NodeRef {
    dom.elements_named("style").nth(index).unwrap()
}

#[test]
fn an_added_sheet_restyles_its_rule_subjects_and_their_subtrees() {
    for css in [
        ".y{color:red}",
        "#target{color:red}",
        "aside p{color:red}",
        ".y::before{content:'new'}",
        "main > .z:hover{color:red}",
        ".y:has(span){color:red}",
        "[class=y]{color:red}",
        ":is(.y, .z){color:red}",
        ":where(#target, aside p){color:red}",
        // Keyless subjects restyle the subtrees of their nearest keyed ancestor.
        "main :not(.z){color:red}",
        ".y > :not(b){color:red}",
        "aside *::before{content:'x'}",
    ] {
        let (stats, total) = refresh(&[".z{color:blue}"], append_sheet(css));
        assert!(!stats.full_rebuild, "{css}");
        assert!(
            stats.recomputed_styles < total,
            "{css}: {}/{total}",
            stats.recomputed_styles
        );
    }
}

#[test]
fn removing_or_editing_a_sheet_restyles_previous_and_new_subjects() {
    let (stats, total) = refresh(&[".z{color:blue}", ".y{color:red}"], |dom| {
        let removed = sheet(dom, 1);
        let head = removed.parent().unwrap();
        Node::remove_from_parent(&removed);
        head
    });
    assert!(!stats.full_rebuild);
    assert!(stats.recomputed_styles < total);
    let (stats, _) = refresh(&[".z{color:blue}", ".y{color:red}"], |dom| {
        Node::set_text_content(&sheet(dom, 1), "#target{color:red}");
        sheet(dom, 1).parent().unwrap()
    });
    assert!(!stats.full_rebuild);
}

#[test]
fn keyless_subjects_layer_order_and_sheet_order_rebuild_the_document() {
    for css in [
        "*{color:red}",
        ":not(.y){color:red}",
        ":is(.y, *){color:red}",
        // A sibling's subject is outside the keyed element's subtree.
        ".y + :not(b){color:red}",
        ".y ~ *{color:red}",
        ":root{--x:1}",
    ] {
        let (stats, _) = refresh(
            &["@layer first{.y{color:blue}} @layer second{.y{color:green}}"],
            append_sheet(css),
        );
        assert!(stats.full_rebuild, "{css}");
    }
    // An earlier layer statement reorders layers whose rules are otherwise unchanged.
    let (stats, _) = refresh(
        &[
            "",
            "@layer first{.y{color:blue}} @layer second{.y{color:green}}",
        ],
        |dom| {
            Node::set_text_content(&sheet(dom, 0), "@layer second, first;");
            sheet(dom, 0).parent().unwrap()
        },
    );
    assert!(stats.full_rebuild);
    // Reordering otherwise unchanged sheets changes which of two equal-specificity rules wins.
    let (stats, _) = refresh(&[".y{color:red}", ".y{color:blue}"], |dom| {
        let first = sheet(dom, 0);
        let head = first.parent().unwrap();
        Node::remove_from_parent(&first);
        Node::append_child(&head, first);
        head
    });
    assert!(stats.full_rebuild);
}

#[test]
fn added_keyframes_restyle_only_the_subjects_of_added_rules() {
    // Wikipedia's late module styles define keyframes; each used to restyle the whole document.
    let (stats, total) = refresh(
        &[".z{color:blue}"],
        append_sheet("@keyframes spin{to{opacity:0}} .y{animation-name:spin}"),
    );
    assert!(!stats.full_rebuild);
    assert!(stats.recomputed_styles < total, "{stats:?}");
    let (stats, total) = refresh(
        &[".z{color:blue}"],
        append_sheet("@keyframes spin{to{opacity:0}}"),
    );
    assert!(!stats.full_rebuild);
    assert!(stats.recomputed_styles < total, "{stats:?}");
}

#[test]
fn animation_discovery_finds_keyframes_added_without_a_full_rebuild() {
    let dom = dom::parse(&format!(
        "<head><style>.y{{animation-name:spin}}</style><style></style></head><body>{BODY}</body>"
    ));
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    let target = dom.elements_named("p").next().unwrap();
    assert!(styles.animation_keyframes(&target, "spin").is_none());
    let head = append_sheet("@keyframes spin{to{opacity:0}}")(&dom);
    let stats = styles.refresh_rules_after_invalidation(
        &dom,
        "",
        &[],
        environment(),
        &RenderInvalidation {
            roots: vec![head.id()],
            impact: crate::engine::invalidation::MutationKind::ChildList.impact(),
            mutation_count: 1,
            rebuild_style_rules: true,
            ..Default::default()
        },
    );
    assert!(!stats.full_rebuild);
    assert!(styles.animation_keyframes(&target, "spin").is_some());
}

#[test]
fn subjects_restyle_their_subtrees_only_when_their_own_style_changes() {
    // An added rule that does not apply yet (`:hover`) leaves the subject's style and its
    // subtree alone; one that applies restyles the subtree for inheritance.
    let (unchanged, _) = refresh(&[".z{color:blue}"], append_sheet("main:hover{color:red}"));
    let (changed, _) = refresh(&[".z{color:blue}"], append_sheet("main{color:red}"));
    assert!(!unchanged.full_rebuild && !changed.full_rebuild);
    assert!(
        unchanged.recomputed_styles < changed.recomputed_styles,
        "{} vs {}",
        unchanged.recomputed_styles,
        changed.recomputed_styles
    );
}

#[test]
fn subjects_restyle_only_where_the_whole_selector_matches() {
    // VisualEditor's `.ve-activated #catlinks a` names every link by key, but matches none until
    // an ancestor gains the class.
    let (baseline, _) = refresh(&[".z{color:blue}"], append_sheet(".absent{color:red}"));
    for css in [
        ".absent p{color:red}",
        ".absent > p::before{content:'x'}",
        "aside.absent p, .absent main p{color:red}",
    ] {
        let (stats, _) = refresh(&[".z{color:blue}"], append_sheet(css));
        assert!(!stats.full_rebuild, "{css}");
        assert_eq!(
            stats.recomputed_styles, baseline.recomputed_styles,
            "{css}: {stats:?}"
        );
    }
    // Matching subjects, their generated content and removed rules still restyle.
    for css in [
        "main > p{color:red}",
        "aside p::before{content:'x'}",
        "body .y span{color:red}",
    ] {
        refresh(&[".z{color:blue}"], append_sheet(css));
        refresh(&[".z{color:blue}", css], |dom| {
            Node::set_text_content(&sheet(dom, 1), "");
            sheet(dom, 1).parent().unwrap()
        });
    }
}
