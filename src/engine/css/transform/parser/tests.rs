use super::*;

#[test]
fn nested_math_keeps_coordinate_and_font_bases() {
    let parsed = parse_transform("translate(min(50%, 30px), calc(1em + 2px))").unwrap();
    assert_eq!(parsed.resolve(100.0, 80.0, 20.0), (30.0, 22.0));
    assert_eq!(parsed.resolve(40.0, 80.0, 10.0), (20.0, 12.0));
    let matrix =
        parse_transform("matrix(calc(1), sqrt(0), 0, 1, round(12, 10), calc(20))").unwrap();
    assert_eq!(matrix.resolve(100.0, 80.0, 20.0), (10.0, 20.0));
}

#[test]
fn css_tokens_allow_escaped_names_and_comments() {
    let parsed = parse_transform(r"translat\65x(/* coordinate */ min(20px, 30px))").unwrap();
    assert_eq!(parsed.resolve(100.0, 80.0, 16.0), (20.0, 0.0));
    assert!(parse_transform("NONE").unwrap().is_none());
}

#[test]
fn invalid_coordinate_invalidates_the_entire_declaration() {
    for value in [
        "translate(10px, auto)",
        "translate(auto)",
        "translate(10px, junk)",
        "translate(10px 20px)",
        "translate(10px,)",
        "translate(,10px)",
        "translate(10px, calc(1s))",
        "translateX(10px, 20px)",
        "translate(10px",
        "matrix(1, 0, 0, 1, 10%, 0)",
        "matrix(1, 0.000001, 0, 1, 0, 0)",
        "matrix(1,0,0,1,calc(10px),0)",
        "rotate(0deg)",
    ] {
        assert!(parse_transform(value).is_none(), "{value}");
    }
}

#[test]
fn function_count_and_source_length_are_bounded() {
    assert!(parse_transform(&"translateX(1px) ".repeat(64)).is_some());
    assert!(parse_transform(&"translateX(1px) ".repeat(65)).is_none());
    assert!(parse_transform(&format!("translateX(1px/*{}*/)", "x".repeat(16_384))).is_none());
}
