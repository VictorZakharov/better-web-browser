//! Anonymous text decoration is used-run state, not element CSS property inheritance.
use super::*;

fn text_child(node: &NodeRef) -> NodeRef {
    node.children
        .borrow()
        .iter()
        .find(|child| matches!(&child.data, NodeData::Text(_)))
        .cloned()
        .expect("direct text child")
}

#[test]
fn direct_decorating_parent_applies_underline_to_its_anonymous_text_run() {
    for html in [
        "<span style='text-decoration:underline'>inline</span>",
        "<p style='text-decoration:underline'>block</p>",
    ] {
        let dom = dom::parse(html);
        let parent = dom
            .elements_named("span")
            .next()
            .or_else(|| dom.elements_named("p").next())
            .unwrap();
        let styles = StyleSet::from_dom(&dom, &[], 800.0);
        assert!(styles.get(&parent).text_decoration_underline);
        assert!(styles.get(&text_child(&parent)).text_decoration_underline);
    }
}

#[test]
fn element_computed_decoration_remains_non_inherited() {
    let dom = dom::parse("<p style='text-decoration:underline'>direct<span>nested</span></p>");
    let parent = dom.elements_named("p").next().unwrap();
    let descendant = dom.elements_named("span").next().unwrap();
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    assert!(styles.get(&text_child(&parent)).text_decoration_underline);
    assert!(!styles.get(&descendant).text_decoration_underline);
    assert!(
        !styles
            .computed_style_for_node(&descendant)
            .unwrap()
            .text_decoration_underline
    );
    // This focused fix does not claim to implement the outer decorating box
    // across intervening element boxes; their computed property remains none.
    assert!(
        !styles
            .get(&text_child(&descendant))
            .text_decoration_underline
    );
}

#[test]
fn boxless_or_nonrendered_parent_does_not_create_anonymous_underline() {
    for display in ["contents", "none"] {
        let dom = dom::parse(&format!(
            "<span style='display:{display};text-decoration:underline'>text</span>"
        ));
        let parent = dom.elements_named("span").next().unwrap();
        let styles = StyleSet::from_dom(&dom, &[], 800.0);
        assert!(styles.get(&parent).text_decoration_underline);
        assert!(!styles.get(&text_child(&parent)).text_decoration_underline);
    }
}

#[test]
fn incremental_direct_underline_changes_stay_geometry_equivalent_and_deferable() {
    let dom = dom::parse("<span>text</span>");
    let parent = dom.elements_named("span").next().unwrap();
    let text = text_child(&parent);
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    for (declaration, expected) in [("text-decoration:underline", true), ("", false)] {
        parent.set_attr("style", declaration);
        let stats = styles.refresh_subtrees(&dom.document, std::slice::from_ref(&parent), &[]);
        assert_eq!(
            stats.changed_styles, 2,
            "element and anonymous run are current"
        );
        assert!(!stats.layout_changed);
        assert!(!stats.non_deferable_paint_changes);
        assert_eq!(styles.get(&parent).text_decoration_underline, expected);
        assert_eq!(styles.get(&text).text_decoration_underline, expected);
    }
}

#[test]
fn direct_decorating_parent_applies_underline_to_cdata_runs() {
    let document = dom::document::xml::parse(
        "<root style='text-decoration:underline'><![CDATA[text]]></root>",
    )
    .expect("owned XML fixture");
    let dom = Dom::from_existing_document(
        document,
        html5ever::interface::tree_builder::QuirksMode::NoQuirks,
    );
    let parent = dom.elements_named("root").next().unwrap();
    let cdata = parent
        .children
        .borrow()
        .iter()
        .find(|child| matches!(&child.data, NodeData::Cdata(_)))
        .cloned()
        .expect("CDATA child");
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    assert!(styles.get(&cdata).text_decoration_underline);
}
