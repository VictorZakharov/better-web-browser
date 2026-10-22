use super::*;
use crate::engine::css::selector_parser::parse_selector;
use crate::engine::dom::{self, Node};

fn reach(selectors: &[&str]) -> PreviousSubtrees {
    let mut reach = PreviousSubtrees::default();
    for selector in selectors {
        reach.include(&parse_selector(selector).expect(selector));
    }
    reach
}

#[test]
fn conditions_on_the_subject_or_its_earlier_siblings_reach_only_the_element() {
    let dom = dom::parse("<div class=a><li><b></b></li></div>");
    let reach = reach(&[
        "li:last-child",
        ".a:only-child + p",
        "p:not(:last-of-type)",
        ":is(.x :last-child)",
        "ul > li:last-child",
    ]);
    for node in Node::descendants(&dom.document).filter(|node| node.element().is_some()) {
        assert!(!reach.reaches(&node), "{:?}", node.tag_name());
    }
}

#[test]
fn conditions_left_of_a_descendant_reach_below_elements_matching_their_keys() {
    let dom = dom::parse("<li class=x></li><li></li><div class=a></div><div></div>");
    let [li_x, li, div_a, div] = ["li", "li", "div", "div"]
        .iter()
        .zip(0..)
        .map(|(tag, index)| dom.elements_named(tag).nth(index % 2).unwrap())
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    let reach = reach(&[
        "li.x:last-child b",
        ":is(div.a:last-of-type .y)",
        "section > .z:only-child ~ p span",
    ]);
    assert!(reach.reaches(&li_x));
    assert!(!reach.reaches(&li));
    assert!(reach.reaches(&div_a));
    assert!(!reach.reaches(&div));
}

#[test]
fn unkeyed_conditions_reach_below_every_element() {
    let dom = dom::parse("<div></div>");
    let div = dom.elements_named("div").next().unwrap();
    for selector in [":last-child .x", "li:is(:last-child .a) b"] {
        assert!(reach(&[selector]).reaches(&div), "{selector}");
    }
}
