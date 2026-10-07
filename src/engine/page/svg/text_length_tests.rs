//! Independent rectangular Ahem outlines make length/placement assertions exact.
//! The existing WPT Ahem fixture is CC0; it is test-only, never a shipped fallback.
use super::*;
use crate::engine::font::WebFont;

fn render(content: &str) -> DecodedImage {
    let font = WebFont {
        family: "LengthFixture".into(),
        weight: 400,
        italic: false,
        sfnt: include_bytes!("../../../../tests/canvas/fonts/ahem.ttf")
            .as_slice()
            .into(),
        source_url: "test-font:length".into(),
        script_source_id: None,
        unicode_ranges: Default::default(),
        features: Default::default(),
    };
    let source = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='240' height='70'><g font-family='LengthFixture' font-size='20'>{content}</g></svg>"
    );
    decode_svg_with_fonts(
        source.as_bytes(),
        "length fixture",
        crate::engine::image_decode::DecodeLimits::PAGE,
        &[font],
    )
    .unwrap()
}

fn pixel(image: &DecodedImage, x: usize, y: usize) -> &[u8] {
    let index = (y * image.width as usize + x) * 4;
    &image.bgra[index..index + 4]
}

fn assert_rectangles(image: &DecodedImage, rectangles: &[(usize, usize, [u8; 4])]) {
    for y in 0..image.height as usize {
        for x in 0..image.width as usize {
            let expected = if (24..44).contains(&y) {
                rectangles
                    .iter()
                    .find(|(left, right, _)| (*left..*right).contains(&x))
                    .map(|(_, _, color)| *color)
                    .unwrap_or([0; 4])
            } else {
                [0; 4]
            };
            assert_eq!(pixel(image, x, y), expected, "pixel ({x},{y})");
        }
    }
}

#[test]
fn parent_glyph_scaling_covers_all_painted_spans_once() {
    let image = render(
        "<text x='10' y='40' textLength='120' lengthAdjust='spacingAndGlyphs'><tspan fill='red'>X</tspan><tspan fill='blue'>YZ</tspan></text>",
    );
    assert_rectangles(
        &image,
        &[(10, 50, [0, 0, 255, 255]), (50, 130, [255, 0, 0, 255])],
    );
}

#[test]
fn parent_spacing_counts_glyphs_across_style_boundaries() {
    let image = render(
        "<text x='10' y='40' textLength='120'><tspan fill='red'>X</tspan><tspan fill='blue'>YZ</tspan></text>",
    );
    assert_rectangles(
        &image,
        &[
            (10, 30, [0, 0, 255, 255]),
            (60, 80, [255, 0, 0, 255]),
            (110, 130, [255, 0, 0, 255]),
        ],
    );
}

#[test]
fn explicit_child_length_is_one_parent_spacing_unit() {
    let image = render(
        "<text x='10' y='40' textLength='140'>X<tspan textLength='80' lengthAdjust='spacingAndGlyphs' fill='blue'>YZ</tspan>W</text>",
    );
    assert_rectangles(
        &image,
        &[
            (10, 30, [0, 0, 0, 255]),
            (40, 120, [255, 0, 0, 255]),
            (130, 150, [0, 0, 0, 255]),
        ],
    );
}

#[test]
fn deep_unadjusted_tspans_do_not_lose_the_ancestor_range() {
    let direct =
        render("<text x='10' y='40' textLength='120' lengthAdjust='spacingAndGlyphs'>XYZ</text>");
    let nested = render(
        "<text x='10' y='40' textLength='120' lengthAdjust='spacingAndGlyphs'><tspan><tspan>X</tspan>Y<tspan>Z</tspan></tspan></text>",
    );
    assert_eq!(direct.bgra, nested.bgra);
    assert_rectangles(&nested, &[(10, 130, [0, 0, 0, 255])]);
}

#[test]
fn invalid_child_lengths_do_not_create_atomic_parent_units() {
    let expected = render("<text x='10' y='40' textLength='120'>XYZ</text>");
    for invalid in ["-1", "oops"] {
        let source = format!(
            "<text x='10' y='40' textLength='120'>X<tspan textLength='{invalid}'>YZ</tspan></text>"
        );
        assert_eq!(render(&source).bgra, expected.bgra, "invalid={invalid}");
    }
}

#[test]
fn anchors_use_adjusted_width_not_natural_or_per_span_width() {
    let end = render(
        "<text x='130' y='40' text-anchor='end' textLength='120' lengthAdjust='spacingAndGlyphs'><tspan>X</tspan><tspan>YZ</tspan></text>",
    );
    let middle = render(
        "<text x='70' y='40' text-anchor='middle' textLength='120' lengthAdjust='spacingAndGlyphs'><tspan>X</tspan><tspan>YZ</tspan></text>",
    );
    assert_rectangles(&end, &[(10, 130, [0, 0, 0, 255])]);
    assert_eq!(end.bgra, middle.bgra);
}

#[test]
fn zero_target_collapses_glyphs_without_non_finite_geometry() {
    let image = render(
        "<text x='10' y='40' textLength='0' lengthAdjust='spacingAndGlyphs'><tspan>XYZ</tspan></text>",
    );
    assert!(image.bgra.iter().all(|byte| *byte == 0));
}

#[test]
fn nested_character_ranges_are_unicode_scalars_not_utf8_byte_offsets() {
    let image = render(
        "<text x='10' y='40' textLength='140'>é<tspan textLength='80' lengthAdjust='spacingAndGlyphs' fill='blue'>YZ</tspan>W</text>",
    );
    assert_rectangles(
        &image,
        &[
            (10, 30, [0, 0, 0, 255]),
            (40, 120, [255, 0, 0, 255]),
            (130, 150, [0, 0, 0, 255]),
        ],
    );
}

#[test]
fn parent_glyph_scaling_preserves_a_resolved_childs_explicit_length() {
    let image = render(
        "<text x='10' y='40' textLength='160' lengthAdjust='spacingAndGlyphs'>X<tspan textLength='80' lengthAdjust='spacingAndGlyphs' fill='blue'>YZ</tspan>W</text>",
    );
    assert_rectangles(
        &image,
        &[
            (10, 50, [0, 0, 0, 255]),
            (50, 130, [255, 0, 0, 255]),
            (130, 170, [0, 0, 0, 255]),
        ],
    );
}

#[test]
fn a_parent_cannot_rescale_a_wholly_resolved_child_or_reflect_its_siblings() {
    let owned = render(
        "<text x='10' y='40'><tspan textLength='80' lengthAdjust='spacingAndGlyphs'>YZ</tspan></text>",
    );
    let parent = render(
        "<text x='10' y='40' textLength='160' lengthAdjust='spacingAndGlyphs'><tspan textLength='80' lengthAdjust='spacingAndGlyphs'>YZ</tspan></text>",
    );
    assert_eq!(owned.bgra, parent.bgra);
    let owned = render(
        "<text x='10' y='40'>X<tspan textLength='80' lengthAdjust='spacingAndGlyphs'>YZ</tspan>W</text>",
    );
    let parent = render(
        "<text x='10' y='40' textLength='40' lengthAdjust='spacingAndGlyphs'>X<tspan textLength='80' lengthAdjust='spacingAndGlyphs'>YZ</tspan>W</text>",
    );
    assert_eq!(owned.bgra, parent.bgra);
}
