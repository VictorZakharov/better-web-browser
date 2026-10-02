//! Malformed native calls cannot spend permits, mutate sessions, or narrow times.
use super::*;

fn call(
    operation: &str,
    values: Vec<JsValue>,
    codecs: &mut VideoCodecs,
) -> JsResult<Option<JsValue>> {
    let mut args = vec![JsValue::from(operation.to_owned())];
    args.extend(values);
    dispatch(operation, &args, codecs)
}

#[test]
fn timestamps_and_durations_require_exact_finite_integer_arithmetic() {
    for value in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        0.5,
        9_007_199_254_740_992.0,
    ] {
        assert!(number(&[JsValue::from(value)], 0, false).is_err());
        assert!(number(&[JsValue::from(value)], 0, true).is_err());
    }
    assert_eq!(number(&[JsValue::from(-17.0)], 0, false).unwrap(), -17);
    assert!(number(&[JsValue::from(-17.0)], 0, true).is_err());
    assert_eq!(
        number(&[JsValue::from(9_007_199_254_740_991.0)], 0, true).unwrap(),
        9_007_199_254_740_991
    );
    assert!(number(&[], 0, false).is_err());
}

#[test]
fn invalid_session_ids_are_not_truncated_or_wrapped_into_live_sessions() {
    let mut codecs = VideoCodecs::default();
    for value in [0.0, -1.0, 0.5, 4_294_967_296.0, f64::NAN, f64::INFINITY] {
        assert!(call("videoCodecClose", vec![JsValue::from(value)], &mut codecs).is_err());
        assert!(codecs.sessions.is_empty());
        assert_eq!(codecs.workers.load(Ordering::Acquire), 0);
    }
    // Closing an already removed valid identifier is idempotent at the native
    // boundary, so finalizers do not need to revive any author-visible resource.
    assert!(call("videoCodecClose", vec![JsValue::from(1u32)], &mut codecs).is_ok());
}

#[test]
fn support_checks_do_not_allocate_native_threads_or_consume_session_ids() {
    let mut codecs = VideoCodecs::default();
    for json in ["{}", "not JSON", r#"{"codec":"av01.0.04M.10"}"#] {
        let result = call(
            "videoCodecSupported",
            vec![JsValue::from(json.to_owned())],
            &mut codecs,
        )
        .unwrap();
        assert!(matches!(result, Some(JsValue::Boolean(false))));
        assert!(
            call(
                "videoCodecStart",
                vec![JsValue::from(json.to_owned())],
                &mut codecs
            )
            .is_err()
        );
    }
    let json = r#"{"codec":"av01.0.04M.08","hardwareAcceleration":"no-preference","optimizeForLatency":false,"rotation":0,"flip":false}"#;
    let result = call(
        "videoCodecSupported",
        vec![JsValue::from(json.to_owned())],
        &mut codecs,
    )
    .unwrap();
    assert!(matches!(result, Some(JsValue::Boolean(true))));
    assert!(codecs.sessions.is_empty());
    assert_eq!(codecs.next_id, 0);
    assert_eq!(codecs.workers.load(Ordering::Acquire), 0);
}

#[test]
fn empty_or_oversized_packets_fail_before_creating_a_native_command() {
    let mut codecs = VideoCodecs::default();
    for bytes in [vec![], vec![0; MAX_INPUT + 1]] {
        let args = vec![
            JsValue::from(1u32),
            JsValue::Bytes(bytes),
            JsValue::from(0.0),
            JsValue::Null,
            JsValue::Boolean(true),
        ];
        assert!(call("videoCodecInput", args, &mut codecs).is_err());
        assert!(codecs.sessions.is_empty());
    }
    assert!(call("videoCodecInput", vec![JsValue::from(1u32)], &mut codecs).is_err());
}

#[test]
fn output_boundary_preserves_negative_timestamps_and_absent_duration() {
    let result = output(Output {
        bytes: vec![10, 20, 30, 255],
        width: 1,
        height: 1,
        timestamp: -900,
        duration: None,
    });
    let JsValue::Object(fields) = result else {
        panic!("output object required")
    };
    assert!(
        fields
            .iter()
            .any(|(name, value)| name == "timestamp" && value.as_number() == Some(-900.0))
    );
    assert!(
        fields
            .iter()
            .any(|(name, value)| name == "duration" && matches!(value, JsValue::Null))
    );
    assert!(
        fields.iter().any(
            |(name, value)| name == "bytes" && value.as_bytes() == Some(&[10, 20, 30, 255][..])
        )
    );
}
