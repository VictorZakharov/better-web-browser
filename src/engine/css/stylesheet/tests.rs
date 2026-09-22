use super::*;

#[test]
fn a_large_earlier_stylesheet_does_not_starve_later_cascade_rules() {
    let first = ".early{color:red}".repeat(20_000);
    let mut rules = Vec::new();
    let mut order = 0;
    let environment = MediaEnvironment::new(1280.0, 720.0, 1.0, false);

    parse_stylesheet(
        &first,
        "https://example.test/early.css",
        environment,
        &mut order,
        &mut rules,
        RuleScope::Document,
    );
    parse_stylesheet(
        ".late{display:block}",
        "https://example.test/late.css",
        environment,
        &mut order,
        &mut rules,
        RuleScope::Document,
    );

    assert_eq!(rules.len(), 20_001);
    assert_eq!(rules.last().unwrap().order, 20_000);
}

#[test]
fn semicolon_at_rules_do_not_consume_the_following_qualified_rule() {
    let mut rules = Vec::new();
    let mut order = 0;
    let environment = MediaEnvironment::new(1280.0, 720.0, 1.0, false);

    parse_stylesheet(
        r#"@charset "UTF-8";@import url("theme.css");
           .player{position:relative;width:100%;height:100%}"#,
        "https://example.test/player.css",
        environment,
        &mut order,
        &mut rules,
        RuleScope::Document,
    );

    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].selector.compounds[0].classes, ["player"]);
    assert_eq!(
        rules[0]
            .declarations
            .iter()
            .map(|declaration| (declaration.name.as_str(), declaration.value.as_str()))
            .collect::<Vec<_>>(),
        [
            ("position", "relative"),
            ("width", "100%"),
            ("height", "100%")
        ]
    );
}

#[test]
fn an_empty_selector_list_member_invalidates_the_complete_style_rule() {
    let mut rules = Vec::new();
    let mut order = 0;
    let environment = MediaEnvironment::new(1280.0, 720.0, 1.0, false);

    parse_stylesheet(
        ".valid{color:green} .trailing,{color:red} ,.leading{color:red} .middle,,span{color:red}",
        "https://example.test/styles.css",
        environment,
        &mut order,
        &mut rules,
        RuleScope::Document,
    );

    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].selector.compounds[0].classes, ["valid"]);
    assert_eq!(rules[0].order, 0);
}
