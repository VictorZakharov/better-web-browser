//! Padding accepts only non-negative values.
//! https://www.w3.org/TR/css-box-3/#padding-physical
use super::*;

#[test]
fn negative_or_auto_padding_invalidates_the_declaration() {
    let dom = dom::parse(
        r#"<div id=a style="padding-right: 4px; padding-right: -10px"></div>
        <div id=b style="padding: 3px; padding: 1px -2px"></div>
        <div id=c style="padding-left: 5%; padding-left: auto; padding-top: calc(2px - 1px)"></div>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let divs = dom.elements_named("div").collect::<Vec<_>>();
    assert_eq!(styles.get(&divs[0]).padding.right, Length::Px(4.0));
    assert_eq!(styles.get(&divs[1]).padding.left, Length::Px(3.0));
    assert_eq!(styles.get(&divs[2]).padding.left, Length::Percent(5.0));
    for (property, value, valid) in [
        ("padding-right", "-10px", false),
        ("padding-right", "0", true),
        ("padding", "1px -2px", false),
        ("padding", "1px 2%", true),
        ("padding-top", "auto", false),
    ] {
        assert_eq!(
            supports::supports_declaration_value(property, value),
            valid,
            "{property}: {value}"
        );
    }
}
