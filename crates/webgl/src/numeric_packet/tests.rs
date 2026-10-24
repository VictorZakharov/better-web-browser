//! Admission is independent of the GL driver; malformed tails execute nothing.
use super::*;

fn record(op: &str, id: u32, integers: &[f64], floats: &[f64]) -> Vec<f64> {
    let code = operations().iter().position(|entry| entry == op).unwrap();
    [
        vec![
            code as f64,
            id as f64,
            integers.len() as f64,
            floats.len() as f64,
        ],
        integers.to_vec(),
        floats.to_vec(),
    ]
    .concat()
}

#[test]
fn one_shared_table_contains_only_unique_native_void_operations() {
    let mut names = std::collections::HashSet::new();
    assert!(operations().len() > 80);
    for op in operations() {
        assert!(names.insert(op));
        assert!(op.bytes().all(|byte| byte.is_ascii_alphanumeric()));
        assert!(
            NumericCommand::new(op.clone(), Vec::new(), Vec::new()).is_some(),
            "{op}"
        );
        let entries = decode(&record(op, u32::MAX, &[], &[])).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, u32::MAX);
    }
    for observation in [
        "getError",
        "getUniform",
        "readPixels",
        "finish",
        "shaderSource",
    ] {
        assert!(!operations().iter().any(|name| name == observation));
    }
    assert_eq!(MAX_PACKET_VALUES, 8704);
}

#[test]
fn records_keep_order_context_names_and_full_precision_values() {
    let mut packet = record(
        "clearColor",
        3,
        &[],
        &[-0., f64::NAN, f64::INFINITY, f64::NEG_INFINITY],
    );
    packet.extend(record(
        "uniform4uiv",
        9,
        &[17., 0., 4_294_967_295., -1.],
        &[],
    ));
    packet.extend(record(
        "uniformMatrix2x3fv",
        3,
        &[12., 0.],
        &[1., 2., 3., 4., 5., 6.],
    ));
    let entries = decode(&packet).unwrap();
    assert_eq!(
        entries.iter().map(|entry| entry.0).collect::<Vec<_>>(),
        [3, 9, 3]
    );
    let mut commands = entries.into_iter().map(|entry| entry.1.into_command());
    let first = commands.next().unwrap();
    assert_eq!(first.op, "clearColor");
    assert!(first.f[0].is_sign_negative() && first.f[1].is_nan());
    assert_eq!(first.f[2..], [f64::INFINITY, f64::NEG_INFINITY]);
    assert_eq!(commands.next().unwrap().i, [17, 0, 4_294_967_295, -1]);
    assert_eq!(commands.next().unwrap().f, [1., 2., 3., 4., 5., 6.]);
}

#[test]
fn all_truncated_prefixes_and_bad_suffixes_reject_the_whole_packet() {
    let first = record("clearColor", 1, &[], &[0., 1., 0., 1.]);
    for length in 0..first.len() {
        assert!(decode(&first[..length]).is_none(), "prefix {length}");
    }
    for suffix in [
        vec![0.],
        vec![0., 1.],
        vec![0., 1., 0.],
        vec![0., 1., 65., 0.],
        vec![0., 1., 0., 1.],
        vec![f64::NAN, 1., 0., 0.],
        vec![1_000_000., 1., 0., 0.],
    ] {
        let mut packet = first.clone();
        packet.extend(suffix);
        assert!(decode(&packet).is_none());
    }
}

#[test]
fn every_metadata_slot_rejects_nonintegral_or_out_of_range_values() {
    let valid = record("clear", 17, &[16384.], &[]);
    for index in 0..HEADER_VALUES {
        for value in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            -1.,
            0.5,
            4_294_967_296.,
        ] {
            let mut invalid = valid.clone();
            invalid[index] = value;
            assert!(decode(&invalid).is_none(), "slot {index}: {value}");
        }
    }
    let mut invalid = valid.clone();
    invalid[1] = 0.;
    assert!(decode(&invalid).is_none());
    for index in [2, 3] {
        let mut invalid = valid.clone();
        invalid[index] = 65.;
        assert!(decode(&invalid).is_none());
    }
}

#[test]
fn integer_safe_range_is_exact_and_float_sentinels_are_not_coerced() {
    for value in [-9_007_199_254_740_991., -0., 0., 9_007_199_254_740_991.] {
        assert!(decode(&record("clear", 1, &[value], &[])).is_some());
    }
    for value in [
        -9_007_199_254_740_992.,
        9_007_199_254_740_992.,
        1.5,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ] {
        assert!(decode(&record("clear", 1, &[value], &[])).is_none());
        assert!(decode(&record("clearColor", 1, &[], &[value])).is_some());
    }
}

#[test]
fn command_count_and_combined_argument_count_are_independently_bounded() {
    let empty = record("clear", 1, &[], &[]);
    assert_eq!(decode(&empty.repeat(128)).unwrap().len(), 128);
    assert!(decode(&empty.repeat(129)).is_none());
    let largest = record("uniform4fv", 1, &[0.; 32], &[0.; 32]);
    assert_eq!(largest.len(), 68);
    let full = largest.repeat(128);
    assert_eq!(full.len(), MAX_PACKET_VALUES);
    assert_eq!(decode(&full).unwrap().len(), 128);
    assert!(decode(&largest.repeat(129)).is_none());
    assert!(decode(&record("uniform4fv", 1, &[0.; 33], &[0.; 32])).is_none());
}
