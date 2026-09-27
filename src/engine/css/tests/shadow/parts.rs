use super::*;
use crate::engine::MediaEnvironment;
use crate::engine::invalidation::RenderInvalidation;

#[test]
fn closed_declarative_parts_obey_exposure_intersection_and_encapsulation_cascade() {
    let dom = dom::parse(
        r#"<style>
             x-card::part(label) { color: #123456; font-size: 28px !important }
             x-card.active::part(label active) { background-color: #345678 }
             x-card::part(label missing) { background-color: red }
             x-card span { color: red }
           </style>
           <x-card class=active><template shadowrootmode=closed>
             <style>.inner { color: #010203; font-size: 12px !important }</style>
             <span id=exposed class=inner part="label active">Exposed</span>
             <span id=private class=inner>Private</span>
           </template></x-card>"#,
    );
    let host = dom.elements_named("x-card").next().unwrap();
    let root = host.shadow_root().expect("closed DSD root");
    let by_id = |id| {
        Node::descendants(&root)
            .find(|node| node.attr("id").as_deref() == Some(id))
            .unwrap()
    };
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let exposed = styles.get(&by_id("exposed"));
    assert_eq!(exposed.color, Color::rgb(0x12, 0x34, 0x56));
    assert_eq!(exposed.background_color, Color::rgb(0x34, 0x56, 0x78));
    assert_eq!(exposed.font_size, 12.0);
    let private = styles.get(&by_id("private"));
    assert_eq!(private.color, Color::rgb(1, 2, 3));
    assert_ne!(private.background_color, exposed.background_color);
}

#[test]
fn exportparts_forwards_names_across_nested_closed_roots_without_exposing_unmapped_parts() {
    let dom = dom::parse(
        r#"<style>
             outer-box::part(public alias) { color: #123456 }
             outer-box::part(inner) { background-color: red }
             inner-box::part(inner) { background-color: red }
           </style>
           <outer-box><template shadowrootmode=open>
             <style>inner-box::part(inner) { background-color: #345678 }</style>
             <inner-box exportparts="inner: public, inner: alias, invalid mapping">
               <template shadowrootmode=closed>
                 <span id=forwarded part="inner extra">Forwarded</span>
                 <span id=hidden part=hidden>Hidden</span>
               </template>
             </inner-box>
           </template></outer-box>"#,
    );
    let outer = dom.elements_named("outer-box").next().unwrap();
    let outer_root = outer.shadow_root().unwrap();
    let inner = Node::descendants(&outer_root)
        .find(|node| node.tag_name() == Some("inner-box"))
        .unwrap();
    let inner_root = inner.shadow_root().expect("closed inner DSD root");
    let forwarded = Node::descendants(&inner_root)
        .find(|node| node.attr("id").as_deref() == Some("forwarded"))
        .unwrap();
    let hidden = Node::descendants(&inner_root)
        .find(|node| node.attr("id").as_deref() == Some("hidden"))
        .unwrap();
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(styles.get(&forwarded).color, Color::rgb(0x12, 0x34, 0x56));
    assert_eq!(
        styles.get(&forwarded).background_color,
        Color::rgb(0x34, 0x56, 0x78)
    );
    assert_ne!(styles.get(&hidden).color, styles.get(&forwarded).color);
    assert_ne!(
        styles.get(&hidden).background_color,
        styles.get(&forwarded).background_color
    );
}

#[test]
fn escaped_part_identifiers_and_malformed_export_mappings_are_independent() {
    let dom = dom::parse(
        r#"<style>outer-box::part(public) { color: #123456 }
                  outer-box::part(invalid) { background-color: red }</style>
           <outer-box><template shadowrootmode=open>
             <inner-box exportparts="bad mapping, a\+b: public, hidden:invalid:extra">
               <template shadowrootmode=closed>
                 <span id=chosen part="a+b">Chosen</span>
                 <span id=hidden part=hidden>Hidden</span>
               </template>
             </inner-box>
           </template></outer-box>"#,
    );
    let outer = dom.elements_named("outer-box").next().unwrap();
    let inner = Node::descendants(&outer.shadow_root().unwrap())
        .find(|node| node.tag_name() == Some("inner-box"))
        .unwrap();
    let root = inner.shadow_root().unwrap();
    let chosen = Node::descendants(&root)
        .find(|node| node.attr("id").as_deref() == Some("chosen"))
        .unwrap();
    let hidden = Node::descendants(&root)
        .find(|node| node.attr("id").as_deref() == Some("hidden"))
        .unwrap();
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(styles.get(&chosen).color, Color::rgb(0x12, 0x34, 0x56));
    assert_ne!(styles.get(&hidden).color, styles.get(&chosen).color);
    assert_ne!(styles.get(&hidden).background_color, Color::rgb(255, 0, 0));
}

#[test]
fn part_and_exportparts_mutations_refresh_exposed_styles_without_rebuilding_rules() {
    let dom = dom::parse(
        r#"<style>
             outer-box::part(surface) { background-color: #123456 }
             .theme outer-box::part(surface) { color: #345678 }
           </style>
           <main class=theme><outer-box><template shadowrootmode=open>
             <inner-box exportparts="chip: surface"><template shadowrootmode=closed>
               <span part=chip>Chip</span>
             </template></inner-box>
           </template></outer-box></main>"#,
    );
    let main = dom.elements_named("main").next().unwrap();
    let outer = dom.elements_named("outer-box").next().unwrap();
    let inner = Node::descendants(&outer.shadow_root().unwrap())
        .find(|node| node.tag_name() == Some("inner-box"))
        .unwrap();
    let chip = Node::descendants(&inner.shadow_root().unwrap())
        .find(|node| node.tag_name() == Some("span"))
        .unwrap();
    let selected_background = Color::rgb(0x12, 0x34, 0x56);
    let selected_color = Color::rgb(0x34, 0x56, 0x78);
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(styles.get(&chip).background_color, selected_background);
    assert_eq!(styles.get(&chip).color, selected_color);

    fn refresh(dom: &Dom, styles: &mut StyleSet, node: &NodeRef) {
        let stats = styles.refresh_rules_after_invalidation(
            dom,
            "",
            &[],
            MediaEnvironment::new(800.0, 800.0, 1.0, false),
            &RenderInvalidation::full(node.id()),
        );
        assert!(!stats.full_rebuild);
        assert_eq!(styles.styles, StyleSet::from_dom(dom, &[], 800.0).styles);
    }
    chip.set_attr("part", "renamed");
    refresh(&dom, &mut styles, &chip);
    assert_ne!(styles.get(&chip).background_color, selected_background);
    chip.set_attr("part", "chip");
    refresh(&dom, &mut styles, &chip);
    assert_eq!(styles.get(&chip).background_color, selected_background);
    inner.set_attr("exportparts", "chip: other");
    refresh(&dom, &mut styles, &inner);
    assert_ne!(styles.get(&chip).background_color, selected_background);
    inner.set_attr("exportparts", "chip: surface");
    refresh(&dom, &mut styles, &inner);
    assert_eq!(styles.get(&chip).background_color, selected_background);
    main.set_attr("class", "off");
    refresh(&dom, &mut styles, &main);
    assert_ne!(styles.get(&chip).color, selected_color);
}
