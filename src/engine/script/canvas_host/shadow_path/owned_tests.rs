//! Only the bridge's independent native allocation can be consumed by paint.
use super::*;

#[test]
fn owned_shadow_keeps_destination_allocation_and_matches_borrowed_ordered_paint() {
    for kind in ["fill", "stroke"] {
        for mode in [
            "source-over",
            "copy",
            "source-in",
            "destination-out",
            "multiply",
        ] {
            for alpha in [0, 127, 255] {
                for clip in [None, Some(vec![0x96; 96]), Some(vec![0; 96])] {
                    let mut args = tests::arguments(kind, mode, 0.37, alpha, clip);
                    let expected = paint(&args);
                    assert!(matches!(expected, JsValue::Bytes(_)));
                    let pointer = args[1].as_bytes().unwrap().as_ptr();
                    let preserved: Vec<_> = args.iter().skip(2).cloned().collect();
                    let actual = paint_owned(&mut args);
                    assert_eq!(actual, expected, "{kind}, {mode}, {alpha}");
                    assert_eq!(actual.as_bytes().unwrap().as_ptr(), pointer);
                    assert_eq!(args[1], JsValue::Null);
                    assert_eq!(&args[2..], preserved.as_slice());
                }
            }
        }
    }
}

#[test]
fn invalid_owned_shadow_returns_no_partial_bitmap_and_preserves_other_inputs() {
    let valid = tests::arguments("stroke", "source-over", 0.5, 127, None);
    for index in 0..valid.len() {
        if index == 0 || index == 4 || index == 11 {
            continue; // Null operation, absent clip and optional false are admitted.
        }
        let mut args = valid.clone();
        args[index] = JsValue::Null;
        let preserved: Vec<_> = args.iter().skip(2).cloned().collect();
        assert_eq!(paint_owned(&mut args), JsValue::Null, "argument {index}");
        assert_eq!(&args[2..], preserved.as_slice());
    }
    assert_eq!(paint_owned(&mut []), JsValue::Null);
    for length in 0..valid.len() {
        assert_eq!(paint_owned(&mut valid[..length].to_vec()), JsValue::Null);
    }
}
