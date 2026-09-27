use super::*;
use crate::engine::MediaEnvironment;
use crate::engine::invalidation::RenderInvalidation;

#[test]
fn declarative_shadow_slots_match_their_origin_and_only_direct_assigned_elements() {
    let dom = dom::parse(
        r#"<x-card>
            <span id=first class=hot data-on slot=first>First <b class=hot>nested</b></span>
            <span id=second class=hot slot=second>Second</span>
            <template shadowrootmode=closed>
              <style>
                ::slotted(.hot[data-on]) { background-color: #112233 }
                section.panel > slot.active::slotted(span.hot[data-on]) {
                  font-size: 29px;
                }
                slot[name=second]::slotted(.hot) { color: #345678 }
              </style>
              <section class=panel><slot name=first class=active></slot></section>
              <slot name=second></slot>
            </template>
           </x-card>"#,
    );
    let host = dom.elements_named("x-card").next().unwrap();
    assert!(
        host.shadow_root().is_some(),
        "parser must attach closed DSD"
    );
    let spans = dom.elements_named("span").collect::<Vec<_>>();
    let nested = dom.elements_named("b").next().unwrap();
    let styles = StyleSet::from_dom(&dom, &[], 800.0);

    assert_eq!(styles.get(&spans[0]).font_size, 29.0);
    assert_eq!(
        styles.get(&spans[0]).background_color,
        Color::rgb(0x11, 0x22, 0x33)
    );
    assert_eq!(styles.get(&spans[1]).color, Color::rgb(0x34, 0x56, 0x78));
    assert_ne!(
        styles.get(&spans[1]).background_color,
        Color::rgb(0x11, 0x22, 0x33)
    );
    // A slotted pseudo-element selects the assigned element, not its descendants.
    assert_ne!(
        styles.get(&nested).background_color,
        Color::rgb(0x11, 0x22, 0x33)
    );
}

#[test]
fn manual_reassignment_and_slot_selector_changes_refresh_slotted_styles_locally() {
    let dom = dom::parse("<x-card><span class=hot>Light</span></x-card>");
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
        "<style>.wrapper slot.first::slotted(.hot){font-size:21px}\
          .wrapper slot.second::slotted(.hot){font-size:32px}</style>\
          <section class=wrapper><slot class=first></slot><slot class=second></slot></section>",
        true,
    );
    let slots = Node::descendants(&root)
        .filter(|node| node.tag_name() == Some("slot"))
        .collect::<Vec<_>>();
    Node::assign_manual_nodes(&slots[0], std::slice::from_ref(&light));
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(styles.get(&light).font_size, 21.0);

    let affected = Node::assign_manual_nodes(&slots[1], std::slice::from_ref(&light));
    let mut invalidation = RenderInvalidation::full(affected[0].id());
    invalidation.roots = affected.iter().map(|node| node.id()).collect();
    let stats = styles.refresh_rules_after_invalidation(
        &dom,
        "",
        &[],
        MediaEnvironment::new(800.0, 800.0, 1.0, false),
        &invalidation,
    );
    assert!(!stats.full_rebuild);
    assert_eq!(styles.get(&light).font_size, 32.0);
    assert_eq!(styles.styles, StyleSet::from_dom(&dom, &[], 800.0).styles);

    slots[1].set_attr("class", "inactive");
    let stats = styles.refresh_rules_after_invalidation(
        &dom,
        "",
        &[],
        MediaEnvironment::new(800.0, 800.0, 1.0, false),
        &RenderInvalidation::full(slots[1].id()),
    );
    assert!(!stats.full_rebuild);
    assert_ne!(styles.get(&light).font_size, 32.0);
    assert_eq!(styles.styles, StyleSet::from_dom(&dom, &[], 800.0).styles);
}

#[test]
fn host_qualified_slot_origins_obey_the_shadow_boundary_and_host_invalidation() {
    let dom = dom::parse(
        r#"<x-card class=active>
             <span class=hot slot=direct>Direct</span>
             <span class=hot slot=deep>Deep</span>
             <template shadowrootmode=closed>
               <style>
                 :host(.active) slot::slotted(.hot) { background-color: #123456 }
                 :host(.active) > slot::slotted(.hot) { font-size: 27px }
               </style>
               <slot name=direct></slot>
               <section><slot name=deep></slot></section>
             </template>
           </x-card>"#,
    );
    let host = dom.elements_named("x-card").next().unwrap();
    let spans = dom.elements_named("span").collect::<Vec<_>>();
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    let selected = Color::rgb(0x12, 0x34, 0x56);
    assert_eq!(styles.get(&spans[0]).background_color, selected);
    assert_eq!(styles.get(&spans[1]).background_color, selected);
    assert_eq!(styles.get(&spans[0]).font_size, 27.0);
    assert_ne!(styles.get(&spans[1]).font_size, 27.0);

    host.set_attr("class", "inactive");
    let stats = styles.refresh_rules_after_invalidation(
        &dom,
        "",
        &[],
        MediaEnvironment::new(800.0, 800.0, 1.0, false),
        &RenderInvalidation::full(host.id()),
    );
    assert!(!stats.full_rebuild);
    assert_ne!(styles.get(&spans[0]).background_color, selected);
    assert_ne!(styles.get(&spans[1]).background_color, selected);
    assert_eq!(styles.styles, StyleSet::from_dom(&dom, &[], 800.0).styles);
}
