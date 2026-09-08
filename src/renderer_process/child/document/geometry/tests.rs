use super::*;
use crate::engine::{FontSpec, TextMeasurer};

#[test]
fn geometry_text_timing_is_opt_in_and_preserves_measurements() {
    struct Measurer;
    impl TextMeasurer for Measurer {
        fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
            std::thread::sleep(Duration::from_millis(1));
            (text.len() as f32 * font.size, font.size)
        }
    }
    let font = FontSpec {
        family: "sans-serif".into(),
        size: 12.0,
        weight: 400,
        italic: false,
        underline: false,
        letter_spacing: 0.0,
        word_spacing: 0.0,
    };
    for profile in [false, true] {
        let mut inner = Measurer;
        let mut measured = GeometryTextMeasurer {
            inner: &mut inner,
            profile,
            elapsed: Duration::ZERO,
        };
        assert_eq!(measured.measure("abc", &font), (36.0, 12.0));
        assert_eq!(measured.elapsed.is_zero(), !profile);
    }
}
