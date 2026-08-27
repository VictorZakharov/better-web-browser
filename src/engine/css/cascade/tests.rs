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
