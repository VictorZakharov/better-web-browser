use super::*;

#[test]
fn float_grammar_matches_the_platform_subset() {
    for valid in [
        "12",
        "-1.5",
        "+.5",
        "5.",
        "1e3",
        "1E-3",
        "  12  ",
        "\t-0.25\n",
    ] {
        assert!(is_valid_float(valid), "{valid}");
    }
    for invalid in [
        "",
        "   ",
        "abc",
        "inf",
        "-Infinity",
        "NaN",
        "0x1",
        "1.2.3",
        "e5",
        "1e",
        "+",
        ".",
        "1,000",
        "12px",
        "--1",
    ] {
        assert!(!is_valid_float(invalid), "{invalid}");
    }
    assert_eq!(parse_float_value("0.1"), Some(0.1));
    assert_eq!(parse_float_value("1e308"), Some(1e308));
    assert_eq!(parse_float_value("1e309"), None);
}

#[test]
fn number_serialization_uses_shortest_plain_or_exponent() {
    for (value, expected) in [
        (0.0, "0"),
        (-0.0, "0"),
        (50.0, "50"),
        (0.3, "0.3"),
        (-2.5, "-2.5"),
        (100.0, "100"),
        (0.000001, "0.000001"),
        (0.0000001, "1e-7"),
        (1e21, "1e+21"),
        (1e22, "1e+22"),
        (1e20, "100000000000000000000"),
    ] {
        assert_eq!(number_to_string(value), expected, "{value}");
    }
}

#[test]
fn non_negative_integer_parser_consumes_only_the_html_decimal_prefix() {
    for (text, expected) in [
        ("0", 0),
        ("-0tail", 0),
        ("+3", 3),
        ("\t\n\r\x0C +12suffix", 12),
        ("2e2", 2),
        ("3.5", 3),
        ("0x10", 0),
        ("18446744073709551615", u64::MAX),
    ] {
        assert_eq!(parse_non_negative(text), Some(expected), "{text}");
    }
    for text in [
        "",
        " ",
        "+",
        "-",
        "--1",
        "+-1",
        "-1",
        "-3suffix",
        ".5",
        "\u{a0}3",
        "18446744073709551616",
        "99999999999999999999999999999999999999999999999999",
    ] {
        assert_eq!(parse_non_negative(text), None, "{text}");
    }
}

#[test]
fn native_length_validation_uses_html_prefixes_only_after_user_edits() {
    let dom = crate::engine::dom::parse(
        "<input minlength=' +5suffix'><textarea maxlength='3.5'></textarea>",
    );
    let input = dom.elements_named("input").next().unwrap();
    let textarea = dom.elements_named("textarea").next().unwrap();
    assert!(input.user_edit_input("four"));
    assert!(textarea.set_textarea_raw("four", true));
    let patterns = control_validity::PatternSource::Cached;
    assert!(control_validity::validity_of(&input, &patterns).too_short);
    assert!(control_validity::validity_of(&textarea, &patterns).too_long);

    input.set_attr("minlength", "\u{a0}5");
    textarea.set_attr("maxlength", "-3suffix");
    assert!(!control_validity::validity_of(&input, &patterns).too_short);
    assert!(!control_validity::validity_of(&textarea, &patterns).too_long);

    input.set_attr("minlength", "5suffix");
    textarea.set_attr("maxlength", "+3suffix");
    input.set_input_value("tiny");
    textarea.set_textarea_raw("large", false);
    assert!(!control_validity::validity_of(&input, &patterns).too_short);
    assert!(!control_validity::validity_of(&textarea, &patterns).too_long);
}

#[test]
fn native_select_display_size_uses_the_shared_integer_prefix_parser() {
    let dom = crate::engine::dom::parse("<select multiple size=' +2suffix'></select>");
    let select = dom.elements_named("select").next().unwrap();
    assert_eq!(select.select_display_size(), 2);
    select.set_attr("size", "3.9");
    assert_eq!(select.select_display_size(), 3);
    select.set_attr("size", "\u{a0}2");
    assert_eq!(select.select_display_size(), 4);
}
