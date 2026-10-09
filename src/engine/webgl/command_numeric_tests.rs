//! Typed and JSON setters share one order, one error stream and one owner.
use super::*;

fn numeric(contexts: &mut Contexts, id: u32, op: &str, i: &[i64], f: &[f64]) -> bool {
    let command = NumericCommand::new(op.into(), i.to_vec(), f.to_vec()).unwrap();
    contexts.execute_numeric(id, command)
}

#[test]
fn numeric_command_admission_is_bounded_and_never_accepts_observations() {
    for op in [
        "clearColor",
        "bindBuffer",
        "drawElements",
        "uniformMatrix4fv",
        "uniform4uiv",
    ] {
        assert!(NumericCommand::new(op.into(), vec![], vec![]).is_some());
    }
    for op in [
        "getError",
        "getParameter",
        "isBuffer",
        "finish",
        "flush",
        "createBuffer",
        "shaderSource",
        "texImage2D",
        "clearSuffix",
        "",
    ] {
        assert!(NumericCommand::new(op.into(), vec![], vec![]).is_none());
    }
    assert!(NumericCommand::new("clearColor".into(), vec![0; 32], vec![0.; 32]).is_some());
    assert!(NumericCommand::new("clearColor".into(), vec![0; 33], vec![0.; 32]).is_none());
    assert!(NumericCommand::new("clearColor".into(), vec![], vec![0.; 65]).is_none());
    assert!(NumericCommand::new("clear".into(), vec![i64::MAX], vec![]).is_none());
    assert!(NumericCommand::new("clear".into(), vec![i64::MIN], vec![]).is_none());
    assert!(NumericCommand::new("clear".into(), vec![9_007_199_254_740_991], vec![]).is_some());
    let values = NumericCommand::new(
        "uniform4f".into(),
        vec![1],
        vec![-0., f64::NAN, f64::INFINITY, f64::NEG_INFINITY],
    )
    .unwrap()
    .into_command();
    assert!(values.f[0].is_sign_negative() && values.f[1].is_nan());
    assert_eq!(values.f[2], f64::INFINITY);
    assert_eq!(values.f[3], f64::NEG_INFINITY);
}

#[test]
fn every_bootstrap_numeric_operation_is_admitted_by_the_native_void_policy() {
    let source = include_str!("../script/bootstrap/webgl_private_wire.js");
    let list = source
        .split_once("const webGlWireNumericOperations = new Set((")
        .unwrap()
        .1
        .split_once(".split(' '));")
        .unwrap()
        .0;
    let mut count = 0;
    for fragment in list
        .split('\'')
        .enumerate()
        .filter_map(|(index, text)| (index % 2 == 1).then_some(text))
    {
        for op in fragment.split_whitespace() {
            assert!(
                NumericCommand::new(op.into(), vec![], vec![]).is_some(),
                "{op}"
            );
            count += 1;
        }
    }
    assert!(
        count > 100,
        "operation declarations must not silently disappear"
    );
    assert_eq!(MAX_NUMERIC_VALUES, 64);
}

#[test]
fn typed_and_json_setters_preserve_cross_context_order_and_automatic_flushes() {
    let mut contexts = Contexts::default();
    let first = contexts.create(2, 2, "{}").unwrap();
    let second = contexts.create(2, 2, "{}").unwrap();
    for _ in 0..65 {
        assert!(numeric(
            &mut contexts,
            first,
            "clearColor",
            &[],
            &[0., 1., 0., 1.]
        ));
        contexts.execute(second, r#"{"op":"clearColor","f":[1,0,0,1]}"#, None);
        assert!(numeric(
            &mut contexts,
            first,
            "clear",
            &[gl::COLOR_BUFFER_BIT as i64],
            &[]
        ));
        contexts.execute(second, r#"{"op":"clear","i":[16384]}"#, None);
    }
    assert!(
        contexts
            .snapshot(first)
            .unwrap()
            .2
            .chunks_exact(4)
            .all(|p| p == [0, 255, 0, 255])
    );
    assert!(
        contexts
            .snapshot(second)
            .unwrap()
            .2
            .chunks_exact(4)
            .all(|p| p == [255, 0, 0, 255])
    );
    assert_eq!(
        contexts.execute(first, r#"{"op":"getError"}"#, None),
        json!(0)
    );
    assert_eq!(
        contexts.execute(second, r#"{"op":"getError"}"#, None),
        json!(0)
    );
    contexts.remove(first);
    assert!(!numeric(&mut contexts, first, "clear", &[16384], &[]));
    assert!(numeric(
        &mut contexts,
        second,
        "clearColor",
        &[],
        &[0., 0., 1., 1.]
    ));
    assert!(numeric(&mut contexts, second, "clear", &[16384], &[]));
    assert!(
        contexts
            .snapshot(second)
            .unwrap()
            .2
            .chunks_exact(4)
            .all(|p| p == [0, 0, 255, 255])
    );
}

#[test]
fn typed_setters_keep_native_shape_enum_and_ordinary_error_validation() {
    let mut contexts = Contexts::default();
    let id = contexts.create(2, 2, "{}").unwrap();
    numeric(&mut contexts, id, "enable", &[0xdead], &[]);
    numeric(&mut contexts, id, "viewport", &[0, 0, -1, 2], &[]);
    numeric(&mut contexts, id, "clearColor", &[], &[0., 1., 0., 1.]);
    numeric(&mut contexts, id, "clear", &[16384], &[]);
    assert_eq!(
        contexts.execute(id, r#"{"op":"getError"}"#, None),
        json!(gl::INVALID_ENUM)
    );
    assert_eq!(
        contexts.execute(id, r#"{"op":"getError"}"#, None),
        json!(gl::INVALID_VALUE)
    );
    assert_eq!(contexts.execute(id, r#"{"op":"getError"}"#, None), json!(0));
    assert!(
        contexts
            .snapshot(id)
            .unwrap()
            .2
            .chunks_exact(4)
            .all(|p| p == [0, 255, 0, 255])
    );
}

#[test]
fn typed_buffer_bindings_cannot_overtake_binary_uploads_or_deletion() {
    let mut contexts = Contexts::default();
    let id = contexts.create(1, 1, r#"{"api":"webgl2"}"#).unwrap();
    let create = r#"{"op":"createBuffer"}"#;
    let first = contexts.execute(id, create, None).as_i64().unwrap();
    let second = contexts.execute(id, create, None).as_i64().unwrap();
    for (buffer, bytes) in [(first, vec![1, 2, 3, 4]), (second, vec![5, 6, 7, 8])] {
        numeric(
            &mut contexts,
            id,
            "bindBuffer",
            &[gl::ARRAY_BUFFER as i64, buffer],
            &[],
        );
        contexts.execute_owned(
            id,
            r#"{"op":"bufferData","i":[34962,4,35044]}"#,
            Some(bytes.clone()),
        );
        let PixelReply::Bytes(actual) =
            contexts.read_pixels(id, r#"{"op":"getBufferSubData","i":[34962,0,4]}"#, None)
        else {
            panic!("buffer read")
        };
        assert_eq!(actual, bytes);
    }
    numeric(
        &mut contexts,
        id,
        "bindBuffer",
        &[gl::ARRAY_BUFFER as i64, first],
        &[],
    );
    numeric(&mut contexts, id, "deleteBuffer", &[first], &[]);
    assert_eq!(
        contexts.execute(id, r#"{"op":"getParameter","i":[34964]}"#, None),
        Value::Null
    );
    numeric(
        &mut contexts,
        id,
        "bindBuffer",
        &[gl::ARRAY_BUFFER as i64, second],
        &[],
    );
    let PixelReply::Bytes(actual) =
        contexts.read_pixels(id, r#"{"op":"getBufferSubData","i":[34962,0,4]}"#, None)
    else {
        panic!("surviving buffer read")
    };
    assert_eq!(actual, [5, 6, 7, 8]);
}
