//! Private protocol boundaries are validated independently from author bindings.
use super::*;
use crate::engine::script::audio_codecs::{config::Config, tests::wait};

fn args(operation: &str, id: f64) -> Vec<JsValue> {
    vec![JsValue::from(operation.to_owned()), JsValue::from(id)]
}

#[test]
fn unknown_operations_do_not_mutate_the_codec_pool() {
    let mut codecs = AudioCodecs::default();
    for operation in [
        "audioCodec",
        "audioCodecOpenFile",
        "audioCodecReadUrl",
        "imageDecodeStart",
    ] {
        assert!(dispatch(operation, &[], &mut codecs).unwrap().is_none());
    }
    assert!(codecs.sessions.is_empty());
}

#[test]
fn protocol_ids_require_exact_nonzero_unsigned_32_bit_numbers() {
    for operation in [
        "audioCodecInput",
        "audioCodecFlush",
        "audioCodecPoll",
        "audioCodecClose",
    ] {
        for value in [
            f64::NAN,
            f64::INFINITY,
            -1.0,
            0.0,
            0.5,
            1.5,
            4_294_967_296.0,
        ] {
            let mut codecs = AudioCodecs::default();
            assert!(
                dispatch(operation, &args(operation, value), &mut codecs).is_err(),
                "{operation} {value}"
            );
            assert!(codecs.sessions.is_empty());
        }
    }
}

#[test]
fn native_configuration_json_is_fail_closed_and_bounded() {
    let mut codecs = AudioCodecs::default();
    for json in [
        "{}".to_string(),
        "null".into(),
        "not json".into(),
        " ".repeat(4097),
        r#"{"codec":"pcm-u8","sampleRate":8000,"numberOfChannels":1,"unknown":true}"#.into(),
    ] {
        let args = vec![
            JsValue::from("audioCodecSupported".to_owned()),
            JsValue::from(false),
            JsValue::from(json.clone()),
        ];
        assert!(matches!(
            dispatch("audioCodecSupported", &args, &mut codecs).unwrap(),
            Some(JsValue::Boolean(false))
        ));
        assert!(dispatch("audioCodecStart", &args, &mut codecs).is_err());
        assert!(codecs.sessions.is_empty());
    }
}

#[test]
fn private_input_accepts_only_owned_bytes_and_exact_safe_timestamps() {
    let mut codecs = AudioCodecs::default();
    let config = Config::read(
        r#"{"codec":"pcm-u8","sampleRate":8000,"numberOfChannels":1}"#,
        false,
    )
    .unwrap();
    let id = codecs.start(config, false).unwrap();
    wait(&mut codecs, id).unwrap();
    for timestamp in [
        f64::NAN,
        f64::INFINITY,
        -f64::INFINITY,
        0.5,
        9_007_199_254_740_992.0,
        -9_007_199_254_740_992.0,
    ] {
        let input = vec![
            JsValue::from("audioCodecInput".to_owned()),
            JsValue::from(id),
            JsValue::Bytes(vec![128]),
            JsValue::from(timestamp),
        ];
        assert!(dispatch("audioCodecInput", &input, &mut codecs).is_err());
        assert!(!codecs.sessions.get(&id).unwrap().busy);
    }
    let wrong = vec![
        JsValue::from("audioCodecInput".to_owned()),
        JsValue::from(id),
        JsValue::from("not bytes".to_owned()),
        JsValue::from(0),
    ];
    assert!(dispatch("audioCodecInput", &wrong, &mut codecs).is_err());
    let missing = vec![
        JsValue::from("audioCodecInput".to_owned()),
        JsValue::from(id),
    ];
    assert!(dispatch("audioCodecInput", &missing, &mut codecs).is_err());
    codecs.close(id);
}

#[test]
fn native_outputs_are_typed_bytes_not_untrusted_json_or_host_references() {
    let value = output(Output {
        bytes: vec![0, 128, 255],
        format: "u8",
        timestamp: -99,
        duration: 375,
        frames: 3,
        sample_rate: 8000,
        channels: 1,
        description: Some(vec![1, 2]),
    });
    let JsValue::Object(entries) = value else {
        panic!("output must be a plain private object")
    };
    let field = |name| &entries.iter().find(|(key, _)| key == name).unwrap().1;
    assert!(matches!(field("bytes"),JsValue::Bytes(bytes) if bytes==&[0,128,255]));
    assert!(matches!(field("description"),JsValue::Bytes(bytes) if bytes==&[1,2]));
    assert_eq!(field("format").string_value(), "u8");
    assert_eq!(field("timestamp").as_number(), Some(-99.0));
    assert_eq!(field("frames").as_number(), Some(3.0));
    assert_eq!(field("duration").as_number(), Some(375.0));
    assert_eq!(field("sampleRate").as_number(), Some(8000.0));
    assert_eq!(field("numberOfChannels").as_number(), Some(1.0));
}

#[test]
fn close_is_idempotent_but_closed_input_cannot_resurrect_a_session() {
    let mut codecs = AudioCodecs::default();
    let config = Config::read(
        r#"{"codec":"pcm-u8","sampleRate":8000,"numberOfChannels":1}"#,
        false,
    )
    .unwrap();
    let id = codecs.start(config, false).unwrap();
    wait(&mut codecs, id).unwrap();
    let close = args("audioCodecClose", id as f64);
    dispatch("audioCodecClose", &close, &mut codecs).unwrap();
    dispatch("audioCodecClose", &close, &mut codecs).unwrap();
    let input = vec![
        JsValue::from("audioCodecInput".to_owned()),
        JsValue::from(id),
        JsValue::Bytes(vec![128]),
        JsValue::from(0),
    ];
    assert!(dispatch("audioCodecInput", &input, &mut codecs).is_err());
    assert!(codecs.sessions.is_empty());
}
