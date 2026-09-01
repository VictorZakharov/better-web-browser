use super::*;
use crate::limits::MAX_DOM_DEPTH;

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
