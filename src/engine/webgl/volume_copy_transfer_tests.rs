//! Packed native readback retains exact integer and floating-point bit domains.
use super::gl;
use super::volume_copy_conversion::Transfer;

fn words(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|word| word.to_ne_bytes()).collect()
}

#[test]
fn rgb_float_copy_transport_preserves_nan_negative_zero_and_hdr_bits() {
    let transfer = Transfer::for_format(0x8815).unwrap();
    let source = words(&[
        0x7fc01234, 0x80000000, 0x41000000, 0x3f800000, 0xff800000, 0x00000001, 0xbf800000, 0,
    ]);
    assert_eq!(
        transfer.select_rgb(&source).unwrap(),
        words(&[
            0x7fc01234, 0x80000000, 0x41000000, 0xff800000, 0x00000001, 0xbf800000
        ])
    );
    assert_eq!(
        Transfer::for_format(0x881b)
            .unwrap()
            .select_rgb(&source)
            .unwrap(),
        transfer.select_rgb(&source).unwrap()
    );
}

#[test]
fn rgb_integer_copy_transport_keeps_all_32_bits_and_discards_only_alpha() {
    for format in [0x8d71, 0x8d83] {
        let transfer = Transfer::for_format(format).unwrap();
        assert_eq!(
            transfer
                .select_rgb(&words(&[0xffffffff, 0x80000000, 0x12345678, 0]))
                .unwrap(),
            words(&[0xffffffff, 0x80000000, 0x12345678])
        );
    }
}

#[test]
fn rgb_integer_copy_transport_checks_narrower_ranges_without_wrapping() {
    let signed8 = Transfer::for_format(0x8d8f).unwrap();
    assert_eq!(
        signed8
            .select_rgb(&words(&[(-128i32) as u32, 127, (-1i32) as u32, 0]))
            .unwrap(),
        vec![128, 127, 255]
    );
    let unsigned8 = Transfer::for_format(0x8d7d).unwrap();
    assert_eq!(
        unsigned8.select_rgb(&words(&[0, 255, 128, 0])).unwrap(),
        vec![0, 255, 128]
    );
    let signed16 = Transfer::for_format(0x8d89).unwrap();
    assert_eq!(
        signed16
            .select_rgb(&words(&[(-32768i32) as u32, 32767, (-1i32) as u32, 0]))
            .unwrap(),
        [-32768i16, 32767, -1]
            .iter()
            .flat_map(|v| v.to_ne_bytes())
            .collect::<Vec<_>>()
    );
    let unsigned16 = Transfer::for_format(0x8d77).unwrap();
    assert_eq!(
        unsigned16
            .select_rgb(&words(&[0, 65535, 32768, 0]))
            .unwrap(),
        [0u16, 65535, 32768]
            .iter()
            .flat_map(|v| v.to_ne_bytes())
            .collect::<Vec<_>>()
    );
    for (transfer, outside) in [
        (signed8, 128),
        (signed8, (-129i32) as u32),
        (unsigned8, 256),
        (signed16, 32768),
        (signed16, (-32769i32) as u32),
        (unsigned16, 65536),
    ] {
        assert_eq!(
            transfer.select_rgb(&words(&[outside, 0, 0, 0])),
            Err(gl::INVALID_OPERATION)
        );
    }
}

#[test]
fn rgb_copy_transport_rejects_partial_pixels_and_unimplemented_formats() {
    let transfer = Transfer::for_format(0x8815).unwrap();
    for size in [1, 4, 12, 15, 17, 31] {
        assert_eq!(
            transfer.select_rgb(&vec![0; size]),
            Err(gl::INVALID_OPERATION)
        );
    }
    assert_eq!(transfer.select_rgb(&[]).unwrap(), Vec::<u8>::new());
    // RGB9_E5 copies are forbidden by GLES validation; do not widen them here.
    for format in [0x8c3d, gl::DEPTH_COMPONENT, gl::RGBA, 0] {
        assert!(matches!(
            Transfer::for_format(format),
            Err(gl::INVALID_OPERATION)
        ));
    }
}
