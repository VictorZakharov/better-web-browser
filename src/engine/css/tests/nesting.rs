use super::*;

fn color_of(dom: &dom::Dom, id: &str) -> Color {
    let styles = StyleSet::from_dom(dom, &[], 1000.0);
    let node = dom
        .elements_named("span")
        .chain(dom.elements_named("div"))
        .find(|node| node.attr("id").as_deref() == Some(id))
        .unwrap();
    styles.get(&node).color
}

#[test]
fn nested_descendant_and_explicit_child_rules_match() {
    let dom = dom::parse(
        "<style>.card { color: red; .title { color: blue } > .direct { color: green } }</style>
         <div class=card><span id=title class=title></span><div><span id=deep class=direct></span></div>
         <span id=direct class=direct></span></div>",
    );
    assert_eq!(color_of(&dom, "title"), Color::rgb(0, 0, 255));
    assert_eq!(color_of(&dom, "direct"), Color::rgb(0, 128, 0));
    assert_eq!(color_of(&dom, "deep"), Color::rgb(255, 0, 0));
}

#[test]
fn nesting_selector_matches_same_element_and_preserves_parent_list_specificity() {
    let dom = dom::parse(
        "<style>#unused, .card { &.active { color: blue } .title { color: blue } }
         .card.active, .card .title { color: red }</style>
         <div id=card class='card active'><span id=title class=title></span></div>",
    );
    assert_eq!(color_of(&dom, "card"), Color::rgb(0, 0, 255));
    assert_eq!(color_of(&dom, "title"), Color::rgb(0, 0, 255));
}

#[test]
fn nesting_specificity_includes_pseudo_element_members_without_matching_them() {
    let dom = dom::parse(
        "<style>.card, #unused::before { & .title { color: blue } }
         .card .title { color: red }</style>
         <div class=card><span id=title class=title></span></div>",
    );
    assert_eq!(color_of(&dom, "title"), Color::rgb(0, 0, 255));
}

#[test]
fn nesting_selector_inside_functional_pseudo_keeps_parent_specificity() {
    let dom = dom::parse(
        "<style>
         #unused, .card {
             :is(&, .other) { color: blue }
             :where(&) > .label { color: green }
         }
         .card { color: red }
         .card > .label { color: red }
         </style>
         <div id=card class=card><span id=label class=label></span></div>",
    );
    assert_eq!(color_of(&dom, "card"), Color::rgb(0, 0, 255));
    assert_eq!(color_of(&dom, "label"), Color::rgb(255, 0, 0));
}

#[test]
fn interleaved_declarations_keep_source_order() {
    let dom = dom::parse(
        "<style>.card { color: red; & { color: blue } color: green }</style>
         <div id=card class=card></div>",
    );
    assert_eq!(color_of(&dom, "card"), Color::rgb(0, 128, 0));
}

#[test]
fn nested_media_and_layer_rules_keep_parent_matching() {
    let dom = dom::parse(
        "<style>@layer base, accent;
         .card { @layer base { color: red } @layer accent { color: blue }
                 @media (min-width: 500px) { > .title { color: green } } }</style>
         <div id=card class=card><span id=title class=title></span></div>",
    );
    assert_eq!(color_of(&dom, "card"), Color::rgb(0, 0, 255));
    assert_eq!(color_of(&dom, "title"), Color::rgb(0, 128, 0));
}

#[test]
fn at_rule_names_are_ascii_case_insensitive_and_not_prefix_matches() {
    let dom = dom::parse(
        "<style>
         @MEDIA (min-width: 500px) { .card { color: red } }
         .card {
             @SUPPORTS (display: block) { color: blue }
             @MEDIA (min-width: 500px) { > .title { color: green } }
             @media-custom (min-width: 500px) { color: purple }
         }
         </style><div id=card class=card><span id=title class=title></span></div>",
    );
    assert_eq!(color_of(&dom, "card"), Color::rgb(0, 0, 255));
    assert_eq!(color_of(&dom, "title"), Color::rgb(0, 128, 0));
}

#[test]
fn custom_property_brace_values_do_not_become_nested_rules() {
    let mut rules = Vec::new();
    let mut order = 0;
    stylesheet::parse_stylesheet(
        ".card { --tokens: {accent: red; main: blue}; color: green; &.active { color: blue } }",
        "https://example.test/style.css",
        media::MediaEnvironment::new(1000.0, 800.0, 1.0, false),
        &mut order,
        &mut rules,
        RuleScope::Document,
    );
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[0].declarations[0].name, "--tokens");
    assert_eq!(rules[0].declarations[0].value, "{accent: red; main: blue}");
    assert_eq!(rules[0].declarations[1].name, "color");
}
