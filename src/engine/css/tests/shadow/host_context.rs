use super::*;
use crate::engine::MediaEnvironment;
use crate::engine::invalidation::RenderInvalidation;

#[test]
fn nested_declarative_roots_match_shadow_including_host_context_and_refresh_on_ancestor_change() {
    let dom = dom::parse(
        r#"<outer-card class=theme><template shadowrootmode=open>
             <inner-card><template shadowrootmode=closed>
               <style>
                 :host-context(.theme) { background-color: #123456 }
                 :host-context(.theme) > .direct { font-size: 27px }
                 :host-context(.theme) .deep { color: #345678 }
                 :host-context(.outer > .theme) { background-color: red }
               </style>
               <span class=direct><b class=deep>Text</b></span>
             </template></inner-card>
           </template></outer-card>"#,
    );
    let outer = dom.elements_named("outer-card").next().unwrap();
    let outer_root = outer.shadow_root().expect("open outer DSD root");
    let inner = Node::descendants(&outer_root)
        .find(|node| node.tag_name() == Some("inner-card"))
        .unwrap();
    let inner_root = inner.shadow_root().expect("closed inner DSD root");
    let direct = Node::descendants(&inner_root)
        .find(|node| node.has_class("direct"))
        .unwrap();
    let deep = Node::descendants(&inner_root)
        .find(|node| node.has_class("deep"))
        .unwrap();
    let selected = Color::rgb(0x12, 0x34, 0x56);
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(styles.get(&inner).background_color, selected);
    assert_eq!(styles.get(&direct).font_size, 27.0);
    assert_eq!(styles.get(&deep).color, Color::rgb(0x34, 0x56, 0x78));

    outer.set_attr("class", "inactive");
    let stats = styles.refresh_rules_after_invalidation(
        &dom,
        "",
        &[],
        MediaEnvironment::new(800.0, 800.0, 1.0, false),
        &RenderInvalidation::full(outer.id()),
    );
    assert!(!stats.full_rebuild);
    assert_ne!(styles.get(&inner).background_color, selected);
    assert_ne!(styles.get(&direct).font_size, 27.0);
    assert_eq!(styles.styles, StyleSet::from_dom(&dom, &[], 800.0).styles);

    // The host itself also participates in :host-context's ancestor walk.
    inner.set_attr("class", "theme");
    let stats = styles.refresh_rules_after_invalidation(
        &dom,
        "",
        &[],
        MediaEnvironment::new(800.0, 800.0, 1.0, false),
        &RenderInvalidation::full(inner.id()),
    );
    assert!(!stats.full_rebuild);
    assert_eq!(styles.get(&inner).background_color, selected);
    assert_eq!(styles.styles, StyleSet::from_dom(&dom, &[], 800.0).styles);
}

#[test]
fn host_context_qualifies_slotted_rules_without_leaking_outside_the_component() {
    let dom = dom::parse(
        r#"<main class=theme><x-card><span class=hot>Light</span>
             <template shadowrootmode=closed>
               <style>:host-context(.theme) slot::slotted(.hot) {
                 background-color: #123456;
               }</style><slot></slot>
             </template></x-card></main>"#,
    );
    let main = dom.elements_named("main").next().unwrap();
    let light = dom.elements_named("span").next().unwrap();
    let selected = Color::rgb(0x12, 0x34, 0x56);
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(styles.get(&light).background_color, selected);

    main.set_attr("class", "inactive");
    let stats = styles.refresh_rules_after_invalidation(
        &dom,
        "",
        &[],
        MediaEnvironment::new(800.0, 800.0, 1.0, false),
        &RenderInvalidation::full(main.id()),
    );
    assert!(!stats.full_rebuild);
    assert_ne!(styles.get(&light).background_color, selected);
    assert_eq!(styles.styles, StyleSet::from_dom(&dom, &[], 800.0).styles);
}
