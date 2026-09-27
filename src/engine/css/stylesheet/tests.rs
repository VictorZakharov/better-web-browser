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

#[test]
fn part_selectors_parse_host_origin_and_intersection_without_leaking_invalid_forms() {
    let mut rules = Vec::new();
    parse_stylesheet(
        "x-card::part(label active){color:red}\
         ::PART(label){color:blue}\
         x-card::part(){color:black}\
         x-card::part(label)::part(child){color:black}\
         x-card::part(label)::before{color:black}\
         x-card::part(label#bad){color:black}\
         x-card::part(a\\+b){color:orange}\
         [data-label='::part(fake)']{color:green}",
        "",
        MediaEnvironment::new(800.0, 600.0, 1.0, false),
        &mut 0,
        &mut rules,
        RuleScope::Document,
    );
    assert_eq!(rules.len(), 4);
    let first = rules[0].part.as_ref().unwrap();
    assert_eq!(first.names, ["label", "active"]);
    assert_eq!(rules[0].selector.specificity.tags, 2);
    assert_eq!(rules[0].selector.specificity.classes, 0);
    assert_eq!(rules[0].selector.compounds[0].attributes[0].name, "part");
    assert_eq!(rules[1].part.as_ref().unwrap().names, ["label"]);
    assert_eq!(rules[1].selector.specificity.tags, 1);
    assert_eq!(rules[2].part.as_ref().unwrap().names, ["a+b"]);
    assert!(rules[3].part.is_none());
}

#[test]
fn host_rules_keep_pseudo_class_and_argument_specificity() {
    let mut rules = Vec::new();
    let mut order = 0;
    let root = Node::create_document().id();
    parse_stylesheet(
        ":host{color:red}:host(.active){color:blue}\
         :host(.active)>span{color:green}:host(.active) .leaf{color:orange}\
         :host(.outer > x-card){color:black}:host(.active).other{color:black}",
        "https://example.test/component.css",
        MediaEnvironment::new(1280.0, 720.0, 1.0, false),
        &mut order,
        &mut rules,
        RuleScope::Shadow(root),
    );
    assert_eq!(rules.len(), 4);
    assert_eq!(rules[0].scope, RuleScope::Host(root));
    assert_eq!(rules[0].selector.specificity.classes, 1);
    assert_eq!(rules[1].scope, RuleScope::Host(root));
    assert_eq!(rules[1].selector.specificity.classes, 2);
    assert_eq!(rules[2].scope, RuleScope::HostChild(root));
    assert_eq!(rules[2].selector.specificity.classes, 2);
    assert_eq!(rules[2].selector.specificity.tags, 1);
    assert_eq!(rules[3].scope, RuleScope::Shadow(root));
    assert_eq!(rules[3].selector.specificity.classes, 3);
}

#[test]
fn slotted_rules_require_a_compound_argument_and_count_the_originating_slot() {
    let mut rules = Vec::new();
    let mut order = 0;
    let root = Node::create_document().id();
    parse_stylesheet(
        "::slotted(.hot){color:red}\
         section > slot.primary::slotted(span.hot[data-state=on]){color:blue}\
         ::SLOTTED(#chosen){color:green}\
         ::slotted(.outer .inner){color:black}\
         ::slotted(){color:black}\
         slot::slotted(.hot)::before{color:black}\
         [data-label='::slotted(.hot)']{color:orange}",
        "https://example.test/component.css",
        MediaEnvironment::new(1280.0, 720.0, 1.0, false),
        &mut order,
        &mut rules,
        RuleScope::Shadow(root),
    );
    assert_eq!(rules.len(), 4);
    assert!(
        rules[..3]
            .iter()
            .all(|rule| rule.scope == RuleScope::Slotted(root))
    );
    assert_eq!(
        rules[0].selector.specificity,
        Specificity {
            ids: 0,
            classes: 1,
            tags: 1
        }
    );
    assert_eq!(
        rules[1].selector.specificity,
        Specificity {
            ids: 0,
            classes: 3,
            tags: 4
        }
    );
    assert_eq!(
        rules[2].selector.specificity,
        Specificity {
            ids: 1,
            classes: 0,
            tags: 1
        }
    );
    assert_eq!(rules[3].scope, RuleScope::Shadow(root));
}

#[test]
fn host_context_requires_a_compound_and_adds_pseudo_class_specificity() {
    let mut rules = Vec::new();
    let mut order = 0;
    let root = Node::create_document().id();
    parse_stylesheet(
        ":host-context(.theme){color:red}\
         :HOST-CONTEXT(#theme)>span{color:blue}\
         :host-context(.outer > .theme){color:black}\
         :host-context(){color:black}",
        "https://example.test/component.css",
        MediaEnvironment::new(800.0, 600.0, 1.0, false),
        &mut order,
        &mut rules,
        RuleScope::Shadow(root),
    );
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[0].scope, RuleScope::Host(root));
    assert!(rules[0].host_context);
    assert_eq!(
        rules[0].selector.specificity,
        Specificity {
            ids: 0,
            classes: 2,
            tags: 0
        }
    );
    assert_eq!(rules[1].scope, RuleScope::HostChild(root));
    assert!(rules[1].host_context);
    assert_eq!(
        rules[1].selector.specificity,
        Specificity {
            ids: 1,
            classes: 1,
            tags: 1
        }
    );
}
