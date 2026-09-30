use super::*;

#[test]
fn unchanged_styles_are_not_marked_unsafe() {
    let dom = dom::parse("<p>plain</p>");
    let target = dom.elements_named("p").next().unwrap();
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    let stats = styles.refresh_subtrees(&dom.document, &[target], &[]);
    assert_eq!(stats.changed_styles, 0);
    assert!(!stats.layout_changed);
    assert!(!stats.non_deferable_paint_changes);
}

#[test]
fn table_background_and_plain_color_underline_refreshes_are_deferable() {
    for html in ["<table><tr><td>cell</td></tr></table>", "<p>plain</p>"] {
        let dom = dom::parse(html);
        let target = dom
            .elements_named("td")
            .next()
            .or_else(|| dom.elements_named("p").next())
            .unwrap();
        let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
        target.set_attr(
            "style",
            "color:blue;background:#f6f6f6;text-decoration:underline",
        );
        let stats = styles.refresh_subtrees(&dom.document, &[target], &[]);
        assert!(stats.changed_styles > 0);
        assert!(!stats.layout_changed);
        assert!(!stats.non_deferable_paint_changes);
    }
}

#[test]
fn inline_background_transition_is_unsafe_without_changing_geometry_contract() {
    let dom = dom::parse("<span>inline</span>");
    let target = dom.elements_named("span").next().unwrap();
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    target.set_attr("style", "background:red");
    let stats = styles.refresh_subtrees(&dom.document, std::slice::from_ref(&target), &[]);
    assert!(!stats.layout_changed);
    assert!(stats.non_deferable_paint_changes);
    target.set_attr("style", "");
    assert!(
        styles
            .refresh_subtrees(&dom.document, &[target], &[])
            .non_deferable_paint_changes
    );
}

#[test]
fn unsafe_change_in_another_dirty_root_is_not_overwritten_by_a_color_change() {
    let dom = dom::parse("<div>first</div><p>second</p>");
    let first = dom.elements_named("div").next().unwrap();
    let second = dom.elements_named("p").next().unwrap();
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    first.set_attr("style", "opacity:.5");
    second.set_attr("style", "color:blue");
    let stats = styles.refresh_subtrees(&dom.document, &[first, second], &[]);
    assert!(!stats.layout_changed);
    assert!(stats.non_deferable_paint_changes);
}

#[test]
fn missing_and_removed_styles_are_unsafe() {
    let dom = dom::parse("<div><p>child</p></div>");
    let target = dom.elements_named("div").next().unwrap();
    let child = dom.elements_named("p").next().unwrap();
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    styles.styles.remove(&child.id());
    let stats = styles.refresh_subtrees(&dom.document, std::slice::from_ref(&target), &[]);
    assert!(stats.non_deferable_paint_changes);
    assert!(stats.layout_changed);
    Node::remove_from_parent(&child);
    let stats = styles.refresh_subtrees(&dom.document, &[target], &[child.id()]);
    assert!(stats.removed_styles > 0);
    assert!(stats.non_deferable_paint_changes);
}

#[test]
fn generated_color_only_changes_are_unsafe_even_without_element_or_geometry_changes() {
    let dom = dom::parse(
        "<style>p::before{content:'generated';color:red}p.active::before{color:blue}</style><p>plain</p>",
    );
    let target = dom.elements_named("p").next().unwrap();
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    target.set_attr("class", "active");
    let stats = styles.refresh_subtrees(&dom.document, &[target], &[]);
    assert_eq!(stats.changed_styles, 0);
    assert!(!stats.layout_changed);
    assert!(stats.non_deferable_paint_changes);
}

#[test]
fn generated_box_text_and_placeholder_changes_are_unsafe() {
    let dom = dom::parse(
        "<style>p::before{content:attr(data-label)}p.removed::before{content:none}input.active::placeholder{color:blue}</style><p data-label=old>plain</p><input placeholder=hint>",
    );
    let target = dom.elements_named("p").next().unwrap();
    let input = dom.elements_named("input").next().unwrap();
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    target.set_attr("data-label", "new");
    assert!(
        styles
            .refresh_subtrees(&dom.document, std::slice::from_ref(&target), &[])
            .non_deferable_paint_changes
    );
    target.set_attr("class", "removed");
    assert!(
        styles
            .refresh_subtrees(&dom.document, &[target], &[])
            .non_deferable_paint_changes
    );
    input.set_attr("class", "active");
    assert!(
        styles
            .refresh_subtrees(&dom.document, &[input], &[])
            .non_deferable_paint_changes
    );
}
