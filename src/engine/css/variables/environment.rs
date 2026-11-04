//! The `env()` environment variables the user agent defines.
//!
//! `env()` substitutes like `var()`, so the shared component walk resolves both. Windows desktop
//! windows have no display cutouts or rounded corners that obscure content, so every safe-area
//! inset is zero, matching desktop Chromium. Any other name is not recognized, which selects the
//! fallback.
//! https://drafts.csswg.org/css-env-1/#safe-area-insets

use super::*;

pub(super) fn is_environment_function(name: &str) -> bool {
    name.eq_ignore_ascii_case("env")
}

/// Consumes `<custom-ident> <integer [0,∞]>*` and returns the variable's value, if recognized.
/// Names are case-sensitive, and a variable given the wrong number of indices is not recognized.
/// https://drafts.csswg.org/css-env-1/#env-function
pub(super) fn parse_reference<'i>(
    parser: &mut Parser<'i, '_>,
) -> Result<Option<&'static str>, cssparser::ParseError<'i, ()>> {
    let name = parser.expect_ident_cloned()?;
    let mut indices = 0_usize;
    while parser
        .try_parse(|input| match input.expect_integer() {
            Ok(index) if index >= 0 => Ok(()),
            Ok(_) => Err(input.new_custom_error::<(), ()>(())),
            Err(error) => Err(error.into()),
        })
        .is_ok()
    {
        indices += 1;
    }
    parser.skip_whitespace();
    Ok((indices == 0).then(|| scalar_value(&name)).flatten())
}

fn scalar_value(name: &str) -> Option<&'static str> {
    matches!(
        name,
        "safe-area-inset-top"
            | "safe-area-inset-right"
            | "safe-area-inset-bottom"
            | "safe-area-inset-left"
            | "safe-area-max-inset-top"
            | "safe-area-max-inset-right"
            | "safe-area-max-inset-bottom"
            | "safe-area-max-inset-left"
    )
    .then_some("0px")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn substitute(value: &str) -> Option<String> {
        substitute_variables(value, &HashMap::from([("--gap".into(), "4px".into())]))
    }

    #[test]
    fn safe_area_insets_are_zero_on_a_desktop_window_and_ignore_the_fallback() {
        for (value, expected) in [
            ("env(safe-area-inset-top)", "0px"),
            ("env(safe-area-inset-left, 9px)", "0px"),
            ("env(safe-area-max-inset-bottom)", "0px"),
            ("ENV(safe-area-inset-right)", "0px"),
            ("calc(1px + env(safe-area-inset-top, 9px))", "calc(1px + 0px)"),
        ] {
            assert_eq!(substitute(value).as_deref(), Some(expected), "{value}");
        }
    }

    #[test]
    fn an_unrecognized_variable_uses_its_fallback_or_is_invalid() {
        for (value, expected) in [
            ("env(unknown, 3px)", Some("3px")),
            ("env(unknown, var(--gap))", Some("4px")),
            ("env(unknown, env(safe-area-inset-top))", Some("0px")),
            ("env(unknown,)", Some("")),
            // Names are case-sensitive, unlike the function's own name.
            ("env(SAFE-AREA-INSET-TOP, 3px)", Some("3px")),
            // A scalar variable given an index does not match it.
            ("env(safe-area-inset-top 0, 3px)", Some("3px")),
            ("env(viewport-segment-width 0 0, 3px)", Some("3px")),
            ("env(unknown)", None),
            ("env(safe-area-inset-top 0)", None),
            ("env(unknown, var(--missing))", None),
        ] {
            // A fallback keeps its source whitespace, as var() does.
            assert_eq!(
                substitute(value).as_deref().map(str::trim),
                expected,
                "{value}"
            );
        }
    }

    #[test]
    fn a_malformed_reference_is_invalid_at_parse_time_only_when_its_syntax_is_wrong() {
        for value in [
            "env(safe-area-inset-top)",
            "env(unknown)",
            "env(viewport-segment-width 0 1, 2px)",
            "env(unknown,)",
            "calc(1px + env(unknown, 2px))",
        ] {
            assert!(contains_valid_variable_reference(value), "{value}");
        }
        for value in [
            "env()",
            "env(1px)",
            "env(name -1)",
            "env(name 1.5)",
            "env(name junk)",
            "env(\"name\")",
        ] {
            assert!(!contains_valid_variable_reference(value), "{value}");
        }
    }

    #[test]
    fn replacements_keep_their_tokens_separate_from_neighboring_components() {
        assert_eq!(
            substitute("env(unknown,25)%").as_deref(),
            Some("25/**/%")
        );
    }

    fn styled_div(css: &str) -> ComputedStyle {
        let dom = dom::parse(&format!("<style>{css}</style><div></div>"));
        let styles = StyleSet::from_dom(&dom, &[], 800.0);
        let node = dom.elements_named("div").next().unwrap();
        styles.get(&node).clone()
    }

    #[test]
    fn calc_with_an_inset_keeps_a_positioned_box_on_the_side_it_names() {
        // The hero screen of a game anchors its panel with exactly this shape; an unresolved
        // env() there made the whole declaration invalid and moved the panel to the far side.
        let style = styled_div("div{--gut:22px;position:absolute;right:calc(var(--gut) + env(safe-area-inset-right, 0px))}");
        assert_eq!(
            styled_div("div{position:absolute;right:22px}").right,
            style.right
        );
    }

    #[test]
    fn an_invalid_reference_is_invalid_at_computed_value_time() {
        // The later declaration wins the cascade, then fails: it cannot resurrect the earlier one.
        let style = styled_div("div{opacity:.5;opacity:env(unknown)}");
        assert_eq!(style.opacity, 1.0);
    }

    #[test]
    fn a_custom_property_can_carry_a_reference() {
        let style = styled_div("div{--inset:env(unknown, .25);opacity:var(--inset)}");
        assert_eq!(style.opacity, 0.25);
    }

    #[test]
    fn feature_queries_accept_a_syntactically_valid_reference() {
        assert!(supports::supports_declaration_value(
            "padding-top",
            "env(safe-area-inset-top, 0px)"
        ));
    }
}
