use super::*;

fn scratch() -> Command {
    Command {
        op: String::with_capacity(64),
        i: Vec::with_capacity(8),
        f: Vec::with_capacity(16),
        text: "must not leak".into(),
    }
}

#[test]
fn common_packet_vectors_and_matrices_own_inline_storage() {
    for floats in [0, 1, 4, 9, 12, 16] {
        let command = NumericCommand::packet(
            "uniformMatrix4fv",
            [5, 0].into_iter().collect(),
            (0..floats).map(|value| value as f64).collect(),
        )
        .unwrap();
        assert!(matches!(command.op, Cow::Borrowed(_)));
        assert!(!command.i.spilled() && !command.f.spilled());
        assert_eq!(command.f.len(), floats);
    }
}

#[test]
fn inline_dispatch_reuses_buffers_and_clears_every_previous_argument_tail() {
    let mut converted = scratch();
    let name = converted.op.as_ptr();
    let integers = converted.i.as_ptr();
    let floats = converted.f.as_ptr();
    NumericCommand::packet(
        "uniformMatrix4fv",
        [15, 0].into_iter().collect(),
        (0..16).map(|value| value as f64).collect(),
    )
    .unwrap()
    .write_command(&mut converted);
    assert_eq!(converted.i, [15, 0]);
    assert_eq!(converted.f.len(), 16);
    assert_eq!(converted.op.as_ptr(), name);
    assert_eq!(converted.i.as_ptr(), integers);
    assert_eq!(converted.f.as_ptr(), floats);
    assert!(converted.text.is_empty());
    NumericCommand::packet("clear", [16384].into_iter().collect(), SmallVec::new())
        .unwrap()
        .write_command(&mut converted);
    assert_eq!(converted.op, "clear");
    assert_eq!(converted.i, [16384]);
    assert!(converted.f.is_empty());
    assert_eq!(converted.i.as_ptr(), integers);
    assert_eq!(converted.f.as_ptr(), floats);
}

#[test]
fn spilled_uniform_storage_moves_without_a_second_heap_allocation() {
    let mut converted = scratch();
    let values: SmallVec<[f64; 16]> = (0..256).map(|value| value as f64).collect();
    let pointer = values.as_ptr();
    assert!(values.spilled());
    NumericCommand::packet("uniform4fv", [9].into_iter().collect(), values)
        .unwrap()
        .write_command(&mut converted);
    assert_eq!(converted.f.as_ptr(), pointer);
    assert_eq!(converted.f.len(), 256);
    assert_eq!(converted.f[255], 255.);
    NumericCommand::packet(
        "clearColor",
        SmallVec::new(),
        [0., 1., 0., 1.].into_iter().collect(),
    )
    .unwrap()
    .write_command(&mut converted);
    assert_eq!(converted.f, [0., 1., 0., 1.]);
    assert_eq!(converted.f.as_ptr(), pointer);
}

#[test]
fn owned_and_inline_paths_preserve_float_bits_and_safe_integer_admission() {
    let values = [-0., f64::NAN, f64::INFINITY, f64::NEG_INFINITY];
    let mut converted = scratch();
    NumericCommand::packet(
        "uniform4f",
        [9].into_iter().collect(),
        values.into_iter().collect(),
    )
    .unwrap()
    .write_command(&mut converted);
    for (actual, expected) in converted.f.iter().zip(values) {
        assert_eq!(actual.to_bits(), expected.to_bits());
    }
    NumericCommand::new("uniform4f".into(), vec![9], values.into())
        .unwrap()
        .write_command(&mut converted);
    for (actual, expected) in converted.f.iter().zip(values) {
        assert_eq!(actual.to_bits(), expected.to_bits());
    }
    assert!(
        NumericCommand::packet("clear", [i64::MAX].into_iter().collect(), SmallVec::new())
            .is_none()
    );
    assert!(NumericCommand::packet("readPixels", SmallVec::new(), SmallVec::new()).is_none());
}
