use super::*;

fn environment() -> MediaEnvironment {
    MediaEnvironment::new(800.0, 600.0, 1.0, false)
}

#[test]
fn offset_lists_accept_keywords_and_percentages_but_not_numbers() {
    assert_eq!(offsets("from, 50%, to"), Some(vec![0.0, 0.5, 1.0]));
    assert_eq!(offsets("0%, from, 100%, to"), Some(vec![0.0, 1.0]));
    for value in ["0", "-1%", "101%", "calc(50%)", "50% garbage", "", "from,"] {
        assert!(offsets(value).is_none(), "{value}");
    }
}

#[test]
fn repeated_offsets_merge_only_their_normal_declarations() {
    let definitions = collect(
        "@keyframes fade {from {opacity:0;color:red} 0%,50%{color:blue}
        from {opacity:.2} to {opacity:1!important;color:green}}",
        environment(),
        RuleScope::Document,
    );
    let frames = definitions[0].merged_blocks();
    assert_eq!(frames.len(), 3);
    assert_eq!(
        frames[0].declarations,
        [
            ("opacity".into(), ".2".into()),
            ("color".into(), "blue".into())
        ]
    );
    assert_eq!(frames[2].declarations, [("color".into(), "green".into())]);
}

#[test]
fn grouping_conditions_and_statement_rules_do_not_swallow_keyframes() {
    let definitions = collect(
        "@charset 'utf-8'; @import 'x.css';
        @media (min-width:900px){@keyframes hidden{to{opacity:1}}}
        @media (min-width:700px){@keyframes shown{to{opacity:1}}}
        @supports (display:flex){@layer a {@keyframes layered{to{opacity:1}}}}
        @supports (unknown:no){@keyframes absent{to{opacity:1}}}",
        environment(),
        RuleScope::Document,
    );
    assert_eq!(
        definitions
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>(),
        ["shown", "layered"]
    );
}

#[test]
fn invalid_keyframe_selectors_are_ignored_without_dropping_valid_siblings() {
    let definitions = collect(
        "@keyframes fade {from {opacity:0} 120%{opacity:.9}
        50%,bad {opacity:.3} to {opacity:1}}",
        environment(),
        RuleScope::Document,
    );
    assert_eq!(definitions[0].blocks.len(), 2);
    assert_eq!(definitions[0].blocks[1].offsets, [1.0]);
}

#[test]
fn names_preserve_case_strings_and_escapes() {
    let definitions = collect(
        r#"@keyframes Fade {} @keyframes fade {} @keyframes "/*name*/" {}
        @keyframes f\61 de {} @keyframes none {} @keyframes inherit {}"#,
        environment(),
        RuleScope::Document,
    );
    assert_eq!(
        definitions
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>(),
        ["Fade", "fade", "/*name*/", "fade"]
    );
}

#[test]
fn keyframe_definitions_do_not_become_style_selector_rules() {
    let dom =
        dom::parse("<style>@keyframes fade {to {color:red}} p{color:blue}</style><p>text</p>");
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let target = dom.elements_named("p").next().unwrap();
    assert_eq!(styles.get(&target).color, Color::rgb(0, 0, 255));
    assert_eq!(
        styles.animation_keyframes(&target, "fade").unwrap().name,
        "fade"
    );
}

#[test]
fn last_definition_wins_without_merging_separate_named_rules() {
    let dom = dom::parse(
        "<style>@keyframes fade {from{opacity:0}} @keyframes fade {to{opacity:1}}</style><p></p>",
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let target = dom.elements_named("p").next().unwrap();
    let definition = styles.animation_keyframes(&target, "fade").unwrap();
    assert_eq!(definition.blocks.len(), 1);
    assert_eq!(definition.blocks[0].offsets, [1.0]);
}
