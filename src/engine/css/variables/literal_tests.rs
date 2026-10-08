use super::*;

fn declaration(value: &str) -> Declaration {
    parse_declarations(&format!("width:{value}")).remove(0)
}

#[test]
fn removing_comments_cannot_retokenize_adjacent_literal_components() {
    for (value, expected) in [
        ("25/**/%", "25/**/%"),
        ("1/**/px", "1/**/px"),
        ("bl/**/ock", "bl/**/ock"),
        ("calc/**/(1)", "calc/**/(1)"),
        ("calc(1/**/+/**/2)", "calc(1+/**/2)"),
        ("rgb(1/**/2/**/3)", "rgb(1/**/2/**/3)"),
        ("25%/**/", "25%"),
    ] {
        assert_eq!(
            declaration(value).prepared_literal(),
            Some(expected),
            "{value}"
        );
    }
}

#[test]
fn variable_replacement_keeps_its_tokens_separate_from_neighboring_components() {
    let custom = HashMap::from([("--n".into(), "25".into())]);
    for (value, expected) in [
        ("var(--n)%", "25/**/%"),
        ("var(--n)px", "25/**/px"),
        ("var(--n)var(--n)", "25/**/25"),
        ("var(--missing,25)%", "25/**/%"),
        ("calc(var(--n) * 1px)", "calc(25 * 1px)"),
    ] {
        assert_eq!(
            substitute_variables(value, &custom).as_deref(),
            Some(expected),
            "{value}"
        );
    }
    let dom = dom::parse(
        "<style>div{--n:25;opacity:.7;opacity:var(--n)%;
        width:30px;width:var(--n)px}</style><div></div>",
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let node = dom.elements_named("div").next().unwrap();
    // Invalid at computed-value time uses the unset/initial value, not the
    // earlier declaration: substitution is not ordinary parse-time rejection.
    assert_eq!(styles.get(&node).opacity, 1.0);
    assert_eq!(styles.get(&node).width, Length::Auto);
}

#[test]
fn missing_and_invalid_variables_use_unset_after_the_cascade() {
    for value in ["var(--missing)", "var(--bad)", "var(--cycle)"] {
        let dom = dom::parse(&format!(
            "<style>main{{color:red}}div{{--bad:20px;--cycle:var(--cycle);
             opacity:.7;opacity:{value};color:blue;color:{value}}}</style>
             <main><div></div></main>"
        ));
        let styles = StyleSet::from_dom(&dom, &[], 800.0);
        let parent = dom.elements_named("main").next().unwrap();
        let child = dom.elements_named("div").next().unwrap();
        assert_eq!(styles.get(&child).opacity, 1.0, "{value}");
        assert_eq!(
            styles.get(&child).color,
            styles.get(&parent).color,
            "{value}"
        );
    }
}

#[test]
fn ordinary_functions_and_unused_fallbacks_share_a_bounded_nesting_contract() {
    let deep = format!("{}1{}", "calc(".repeat(128), ")".repeat(128));
    assert!(declaration(&deep).prepared_literal().is_none());
    assert!(substitute_variables(&deep, &HashMap::new()).is_none());
    assert!(!contains_valid_variable_reference(&format!(
        "{}var(--n){}",
        "calc(".repeat(128),
        ")".repeat(128)
    )));
    let custom = HashMap::from([("--n".into(), "1px".into())]);
    assert!(substitute_variables(&format!("var(--n,{deep})"), &custom).is_none());
}

#[test]
fn literal_preparation_matches_uncached_token_serialization_and_reuses_storage() {
    for value in [
        "block",
        "100%",
        "0.5rem",
        "rgb(12, 34, 56)",
        "calc(100% - (2 * 1em))",
        "[a] {b}",
        r#""var(--not-a-reference)""#,
        r#"url("image with spaces.svg")"#,
        r"\62 lock",
        "red /* comment */ blue",
    ] {
        let declaration = declaration(value);
        let expected = substitute_variables(value, &HashMap::new());
        assert_eq!(
            declaration.prepared_literal(),
            expected.as_deref(),
            "{value}"
        );
        let first = declaration.prepared_literal().unwrap();
        assert_eq!(
            first.as_ptr(),
            declaration.prepared_literal().unwrap().as_ptr()
        );
        assert_eq!(declaration.value, value);
    }
}

#[test]
fn variable_functions_are_not_cached_even_with_escapes_or_nested_fallbacks() {
    for value in [
        "var(--width)",
        "VAR(--width, 20px)",
        r"\76 ar(--width, 20px)",
        "calc(1px + var(--width))",
        "min(1em, calc(var(--width) * 2))",
        "var(--absent, var(--width, 20px))",
    ] {
        let declaration = declaration(value);
        assert!(declaration.prepared_literal().is_none(), "{value}");
        assert_eq!(declaration.literal_value.get(), Some(&None));
    }
    let declaration = declaration(r"\76 ar(--width, 20px)");
    for value in ["30px", "60px"] {
        let custom = HashMap::from([("--width".to_string(), value.to_string())]);
        assert_eq!(
            substitute_variables(&declaration.value, &custom).as_deref(),
            Some(value)
        );
    }
}

#[test]
fn large_literal_values_use_the_uncached_path_without_losing_the_value() {
    let value = format!("\"{}\"", "a".repeat(MAX_CACHED_LITERAL_BYTES));
    let declaration = declaration(&value);
    assert!(declaration.prepared_literal().is_none());
    assert_eq!(declaration.literal_value.get(), Some(&None));
    assert_eq!(
        substitute_variables(&declaration.value, &HashMap::new()),
        Some(value)
    );
}

#[test]
fn prepared_values_still_resolve_per_element_and_after_incremental_changes() {
    let dom = dom::parse(
        r#"
        <style>
        .parent { --width:30px; font-size:10px }
        .parent.changed { --width:60px; font-size:20px }
        .literal { width:2em }
        .variable { width:var(--width) }
        </style>
        <main class=parent><i class=literal></i><b class=variable></b></main>
        <section class='parent changed'><i class=literal></i><b class=variable></b></section>
    "#,
    );
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    let widths = |styles: &StyleSet, tag| {
        dom.elements_named(tag)
            .map(|node| {
                let style = styles.get(&node);
                style.width.resolve(800.0, style.font_size).unwrap()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(widths(&styles, "i"), vec![20.0, 40.0]);
    assert_eq!(widths(&styles, "b"), vec![30.0, 60.0]);
    let main = dom.elements_named("main").next().unwrap();
    main.set_attr("class", "parent changed");
    styles.refresh_subtrees(&dom.document, &[main], &[]);
    assert_eq!(widths(&styles, "i"), vec![40.0; 2]);
    assert_eq!(widths(&styles, "b"), vec![60.0; 2]);
    let fresh = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(styles.styles, fresh.styles);
}
