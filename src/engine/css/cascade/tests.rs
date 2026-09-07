use super::*;
use crate::limits::MAX_DOM_DEPTH;

#[test]
fn generated_pseudos_invalidate_geometry_only_when_boxes_or_text_change() {
    let dom = dom::parse(
        r#"<style>
        p::before {content:attr(data-label);width:10px;display:block}
        p.wide::before {width:20px} p.removed::before {content:none}
        p.blue::before {color:blue} p::after {color:red}
        </style><p data-label='one'>body</p>"#,
    );
    let node = dom.elements_named("p").next().unwrap();
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    // CSSOM style queries can clear computed styles while retaining generated node identity.
    styles.clear_computed_styles();
    styles.computed_style_for_node(&node).unwrap();
    assert!(
        styles
            .generated_pseudo(&node, PseudoElement::Before)
            .is_some()
    );
    styles.compute_subtree(&dom.document, None);
    for (attribute, value, expected) in [
        ("data-unused", "changed", false),
        ("class", "blue", false),
        ("data-label", "two", true),
        ("class", "wide", true),
        ("class", "removed", true),
        ("data-label", "three", false),
        ("class", "", true),
        ("hidden", "", true),
    ] {
        node.set_attr(attribute, value);
        let stats = styles.refresh_subtrees(&dom.document, std::slice::from_ref(&node), &[]);
        assert_eq!(stats.layout_changed, expected, "{attribute}={value}");
    }
}

#[test]
fn incremental_styles_reuse_only_equal_custom_property_maps() {
    let dom = dom::parse(
        "<style>main {--tone:red} p {color:var(--tone)}</style><main><p>text</p></main>",
    );
    let main = dom.elements_named("main").next().unwrap();
    let child = dom.elements_named("p").next().unwrap();
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    let original = Arc::clone(&styles.get(&main).custom_properties);
    main.set_attr("data-unused", "changed");
    styles.refresh_subtrees(&dom.document, std::slice::from_ref(&main), &[]);
    assert!(Arc::ptr_eq(&original, &styles.get(&main).custom_properties));
    assert!(Arc::ptr_eq(
        &original,
        &styles.get(&child).custom_properties
    ));
    main.set_attr("style", "--tone:blue");
    styles.refresh_subtrees(&dom.document, std::slice::from_ref(&main), &[]);
    assert!(!Arc::ptr_eq(
        &original,
        &styles.get(&main).custom_properties
    ));
    assert_eq!(styles.get(&child).color, Color::rgb(0, 0, 255));
    assert_eq!(original.get("--tone").map(String::as_str), Some("red"));
}

#[test]
fn computes_the_bounded_maximum_dom_depth_without_recursive_style_walks() {
    let mut html = String::from("<main>");
    for _ in 0..MAX_DOM_DEPTH + 32 {
        html.push_str("<div>");
    }
    for _ in 0..MAX_DOM_DEPTH + 32 {
        html.push_str("</div>");
    }
    let dom = dom::parse(&html);
    let node_count = Node::descendants(&dom.document).count();
    let styles = StyleSet::from_dom(&dom, &[], 800.0);

    assert_eq!(styles.styles.len(), node_count);
}

#[test]
fn lazily_hydrates_a_newly_connected_subtree() {
    let dom = dom::parse("<main></main>");
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    let main = dom.elements_named("main").next().unwrap();
    let child = Node::create_element_for(&dom.document, "section");
    assert!(Node::append_child(&main, child.clone()));

    assert!(styles.computed_style_for_node(&child).is_some());
    assert!(styles.styles.contains_key(&child.id()));
}

#[test]
fn absolutely_positioned_elements_compute_float_to_none_regardless_of_source_order() {
    let dom = dom::parse(
        r#"<style>
              #before { float: left; position: absolute }
              #after { position: fixed; float: right }
            </style>
            <div id="before"></div><div id="after"></div>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    for node in dom.elements_named("div") {
        assert_eq!(styles.get(&node).float, Float::None);
    }
}

#[test]
fn generated_pseudo_boxes_are_styled_but_not_dom_children() {
    let dom = dom::parse(
        r#"<style>
            #target { color: #123456 }
            #target::before { content: attr(data-label); background: #abcdef }
        </style><div id="target" data-label="generated"></div>"#,
    );
    let target = dom.elements_named("div").next().unwrap();
    let dom_children = target.children.borrow().len();
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let pseudo = styles
        .generated_pseudo(&target, PseudoElement::Before)
        .unwrap();

    assert_eq!(pseudo.text_content(), "generated");
    assert_eq!(styles.get(&pseudo).color, Color::rgb(0x12, 0x34, 0x56));
    assert_eq!(
        styles.get(&pseudo).background_color,
        Color::rgb(0xab, 0xcd, 0xef)
    );
    assert_eq!(target.children.borrow().len(), dom_children);
    assert!(Node::descendants(&dom.document).all(|node| node.id() != pseudo.id()));
}
