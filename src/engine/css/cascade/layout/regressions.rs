use super::*;

fn environment() -> MediaEnvironment {
    MediaEnvironment::new(800.0, 600.0, 1.0, false)
}

#[test]
fn hidden_mutations_and_rule_changes_rebuild_current_generated_content_on_reveal() {
    let source = |tone: &str, width: usize| {
        format!(
            "main{{--tone:{tone};--width:{width}px}}.hidden{{display:none}}\
             span{{color:var(--tone)}}\
             span::before{{content:attr(data-label);display:block;width:var(--width)}}"
        )
    };
    let dom = dom::parse(&format!(
        "<style>{}</style><main><span data-label=old>body</span></main>",
        source("red", 12)
    ));
    let parent = dom.elements_named("main").next().unwrap();
    let child = dom.elements_named("span").next().unwrap();
    let sheet = dom.elements_named("style").next().unwrap();
    let mut styles = StyleSet::from_sources_for_layout(&dom, "", &[], environment());
    let original = styles
        .generated_pseudo(&child, PseudoElement::Before)
        .unwrap();
    let old_generated_ids = Node::descendants(&original)
        .map(|node| node.id())
        .collect::<Vec<_>>();

    parent.set_attr("class", "hidden");
    styles.refresh_subtrees(&dom.document, std::slice::from_ref(&parent), &[]);
    assert!(!styles.styles.contains_key(&child.id()));
    assert!(
        styles
            .generated_pseudo(&child, PseudoElement::Before)
            .is_none()
    );
    assert!(
        old_generated_ids
            .iter()
            .all(|id| !styles.generated_styles.contains_key(id))
    );

    child.set_attr("data-label", "new label");
    let hidden = styles.refresh_subtrees(&dom.document, std::slice::from_ref(&child), &[]);
    assert_eq!(hidden.recomputed_styles, 0);
    Node::set_text_content(&sheet, &source("blue", 30));
    styles.rebuild_rules_for_media_environment(&dom, "", &[], environment(), &[]);
    assert!(!styles.styles.contains_key(&child.id()));
    assert!(
        styles
            .generated_pseudo(&child, PseudoElement::Before)
            .is_none()
    );

    parent.set_attr("class", "");
    let revealed = styles.refresh_subtrees(&dom.document, std::slice::from_ref(&parent), &[]);
    assert!(revealed.layout_changed);
    let fresh = StyleSet::from_sources_for_layout(&dom, "", &[], environment());
    assert_eq!(styles.get(&child), fresh.get(&child));
    assert_eq!(styles.get(&child).color, Color::rgb(0, 0, 255));
    let current = styles
        .generated_pseudo(&child, PseudoElement::Before)
        .unwrap();
    let expected = fresh
        .generated_pseudo(&child, PseudoElement::Before)
        .unwrap();
    assert_eq!(current.text_content(), "new label");
    assert_eq!(current.text_content(), expected.text_content());
    assert_eq!(styles.get(&current), fresh.get(&expected));
    assert_eq!(styles.get(&current).width, Length::Px(30.0));
    assert_ne!(current.id(), original.id());
}

#[test]
fn hidden_named_slot_defers_assigned_nodes_and_reveals_current_inherited_styles() {
    let dom = dom::parse(
        "<style>x-host{--tone:red}span{color:var(--tone);width:var(--width)}</style>\
         <x-host><span slot=content><em>assigned child</em></span></x-host>",
    );
    let host = dom.elements_named("x-host").next().unwrap();
    let child = dom.elements_named("span").next().unwrap();
    let nested = dom.elements_named("em").next().unwrap();
    let shadow = Node::attach_shadow(
        &host,
        crate::engine::dom::ShadowRootMode::Open,
        false,
        false,
        false,
    )
    .unwrap();
    Node::replace_inner_html(
        &shadow,
        "<style>slot{--width:20px}slot.hidden{display:none}</style><slot name=content></slot>",
        true,
    );
    let slot = Node::descendants(&shadow)
        .find(|node| node.tag_name() == Some("slot"))
        .unwrap();
    assert_eq!(child.parent().unwrap().id(), host.id());
    assert_eq!(Node::composed_parent(&child).unwrap().id(), slot.id());
    let mut styles = StyleSet::from_sources_for_layout(&dom, "", &[], environment());
    assert_eq!(styles.get(&child).width, Length::Px(20.0));
    slot.set_attr("class", "hidden");
    styles.refresh_subtrees(&dom.document, std::slice::from_ref(&slot), &[]);
    assert!(!styles.styles.contains_key(&child.id()));
    assert!(!styles.styles.contains_key(&nested.id()));

    child.set_attr("style", "font-size:24px");
    let hidden = styles.refresh_subtrees(&dom.document, std::slice::from_ref(&child), &[]);
    assert_eq!(hidden.recomputed_styles, 0);
    host.set_attr("style", "--tone:blue");
    slot.set_attr("style", "--width:40px");
    styles.refresh_subtrees(&dom.document, std::slice::from_ref(&host), &[]);
    assert!(!styles.styles.contains_key(&child.id()));

    slot.set_attr("class", "");
    styles.refresh_subtrees(&dom.document, std::slice::from_ref(&slot), &[]);
    let fresh = StyleSet::from_sources_for_layout(&dom, "", &[], environment());
    assert_eq!(styles.get(&child), fresh.get(&child));
    assert_eq!(styles.get(&nested), fresh.get(&nested));
    assert_eq!(styles.get(&child).color, Color::rgb(0, 0, 255));
    assert_eq!(styles.get(&child).width, Length::Px(40.0));
    assert_eq!(styles.get(&nested).font_size, 24.0);
}
