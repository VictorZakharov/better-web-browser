use super::*;

#[test]
fn matches_adjacent_and_general_element_siblings() {
    let dom = dom::parse("<i></i>text<b id='one'></b><b id='two'></b>");
    let two = dom
        .find_node(dom.elements_named("b").nth(1).unwrap().id())
        .unwrap();
    assert!(selector_matches(&parse_selector("b + b").unwrap(), &two));
    assert!(selector_matches(&parse_selector("i ~ b").unwrap(), &two));
    assert!(!selector_matches(&parse_selector("i + b").unwrap(), &two));
}

#[test]
fn matches_enabled_and_disabled_form_controls() {
    let dom = dom::parse("<button id=on></button><button id=off disabled></button>");
    let buttons = dom.elements_named("button").collect::<Vec<_>>();
    assert!(selector_matches(
        &parse_selector("button:enabled").unwrap(),
        &buttons[0]
    ));
    assert!(selector_matches(
        &parse_selector("button:disabled").unwrap(),
        &buttons[1]
    ));
}

#[test]
fn matches_first_element_of_the_same_expanded_type() {
    let dom = dom::parse("<i></i>text<b id='first'></b><em></em><b id='second'></b>");
    let nodes = dom.elements_named("b").collect::<Vec<_>>();
    let selector = parse_selector("b:first-of-type").unwrap();

    assert!(selector_matches(&selector, &nodes[0]));
    assert!(!selector_matches(&selector, &nodes[1]));
}

#[test]
fn functional_pseudo_classes_match_attribute_selectors() {
    let dom =
        dom::parse("<main is-two-columns_ force-default-style></main><main id='single'></main>");
    let nodes = dom.elements_named("main").collect::<Vec<_>>();

    assert!(!selector_matches(
        &parse_selector("main:not([is-two-columns_])").unwrap(),
        &nodes[0]
    ));
    assert!(selector_matches(
        &parse_selector("main:not([is-two-columns_])").unwrap(),
        &nodes[1]
    ));
    assert!(selector_matches(
        &parse_selector("main:is([force-default-style], .fallback)").unwrap(),
        &nodes[0]
    ));
}
