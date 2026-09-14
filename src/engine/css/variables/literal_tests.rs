use super::*;

fn declaration(value: &str) -> Declaration {
    parse_declarations(&format!("width:{value}")).remove(0)
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
