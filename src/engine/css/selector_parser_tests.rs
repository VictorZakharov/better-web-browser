use super::*;

#[test]
fn fuzz_regression_non_ascii_attribute_suffix_does_not_panic() {
    let attribute = parse_attribute_selector("data-value=\"x\"�").unwrap();

    assert_eq!(attribute.name, "data-value");
    assert!(!attribute.case_insensitive);
}

#[test]
fn attribute_modifiers_accept_css_whitespace_without_byte_slicing() {
    assert!(
        parse_attribute_selector("data-value=\"x\"\tI")
            .unwrap()
            .case_insensitive
    );
    assert!(
        !parse_attribute_selector("data-value=\"x\"\nS")
            .unwrap()
            .case_insensitive
    );
}

#[test]
fn generated_pseudo_elements_use_the_originating_selector() {
    let (selector, pseudo) = parse_style_rule_selector(".card:before").unwrap();
    assert_eq!(pseudo, Some(PseudoElement::Before));
    assert_eq!(selector.specificity.classes, 1);
    assert_eq!(selector.specificity.tags, 1);

    let (selector, pseudo) = parse_style_rule_selector("#footer::AFTER").unwrap();
    assert_eq!(pseudo, Some(PseudoElement::After));
    assert_eq!(selector.specificity.ids, 1);
    assert_eq!(selector.specificity.tags, 1);
    assert!(parse_style_rule_selector(".card::marker").is_none());
}
