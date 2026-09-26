use super::*;
use crate::engine::css::{Display, StyleSet};
use crate::engine::dom;

fn environment(width: f32, height: f32, dppx: f32, dark: bool) -> MediaEnvironment {
    MediaEnvironment::new(width, height, dppx, dark)
}

fn desktop() -> MediaEnvironment {
    environment(800.0, 600.0, 1.5, false)
}

#[test]
fn empty_list_and_media_types() {
    let env = desktop();
    assert!(media_matches_for_environment("", env));
    assert!(media_matches_for_environment("@media", env));
    assert!(media_query_matches("/* no queries */", env));
    assert!(media_query_matches("screen", env));
    assert!(media_query_matches("all", env));
    assert!(!media_query_matches("print", env));
    assert!(!media_query_matches("speech", env));
    assert!(media_query_matches("not print", env));
    assert!(!media_query_matches("not screen", env));
    assert!(!media_query_matches("only print", env));
    assert!(media_query_matches("only screen", env));
    assert_eq!(serialize_media_query_list("only screen"), "screen");
}

#[test]
fn parses_media_lists_with_independent_error_recovery() {
    let env = desktop();
    assert!(media_query_matches("print, screen", env));
    assert!(media_query_matches("screen and, (min-width: 700px)", env));
    assert_eq!(
        serialize_media_query_list("screen and, (min-width:700px)"),
        "not all, (min-width: 700px)"
    );
    assert_eq!(serialize_media_query_list("::"), "not all");
    assert_eq!(serialize_media_query_list("screen,"), "screen, not all");
    assert_eq!(serialize_media_query_list(",screen"), "not all, screen");
    // An unclosed final block consumes subsequent commas, not earlier queries.
    assert!(media_query_matches("screen, (", env));
    assert_eq!(serialize_media_query_list("screen, ("), "screen, not all");
    assert!(media_query_matches("screen, 'unterminated", env));
    assert_eq!(
        serialize_media_query_list("screen, 'unterminated"),
        "screen, not all"
    );
    assert!(!media_query_matches("screen and (", env));
    assert_eq!(
        serialize_media_query_list("not (future-feature: 1), screen"),
        "not (future-feature: 1), screen"
    );
    assert_eq!(
        serialize_media_query_list("screen and (min-orientation: portrait)"),
        "screen and (min-orientation: portrait)"
    );
    assert_eq!(
        serialize_media_query_list("not all and (width: 800px)"),
        "not all and (width: 800px)"
    );
    assert!(!media_query_matches("screen and", env));
    assert!(!media_query_matches("or and (width)", env));
    assert!(!media_query_matches("only (width)", env));
    assert!(!media_query_matches("screen extra", env));
    assert!(!media_query_matches("screen and (width) garbage", env));
    assert!(!media_query_matches("screen and (width) or (height)", env));
}

#[test]
fn nested_conditions_preserve_grouping_and_three_valued_logic() {
    let env = desktop();
    assert!(media_query_matches(
        "(width >= 800px) and ((orientation: portrait) or (hover: hover))",
        env
    ));
    assert!(media_query_matches(
        "(width < 200px) or (height >= 600px)",
        env
    ));
    assert!(media_query_matches(
        "(width < 200px) or (future-feature: 1) or (height: 600px)",
        env
    ));
    assert!(!media_query_matches(
        "(width < 200px) or (future-feature: 1)",
        env
    ));
    assert!(!media_query_matches("not (future-feature: 1)", env));
    assert!(!media_query_matches("not future-feature(yes)", env));
    assert!(!media_query_matches(
        "(future-feature: 1) and (width < 200px)",
        env
    ));
    assert!(!media_query_matches("(width) and (hover) or (height)", env));
    assert!(!media_query_matches("(width) and not (hover)", env));
    assert!(media_query_matches("not screen and (width < 200px)", env));
    assert!(!media_query_matches("not screen and (width >= 200px)", env));
    for query in [
        "(width >= 1px) or (future-feature)",
        "(width < 1px) and (future-feature)",
        "not (future-feature)",
        "not all and (width: 800px)",
    ] {
        let serialized = serialize_media_query_list(query);
        assert_eq!(
            media_query_matches(query, env),
            media_query_matches(&serialized, env),
            "{query} serialized as {serialized}"
        );
    }
}

#[test]
fn supports_direct_reverse_and_chained_range_syntax() {
    let env = desktop();
    for query in [
        "(width >= 800px)",
        "(800px <= width)",
        "(600px < width <= 800px)",
        "(1000px > width >= 800px)",
        "(height > 500px)",
        "(resolution >= 144dpi)",
        "(aspect-ratio = 4/3)",
    ] {
        assert!(media_query_matches(query, env), "{query}");
    }
    for query in [
        "(width > 800px)",
        "(800px > width)",
        "(600px < width < 800px)",
        "(1000px < width < 1200px)",
        "(resolution < 1.5dppx)",
        "(aspect-ratio > 2/1)",
    ] {
        assert!(!media_query_matches(query, env), "{query}");
    }
    assert_eq!(
        serialize_media_query_list("(600px<width<=800px)"),
        "(600px < width <= 800px)"
    );
    assert_eq!(
        serialize_media_query_list("(aspect-ratio: 1/3)"),
        "(aspect-ratio: 1 / 3)"
    );
    assert_eq!(
        serialize_media_query_list("(min-aspect-ratio: 4/3)"),
        "(min-aspect-ratio: 4 / 3)"
    );
    assert_eq!(
        serialize_media_query_list("(1/3 < aspect-ratio <= 4/3)"),
        "(1 / 3 < aspect-ratio <= 4 / 3)"
    );
}

#[test]
fn feature_values_use_initial_font_and_real_viewport_axes() {
    let env = desktop();
    assert!(media_query_matches("(min-width: 8in)", env));
    assert!(media_query_matches("(min-width: 40em)", env));
    assert!(media_query_matches("(min-width: 100vh)", env));
    assert!(media_query_matches("(min-height: 50vw)", env));
    assert!(media_query_matches("(min-width: 100vmin)", env));
    assert!(media_query_matches("(max-width: 100vmax)", env));
    assert!(media_query_matches("(min-width: calc(50vw + 10px))", env));
    assert!(!media_query_matches("(min-width: 101vw)", env));
    assert!(!media_query_matches("(width: 100%)", env));
    assert!(!media_query_matches("(width: auto)", env));
    assert!(!media_query_matches("(width: 10px !important)", env));
    // Whitespace between a number and unit creates separate CSS tokens, not a dimension.
    assert!(!media_query_matches("not (width: 5 px)", env));
    assert!(!media_query_matches("not (width: 5 cm)", env));
    assert!(!media_query_matches("not (resolution: 1 dpi)", env));
}

#[test]
fn known_discrete_values_and_unknown_values_are_distinct() {
    let env = desktop();
    assert!(media_query_matches("(orientation: landscape)", env));
    assert!(!media_query_matches("(orientation: portrait)", env));
    let square = environment(800.0, 800.0, 1.0, false);
    assert!(media_query_matches("(orientation: portrait)", square));
    assert!(!media_query_matches("(orientation: landscape)", square));
    assert!(media_query_matches("(hover: hover)", env));
    assert!(!media_query_matches("(pointer: coarse)", env));
    assert!(media_query_matches("(color: 8)", env));
    assert!(!media_query_matches("(monochrome)", env));
    assert!(media_query_matches("(prefers-color-scheme: light)", env));
    assert!(!media_query_matches("(prefers-color-scheme: dark)", env));
    assert!(!media_query_matches("not (orientation: diagonal)", env));
    assert!(!media_query_matches("not (min-orientation: portrait)", env));
    assert!(!media_query_matches("not (width: 1zq)", env));
    assert!(!media_query_matches("not (future-feature: foo)", env));
}

#[test]
fn tokens_keep_comments_escapes_and_nested_commas_with_their_query() {
    let env = desktop();
    assert!(media_query_matches("S\\43REEN/**/and (width: 800px)", env));
    assert!(media_query_matches("screen and (width/**/:/**/800px)", env));
    assert!(!media_query_matches("not future-feature(red, blue)", env));
    assert!(!media_query_matches("screen and (width: 800px", env));
    assert!(!media_query_matches("screen and (width: 800px) )", env));
    assert_eq!(
        serialize_media_query_list("screen/**/and (width:800px)"),
        "screen and (width: 800px)"
    );
}

#[test]
fn serialization_preserves_unknown_token_stream_case_and_quoted_content() {
    assert_eq!(
        serialize_media_query_list("(Future: \"Case  Sensitive/*literal*/\")"),
        "(Future: \"Case  Sensitive/*literal*/\")"
    );
    assert_eq!(
        serialize_media_query_list("FutureFunction(Var(--BrandColor), \"MiXeD\")"),
        "FutureFunction(Var(--BrandColor), \"MiXeD\")"
    );
    assert_eq!(
        serialize_media_query_list("(orientation: UNKNOWN)"),
        "(orientation: UNKNOWN)"
    );
    assert_eq!(
        serialize_media_query_list("(MIN-WIDTH: 1PX), (WIDTH >= 1PX)"),
        "(min-width: 1px), (width >= 1px)"
    );
}

#[test]
fn negative_range_operands_remain_valid_under_negation() {
    let env = desktop();
    // MQ4 §2.4.3 requires resolution, like width/color, to be false in the
    // negative range. Chrome currently differs for negative resolution and
    // treats it as unknown, but following Chrome there would break `not`.
    for query in [
        "not (width: -1px)",
        "not (width < -1px)",
        "not (color: -1)",
        "not (resolution: -1dpi)",
        "(min-width: -1px)",
        "(min-color: -1)",
        "(min-resolution: -1dpi)",
    ] {
        assert!(media_query_matches(query, env), "{query}");
    }
}

#[test]
fn stylesheet_media_rules_share_the_query_engine() {
    let document = dom::parse(
        "<style>.target{display:none}\
         @media screen and (600px < width <= 800px){.target{display:block}}\
         @media (width > 1000px){.target{display:none}}</style><div class=target></div>",
    );
    let styles = StyleSet::from_sources_for_media_environment(&document, "", &[], desktop());
    let target = document.elements_named("div").next().unwrap();
    assert_eq!(styles.get(&target).display, Display::Block);
}

#[test]
fn media_environment_changes_update_query_results() {
    let small = environment(500.0, 700.0, 1.0, false);
    let large = environment(900.0, 700.0, 2.0, true);
    assert!(!media_query_matches("(width >= 800px)", small));
    assert!(media_query_matches("(width >= 800px)", large));
    assert!(!media_query_matches("(resolution >= 2dppx)", small));
    assert!(media_query_matches("(resolution >= 2dppx)", large));
    assert!(media_query_matches("(prefers-color-scheme: light)", small));
    assert!(media_query_matches("(prefers-color-scheme: dark)", large));
}
