use super::*;
use crate::engine::MediaEnvironment;
use crate::engine::invalidation::RenderInvalidation;

#[test]
fn declarative_slots_distinguish_assigned_elements_from_fallback_content() {
    let dom = dom::parse(
        r#"<x-card><span slot=filled>Assigned</span><template shadowrootmode=closed>
             <style>
               slot:has-slotted { color: #123456 }
               slot:not(:has-slotted) { color: #654321 }
             </style>
             <slot name=empty>Fallback</slot><slot name=filled>Hidden fallback</slot>
           </template></x-card>"#,
    );
    let host = dom.elements_named("x-card").next().unwrap();
    let light = dom.elements_named("span").next().unwrap();
    let root = host.shadow_root().unwrap();
    let slots = Node::descendants(&root)
        .filter(|node| node.tag_name() == Some("slot"))
        .collect::<Vec<_>>();
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(styles.get(&slots[0]).color, Color::rgb(0x65, 0x43, 0x21));
    assert_eq!(styles.get(&slots[1]).color, Color::rgb(0x12, 0x34, 0x56));

    light.set_attr("slot", "empty");
    let stats = styles.refresh_rules_after_invalidation(
        &dom,
        "",
        &[],
        MediaEnvironment::new(800.0, 800.0, 1.0, false),
        &RenderInvalidation::full(host.id()),
    );
    assert!(!stats.full_rebuild);
    assert_eq!(styles.get(&slots[0]).color, Color::rgb(0x12, 0x34, 0x56));
    assert_eq!(styles.get(&slots[1]).color, Color::rgb(0x65, 0x43, 0x21));
    assert_eq!(styles.styles, StyleSet::from_dom(&dom, &[], 800.0).styles);
}

#[test]
fn whitespace_text_assignment_satisfies_has_slotted() {
    let dom = dom::parse(
        "<x-card> \t <template shadowrootmode=open><style>slot:has-slotted{color:red}</style>\
         <slot></slot></template></x-card>",
    );
    let root = dom
        .elements_named("x-card")
        .next()
        .unwrap()
        .shadow_root()
        .unwrap();
    let slot = Node::descendants(&root)
        .find(|node| node.tag_name() == Some("slot"))
        .unwrap();
    assert!(
        Node::assigned_nodes(&slot, false)
            .iter()
            .any(|node| node.element().is_none())
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(styles.get(&slot).color, Color::rgb(255, 0, 0));
}

#[test]
fn manual_slot_assignment_toggles_has_slotted_without_rebuilding_rules() {
    let dom = dom::parse("<x-card><span>Selected</span></x-card>");
    let host = dom.elements_named("x-card").next().unwrap();
    let light = dom.elements_named("span").next().unwrap();
    let root = Node::attach_shadow_with_assignment(
        &host,
        crate::engine::dom::ShadowRootMode::Open,
        false,
        false,
        false,
        true,
    )
    .unwrap();
    Node::replace_inner_html(
        &root,
        "<style>slot:has-slotted{font-size:23px}</style><slot>Fallback</slot>",
        true,
    );
    let slot = Node::descendants(&root)
        .find(|node| node.tag_name() == Some("slot"))
        .unwrap();
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    assert_ne!(styles.get(&slot).font_size, 23.0);

    for (assigned, expected) in [(true, true), (false, false)] {
        let requested = if assigned {
            vec![light.clone()]
        } else {
            Vec::new()
        };
        Node::assign_manual_nodes(&slot, &requested);
        let stats = styles.refresh_rules_after_invalidation(
            &dom,
            "",
            &[],
            MediaEnvironment::new(800.0, 800.0, 1.0, false),
            &RenderInvalidation::full(slot.id()),
        );
        assert!(!stats.full_rebuild);
        assert_eq!(styles.get(&slot).font_size == 23.0, expected);
        assert_eq!(styles.styles, StyleSet::from_dom(&dom, &[], 800.0).styles);
    }
}
