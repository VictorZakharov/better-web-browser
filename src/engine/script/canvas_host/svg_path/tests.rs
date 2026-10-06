use super::*;

#[test]
fn parser_preserves_all_svg_commands_without_rounding_or_arc_replacement() {
    let result = parse("M1 2l3 4H5v6C1 2 3 4 5 6s7 8 9 10Q1 2 3 4t5 6A2 3 45 1 0 7 8z").unwrap();
    assert_eq!(
        result
            .iter()
            .map(|(command, _)| *command)
            .collect::<String>(),
        "MlHvCsQtAz"
    );
    assert_eq!(result[5].1, [7.0, 8.0, 9.0, 10.0]);
    assert_eq!(result[8].1, [2.0, 3.0, 45.0, 1.0, 0.0, 7.0, 8.0]);
    assert!(result[9].1.is_empty());
}

#[test]
fn implicit_move_pairs_are_explicit_line_commands_with_original_coordinate_space() {
    assert_eq!(
        parse("m1 2 3 4 5 6").unwrap(),
        [
            ('m', vec![1.0, 2.0]),
            ('l', vec![3.0, 4.0]),
            ('l', vec![5.0, 6.0])
        ]
    );
}

#[test]
fn compact_numbers_flags_and_exponents_reuse_the_svg_provider_grammar() {
    let result = parse("M10-20A5.5.3-4 110-.1L1e2-.5Z").unwrap();
    assert_eq!(result[0], ('M', vec![10.0, -20.0]));
    assert_eq!(result[1], ('A', vec![5.5, 0.3, -4.0, 1.0, 1.0, 0.0, -0.1]));
    assert_eq!(result[2], ('L', vec![100.0, -0.5]));
}

#[test]
fn malformed_first_command_is_empty_and_malformed_later_command_keeps_prefix() {
    for input in ["L1 1", "Z", "M1", "?", "\u{a0}M1 1"] {
        assert!(parse(input).unwrap().is_empty(), "{input}");
    }
    let prefix = parse("M1 1L8 1 8 8 1 8Z").unwrap();
    for suffix in [" X1 1", " L2", " A2 2 0 2 1 4 0", " M", " ?"] {
        assert_eq!(
            parse(&format!("M1 1L8 1 8 8 1 8Z{suffix}")).unwrap(),
            prefix
        );
    }
}

#[test]
fn incomplete_repeated_command_keeps_only_complete_coordinate_groups() {
    assert_eq!(
        parse("M1 1L8 1 8 8 1 8 1").unwrap(),
        parse("M1 1L8 1 8 8 1 8").unwrap()
    );
}

#[test]
fn resource_limits_reject_instead_of_reporting_a_partial_success() {
    assert!(parse(&" ".repeat(MAX_SOURCE_BYTES)).unwrap().is_empty());
    assert!(parse(&" ".repeat(MAX_SOURCE_BYTES + 1)).is_none());
    assert!(parse(&format!("M0 0{}", " L1 1".repeat(MAX_SEGMENTS - 1))).is_some());
    assert!(parse(&format!("M0 0{}", " L1 1".repeat(MAX_SEGMENTS))).is_none());
}

#[test]
fn parser_results_are_owned_and_later_parses_do_not_reuse_previous_storage() {
    let mut first = parse("M1 2L3 4").unwrap();
    first[0].1[0] = 999.0;
    assert_eq!(parse("M1 2L3 4").unwrap()[0].1, [1.0, 2.0]);
}

#[test]
fn malformed_host_payloads_decline_without_parsing_or_coercing_author_values() {
    for argument in [JsValue::Null, JsValue::Boolean(true), JsValue::from(12.0)] {
        assert!(matches!(
            segments(&[JsValue::Null, argument]),
            JsValue::Null
        ));
    }
    assert!(matches!(segments(&[]), JsValue::Null));
    assert!(
        matches!(segments(&[JsValue::Null, JsValue::String("L0 0".into())]), JsValue::Array(v) if v.is_empty())
    );
}
