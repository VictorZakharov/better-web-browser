//! Straight Canvas copies do not mutate raw GL pixels or author GL state.
use super::tests::command;
use super::*;

#[test]
fn canvas_snapshot_converts_once_and_keeps_independent_raw_copies() {
    for api in ["webgl1", "webgl2"] {
        for alpha in [false, true] {
            for premultiplied in [false, true] {
                let mut contexts = Contexts::default();
                let id = contexts.create(3, 2, &format!(
                    r#"{{"api":"{api}","alpha":{alpha},"premultiplied_alpha":{premultiplied},"preserve":true}}"#
                )).unwrap();
                command(
                    &mut contexts,
                    id,
                    "clearColor",
                    &[],
                    &[0.25, 0.125, 0., 0.5],
                    "",
                    None,
                );
                command(
                    &mut contexts,
                    id,
                    "clear",
                    &[gl::COLOR_BUFFER_BIT],
                    &[],
                    "",
                    None,
                );
                let raw = contexts.snapshot(id).unwrap();
                let first = contexts.canvas_snapshot(id).unwrap();
                assert_eq!((first.0, first.1), (3, 2));
                let expected = if !alpha {
                    [64, 32, 0, 255]
                } else if premultiplied {
                    [128, 64, 0, 128]
                } else {
                    [64, 32, 0, 128]
                };
                for pixel in first.2.chunks_exact(4) {
                    assert!(pixel.iter().zip(expected).all(|(&a, b)| a.abs_diff(b) <= 1));
                }
                assert_eq!(
                    contexts.snapshot(id).unwrap().2,
                    raw.2,
                    "raw buffer mutated"
                );
                assert_eq!(
                    contexts.canvas_snapshot(id).unwrap().2,
                    first.2,
                    "double recovery"
                );
                command(
                    &mut contexts,
                    id,
                    "clearColor",
                    &[],
                    &[0., 1., 0., 1.],
                    "",
                    None,
                );
                command(
                    &mut contexts,
                    id,
                    "clear",
                    &[gl::COLOR_BUFFER_BIT],
                    &[],
                    "",
                    None,
                );
                assert!(
                    contexts
                        .canvas_snapshot(id)
                        .unwrap()
                        .2
                        .chunks_exact(4)
                        .all(|pixel| pixel == [0, 255, 0, 255])
                );
                assert!(first.2.chunks_exact(4).all(|pixel| pixel[0] >= 63));
                assert_eq!(
                    command(&mut contexts, id, "getError", &[], &[], "", None),
                    json!(0)
                );
                contexts.remove(id);
                assert!(contexts.canvas_snapshot(id).is_none());
            }
        }
    }
}

#[test]
fn canvas_snapshot_recovers_zero_alpha_without_discarding_default_buffer_or_errors() {
    for premultiplied in [false, true] {
        let mut contexts = Contexts::default();
        let id = contexts
            .create(
                1,
                1,
                &format!(r#"{{"premultiplied_alpha":{premultiplied},"preserve":false}}"#),
            )
            .unwrap();
        command(
            &mut contexts,
            id,
            "clearColor",
            &[],
            &[1., 0.5, 0.25, 0.],
            "",
            None,
        );
        command(
            &mut contexts,
            id,
            "clear",
            &[gl::COLOR_BUFFER_BIT],
            &[],
            "",
            None,
        );
        command(&mut contexts, id, "enable", &[0xffff], &[], "", None);
        let raw = contexts.snapshot(id).unwrap();
        let canvas = contexts.canvas_snapshot(id).unwrap();
        assert_eq!(
            canvas.2,
            if premultiplied {
                vec![0, 0, 0, 0]
            } else {
                raw.2.clone()
            }
        );
        assert_eq!(contexts.snapshot(id).unwrap().2, raw.2);
        assert_eq!(
            command(&mut contexts, id, "getError", &[], &[], "", None),
            json!(gl::INVALID_ENUM)
        );
        assert_eq!(
            command(&mut contexts, id, "getError", &[], &[], "", None),
            json!(0)
        );
    }
}
