use super::*;
use crate::engine::css::media::MediaEnvironment;
use crate::engine::dom::{self, Dom, ShadowRootMode};

fn styles(dom: &Dom) -> StyleSet {
    StyleSet::from_dom(dom, &[], 800.)
}

fn element(dom: &Dom, name: &str) -> NodeRef {
    dom.elements_named(name).next().unwrap()
}

#[test]
fn only_display_none_can_suppress_a_complete_subtree() {
    for (declaration, expected) in [
        ("display:none", true),
        ("display:block", false),
        ("display:contents", false),
        ("visibility:hidden", false),
        ("opacity:0", false),
        ("width:0;height:0;overflow:hidden", false),
        ("position:absolute;left:-10000px", false),
    ] {
        let dom = dom::parse(&format!(
            "<section style='{declaration}'><svg style='display:block;visibility:visible'></svg></section>"
        ));
        let svg = element(&dom, "svg");
        assert_eq!(suppressed(&svg, &styles(&dom)), expected, "{declaration}");
    }
}

#[test]
fn the_svg_itself_and_a_distant_hidden_ancestor_are_both_authoritative() {
    for html in [
        "<svg style='display:none'></svg>",
        "<section style='display:none'><div><article><svg></svg></article></div></section>",
        "<style>.off{display:none!important}</style><section class=off style='display:block'><svg></svg></section>",
    ] {
        let dom = dom::parse(html);
        assert!(suppressed(&element(&dom, "svg"), &styles(&dom)));
    }
}

#[test]
fn unknown_styles_are_not_a_visibility_proof() {
    let dom = dom::parse("<section style='display:none'><svg></svg></section>");
    assert!(!suppressed(&element(&dom, "svg"), &StyleSet::default()));
    let sparse = StyleSet::from_sources_for_layout(
        &dom,
        "https://example.test/",
        &[],
        MediaEnvironment::new(800., 600., 1., false),
    );
    let svg = element(&dom, "svg");
    assert!(!sparse.styles.contains_key(&svg.id()));
    let count = sparse.styles.len();
    assert!(suppressed(&svg, &sparse));
    assert_eq!(
        sparse.styles.len(),
        count,
        "proof must not hydrate the hidden tree"
    );
}

#[test]
fn closed_shadow_hosts_and_shadow_ancestors_suppress_raster_work() {
    for mode in [ShadowRootMode::Open, ShadowRootMode::Closed] {
        let dom = dom::parse("<x-host style='display:none'></x-host>");
        let host = element(&dom, "x-host");
        let shadow = Node::attach_shadow(&host, mode, false, false, false).unwrap();
        Node::replace_inner_html(&shadow, "<section><svg></svg></section>", true);
        let svg = Node::descendants(&shadow)
            .find(|n| n.tag_name() == Some("svg"))
            .unwrap();
        assert!(suppressed(&svg, &styles(&dom)));
        host.set_attr("style", "display:block");
        assert!(!suppressed(&svg, &styles(&dom)));
        let section = Node::descendants(&shadow)
            .find(|n| n.tag_name() == Some("section"))
            .unwrap();
        section.set_attr("style", "display:none");
        assert!(suppressed(&svg, &styles(&dom)));
    }
}

fn slotted() -> (Dom, NodeRef, NodeRef, NodeRef) {
    let dom = dom::parse("<x-host><main slot=content><svg></svg></main></x-host>");
    let host = element(&dom, "x-host");
    let target = element(&dom, "main");
    let svg = element(&dom, "svg");
    let shadow = Node::attach_shadow(&host, ShadowRootMode::Open, false, false, false).unwrap();
    Node::replace_inner_html(
        &shadow,
        "<slot name=content style='display:none'></slot>",
        true,
    );
    (dom, host, target, svg)
}

#[test]
fn assigned_hidden_slots_are_checked_for_normal_svg_descendants() {
    let (dom, _, _, svg) = slotted();
    assert!(suppressed(&svg, &styles(&dom)));
    let slot = Node::shadow_including_descendants(&dom.document)
        .find(|node| node.tag_name() == Some("slot"))
        .unwrap();
    slot.set_attr("style", "display:contents");
    assert!(!suppressed(&svg, &styles(&dom)));
}

#[test]
fn top_layers_bypass_a_hidden_slot_but_not_real_hidden_ancestry() {
    for popover in [false, true] {
        let (dom, host, target, svg) = slotted();
        if popover {
            target.set_attr("popover", "manual");
            target.set_popover_order(1);
        } else {
            target.set_fullscreen(true);
        }
        assert!(!suppressed(&svg, &styles(&dom)));
        host.set_attr("style", "display:none");
        assert!(suppressed(&svg, &styles(&dom)));
        host.set_attr("style", "display:block");
        target.set_attr("style", "display:none");
        assert!(suppressed(&svg, &styles(&dom)));
    }
}

#[test]
fn computed_ancestry_is_rechecked_after_rule_and_assignment_changes() {
    let (dom, _, target, svg) = slotted();
    assert!(suppressed(&svg, &styles(&dom)));
    let shadow = element(&dom, "x-host").shadow_root().unwrap();
    Node::replace_inner_html(
        &shadow,
        "<slot name=visible></slot><slot name=content style='display:none'></slot>",
        true,
    );
    target.set_attr("slot", "visible");
    assert!(!suppressed(&svg, &styles(&dom)));
    target.set_attr("slot", "content");
    assert!(suppressed(&svg, &styles(&dom)));
}
