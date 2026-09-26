use super::super::*;

#[test]
fn scopes_shadow_rules_and_inherits_through_slots_in_the_composed_tree() {
    let dom = dom::parse(
        r#"<style>.inside { color: red } .light { font-size: 11px }</style>
            <x-card id="host" class="theme"><span class="light">Light</span></x-card>"#,
    );
    let host = dom.elements_named("x-card").next().unwrap();
    let light = dom.elements_named("span").next().unwrap();
    let root = Node::attach_shadow(
        &host,
        crate::engine::dom::ShadowRootMode::Open,
        false,
        false,
        false,
    )
    .unwrap();
    Node::replace_inner_html(
        &root,
        r#"<style>:host(.theme) { color: #123456 } .inside { color: green }
            ::slotted(.light) { font-size: 24px }</style>
            <div class="inside">Shadow</div><slot></slot>"#,
        true,
    );
    let inside = Node::descendants(&root)
        .find(|node| node.has_class("inside"))
        .unwrap();
    let styles = StyleSet::from_dom(&dom, &[], 1000.0);

    assert_eq!(styles.get(&host).color, Color::rgb(0x12, 0x34, 0x56));
    assert_eq!(styles.get(&inside).color, Color::rgb(0, 128, 0));
    // CSS Cascade 5 sorts encapsulation context before specificity and source order: the
    // light-DOM normal declaration wins over a normal ::slotted() default.
    assert_eq!(styles.get(&light).font_size, 11.0);
}

#[test]
fn encapsulation_context_precedes_layer_order_and_reverses_for_important() {
    let dom = dom::parse(
        r#"<style>@layer page { #host { color: blue } .light { font-size: 11px } }</style>
           <x-card id=host><span class=light>Light</span></x-card>"#,
    );
    let host = dom.elements_named("x-card").next().unwrap();
    let light = dom.elements_named("span").next().unwrap();
    let root = Node::attach_shadow(
        &host,
        crate::engine::dom::ShadowRootMode::Open,
        false,
        false,
        false,
    )
    .unwrap();
    Node::replace_inner_html(
        &root,
        "<style>@layer component { :host { color: red } ::slotted(.light) { font-size: 24px } }</style><slot></slot>",
        true,
    );
    let styles = StyleSet::from_dom(&dom, &[], 1000.0);
    assert_eq!(styles.get(&host).color, Color::rgb(0, 0, 255));
    assert_eq!(styles.get(&light).font_size, 11.0);

    Node::replace_inner_html(
        &root,
        "<style>@layer component { :host { color: green !important } ::slotted(.light) { font-size: 24px !important } }</style><slot></slot>",
        true,
    );
    let styles = StyleSet::from_dom(&dom, &[], 1000.0);
    assert_eq!(styles.get(&host).color, Color::rgb(0, 128, 0));
    assert_eq!(styles.get(&light).font_size, 24.0);

    host.set_attr("style", "color: blue !important");
    light.set_attr("style", "font-size: 11px !important");
    let styles = StyleSet::from_dom(&dom, &[], 1000.0);
    assert_eq!(styles.get(&host).color, Color::rgb(0, 128, 0));
    assert_eq!(styles.get(&light).font_size, 24.0);
}

#[test]
fn nested_shadow_contexts_keep_independent_layer_precedence() {
    let dom = dom::parse("<outer-box></outer-box>");
    let outer = dom.elements_named("outer-box").next().unwrap();
    let outer_root = Node::attach_shadow(
        &outer,
        crate::engine::dom::ShadowRootMode::Open,
        false,
        false,
        false,
    )
    .unwrap();
    Node::replace_inner_html(
        &outer_root,
        "<style>@layer outer { inner-box { color: blue } }</style><inner-box></inner-box>",
        true,
    );
    let inner = Node::descendants(&outer_root)
        .find(|node| node.tag_name() == Some("inner-box"))
        .unwrap();
    let inner_root = Node::attach_shadow(
        &inner,
        crate::engine::dom::ShadowRootMode::Open,
        false,
        false,
        false,
    )
    .unwrap();
    Node::replace_inner_html(
        &inner_root,
        "<style>@layer inner { :host { color: red } }</style><slot></slot>",
        true,
    );
    let styles = StyleSet::from_dom(&dom, &[], 1000.0);
    assert_eq!(styles.get(&inner).color, Color::rgb(0, 0, 255));

    Node::replace_inner_html(
        &inner_root,
        "<style>@layer inner { :host { color: red !important } }</style><slot></slot>",
        true,
    );
    let styles = StyleSet::from_dom(&dom, &[], 1000.0);
    assert_eq!(styles.get(&inner).color, Color::rgb(255, 0, 0));
}

#[test]
fn adopted_host_rules_resolve_component_size_custom_properties() {
    let dom = dom::parse("<x-card></x-card>");
    let host = dom.elements_named("x-card").next().unwrap();
    let root = Node::attach_shadow(
        &host,
        crate::engine::dom::ShadowRootMode::Open,
        false,
        false,
        false,
    )
    .unwrap();
    root.set_adopted_stylesheets(vec![crate::engine::AdoptedStyleSheet {
        base_url: "https://example.test/component.css".into(),
        media: String::new(),
        source: r#":host {
            --width-card-1u: 300px;
            --card-height: 304px;
            width: var(--override-card-width, var(--width-card-1u)) !important;
            min-width: var(--width-card-1u) !important;
            max-width: var(--width-card-1u) !important;
            height: var(--card-height);
        }"#
        .into(),
    }]);

    let styles = StyleSet::from_dom(&dom, &[], 1000.0);
    let style = styles.get(&host);
    assert_eq!(
        style
            .custom_properties
            .get("--width-card-1u")
            .map(String::as_str),
        Some("300px")
    );
    assert_eq!(
        variables::substitute_variables(
            "var(--override-card-width, var(--width-card-1u))",
            &style.custom_properties,
        )
        .as_deref()
        .map(str::trim),
        Some("300px")
    );
    assert_eq!(style.width, Length::Px(300.0));
    assert_eq!(style.min_width, Length::Px(300.0));
    assert_eq!(style.max_width, Length::Px(300.0));
    assert_eq!(style.height, Length::Px(304.0));
}

#[test]
fn adopted_host_descendant_rules_match_the_owning_shadow_hosts_state() {
    let dom = dom::parse("<x-card immersive></x-card><x-card></x-card>");
    let hosts = dom.elements_named("x-card").collect::<Vec<_>>();
    let mut media = Vec::new();
    for host in &hosts {
        let root = Node::attach_shadow(
            host,
            crate::engine::dom::ShadowRootMode::Open,
            false,
            false,
            false,
        )
        .unwrap();
        Node::replace_inner_html(&root, r#"<div class="media">Image</div>"#, true);
        root.set_adopted_stylesheets(vec![crate::engine::AdoptedStyleSheet {
            base_url: "https://example.test/component.css".into(),
            media: String::new(),
            source: r#".media { position: relative }
                :host([immersive]:not([wide])) .media { position: absolute }"#
                .into(),
        }]);
        media.push(
            Node::descendants(&root)
                .find(|node| node.has_class("media"))
                .unwrap(),
        );
    }

    let styles = StyleSet::from_dom(&dom, &[], 1000.0);
    assert_eq!(styles.get(&media[0]).position, Position::Absolute);
    assert_eq!(styles.get(&media[1]).position, Position::Relative);
}

#[test]
fn opt_in_style_diagnostics_find_elements_inside_shadow_trees() {
    let dom = dom::parse("<x-card></x-card>");
    let host = dom.elements_named("x-card").next().unwrap();
    let root = Node::attach_shadow(
        &host,
        crate::engine::dom::ShadowRootMode::Open,
        false,
        false,
        false,
    )
    .unwrap();
    Node::replace_inner_html(&root, r#"<section id="inside">Shadow</section>"#, true);
    let styles = StyleSet::from_dom(&dom, &[], 1000.0);

    let matches = styles.query_selector_all(&dom, "#inside").unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].tag_name(), Some("section"));
}
