use super::*;

#[test]
fn all_admitted_yuv_planes_convert_limited_luma_with_exact_alpha() {
    let luma = [16, 235, 16, 235];
    let subsampled = [128];
    let full = [128; 4];
    let horizontal = [128; 2];
    let interleaved = [128, 128];
    let alpha = [1, 2, 128, 255];
    for (format, planes) in [
        ("I420", vec![&luma[..], &subsampled[..], &subsampled[..]]),
        (
            "I420A",
            vec![&luma[..], &subsampled[..], &subsampled[..], &alpha[..]],
        ),
        ("I422", vec![&luma[..], &horizontal[..], &horizontal[..]]),
        (
            "I422A",
            vec![&luma[..], &horizontal[..], &horizontal[..], &alpha[..]],
        ),
        ("I444", vec![&luma[..], &full[..], &full[..]]),
        ("I444A", vec![&luma[..], &full[..], &full[..], &alpha[..]]),
        ("NV12", vec![&luma[..], &interleaved[..]]),
    ] {
        let result = convert(2, 2, format, &planes, false, "bt709").unwrap();
        assert_eq!(&result[..3], &[0, 0, 0], "{format}");
        assert_eq!(&result[4..7], &[255, 255, 255], "{format}");
        for (index, pixel) in result.chunks_exact(4).enumerate() {
            assert_eq!(
                pixel[3],
                if format.ends_with('A') {
                    alpha[index]
                } else {
                    255
                }
            );
        }
    }
}

#[test]
fn matrix_selection_changes_actual_chromatic_samples() {
    let planes: [&[u8]; 3] = [&[100], &[40], &[200]];
    let rec709 = convert(1, 1, "I444", &planes, false, "bt709").unwrap();
    let rec601 = convert(1, 1, "I444", &planes, false, "smpte170m").unwrap();
    assert_ne!(rec709, rec601);
    assert_eq!(
        rec601,
        convert(1, 1, "I444", &planes, false, "bt470bg").unwrap()
    );
}

#[test]
fn odd_dimensions_use_ceil_chroma_sizes() {
    let y = [16; 9];
    let chroma = [128; 4];
    let output = convert(3, 3, "I420", &[&y, &chroma, &chroma], false, "bt709").unwrap();
    assert_eq!(output.len(), 36);
    assert!(output.chunks_exact(4).all(|pixel| pixel == [0, 0, 0, 255]));
}

#[test]
fn boundary_rejects_oversized_and_incomplete_planes_before_library_entry() {
    assert!(convert(0, 1, "I420", &[], false, "bt709").is_err());
    assert!(convert(u32::MAX, u32::MAX, "I420", &[], false, "bt709").is_err());
    assert!(convert(2, 2, "I420", &[&[0; 4], &[128], &[]], false, "bt709").is_err());
    assert!(convert(1, 1, "I444", &[&[0], &[128], &[128]], false, "invalid").is_err());
    assert!(convert(1, 1, "RGBA", &[&[0; 4]], true, "bt709").is_err());
}

#[test]
fn alpha_plane_admission_is_exact_for_all_three_subsampling_modes() {
    for (format, chroma) in [("I420A", 1usize), ("I422A", 2), ("I444A", 4)] {
        let y = [16, 235, 16, 235];
        let u = vec![128; chroma];
        let v = vec![128; chroma];
        assert!(
            convert(2, 2, format, &[&y, &u, &v], false, "bt709").is_err(),
            "missing alpha {format}"
        );
        assert!(
            convert(2, 2, format, &[&y, &u, &v, &[255; 3]], false, "bt709").is_err(),
            "short alpha {format}"
        );
        assert!(
            convert(2, 2, format, &[&y, &u, &v, &[255; 5]], false, "bt709").is_err(),
            "long alpha {format}"
        );
        let pixels = convert(
            2,
            2,
            format,
            &[&y, &u, &v, &[0, 64, 128, 255]],
            false,
            "bt709",
        )
        .unwrap();
        assert_eq!(
            pixels
                .chunks_exact(4)
                .map(|pixel| pixel[3])
                .collect::<Vec<_>>(),
            [0, 64, 128, 255]
        );
    }
}

#[test]
fn full_range_neutral_chroma_preserves_luma_endpoints_in_each_layout() {
    let y = [0, 255, 0, 255];
    let u = [128; 4];
    let v = [128; 4];
    for (format, count) in [("I420", 1), ("I422", 2), ("I444", 4)] {
        let output = convert(2, 2, format, &[&y, &u[..count], &v[..count]], true, "bt709").unwrap();
        assert_eq!(&output[..4], &[0, 0, 0, 255]);
        assert_eq!(&output[4..8], &[255, 255, 255, 255]);
    }
}
