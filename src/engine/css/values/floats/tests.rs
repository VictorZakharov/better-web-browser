use super::*;
use crate::engine::{
    css::{StyleSet, cssom, supports},
    dom,
};

#[test]
fn clear_cascades_css_wide_keywords_and_rejects_invalid_values() {
    let dom = dom::parse(
        "<div style='clear:right'><p id=default></p><p id=inherit style='clear:inherit'></p><p id=unset style='clear:both;clear:unset'></p><p id=initial style='clear:left;clear:initial'></p><p id=revert style='clear:right;clear:revert'></p><p id=invalid style='clear:LEFT;clear:diagonal'></p></div>",
    );
    let styles = StyleSet::from_dom(&dom, &[], 600.0);
    for (node, expected) in dom.elements_named("p").zip([
        Clear::None,
        Clear::Right,
        Clear::None,
        Clear::None,
        Clear::None,
        Clear::Left,
    ]) {
        let style = styles.get(&node);
        assert_eq!(style.clear, expected, "{:?}", node.attr("id"));
        assert_eq!(
            cssom::resolved_property_value(style, "clear").as_deref(),
            Some(expected.css_keyword())
        );
    }
}

#[test]
fn clear_supports_and_invalidation_agree_with_the_cascade() {
    for keyword in ["none", "left", "right", "both", "LEFT"] {
        assert!(supports::supports_matches(&format!("(clear:{keyword})")));
    }
    for value in ["diagonal", "left right", "10px"] {
        assert!(!supports::supports_matches(&format!("(clear:{value})")));
    }
    let original = crate::engine::css::ComputedStyle::initial();
    let mut changed = original.clone();
    changed.clear = Clear::Both;
    assert!(!original.layout_equivalent(&changed));
}
