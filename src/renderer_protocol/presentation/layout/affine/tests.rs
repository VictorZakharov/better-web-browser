use super::*;

#[test]
fn affine_wire_retains_binary_double_coefficients() {
    let matrix = Affine2D([0.04, 0.000_000_1, -1.0, 1.25, 123.000_000_1, -99.5]);
    let mut writer = WireWriter::new();
    encode(&mut writer, matrix).unwrap();
    let bytes = writer.finish();
    let mut reader = WireReader::new(&bytes);
    assert_eq!(decode(&mut reader).unwrap(), matrix);
}

#[test]
fn nonfinite_and_overbudget_matrix_coefficients_fail_closed() {
    for value in [f64::NAN, f64::INFINITY, -f64::INFINITY, f64::MAX] {
        let matrix = Affine2D([value, 0.0, 0.0, 1.0, 0.0, 0.0]);
        assert!(encode(&mut WireWriter::new(), matrix).is_err());
        let mut writer = WireWriter::new();
        for v in matrix.0 {
            writer.u64(v.to_bits());
        }
        assert!(decode(&mut WireReader::new(&writer.finish())).is_err());
    }
    assert!(valid(Affine2D([0.0, 0.0, 0.0, 0.0, 0.0, 0.0])));
}
