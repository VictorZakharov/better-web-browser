use super::*;
use crate::engine::{FontSpec, TextMeasurer};

struct Measurer;
impl TextMeasurer for Measurer {
    fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
        std::thread::sleep(Duration::from_millis(1));
        (text.len() as f32 * font.size, font.size)
    }
    fn normal_line_height(&mut self, font: &FontSpec) -> f32 {
        font.size * 1.5
    }
    fn layout_cache_epoch(&self) -> Option<u64> {
        Some(7)
    }
}

fn font() -> FontSpec {
    FontSpec {
        family: "sans-serif".into(),
        size: 12.0,
        weight: 400,
        italic: false,
        underline: false,
        letter_spacing: 0.0,
        word_spacing: 0.0,
        rtl: false,
        kerning: true,
        variants: Default::default(),
        features: Default::default(),
    }
}

#[test]
fn geometry_text_timing_is_opt_in_and_preserves_measurements() {
    let font = font();
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

#[test]
fn script_geometry_measures_and_retains_layouts_like_rendering() {
    let mut inner = Measurer;
    let mut measured = GeometryTextMeasurer {
        inner: &mut inner,
        profile: false,
        elapsed: Duration::ZERO,
    };
    assert_eq!(measured.normal_line_height(&font()), 18.0);
    assert_eq!(measured.layout_cache_epoch(), Some(7));
}
