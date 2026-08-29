use better_web_browser::engine::css::Color;
use better_web_browser::engine::{DisplayItem, FontSpec, Page, RectF, TextMeasurer, layout_page};

struct FixedMeasurer;

impl TextMeasurer for FixedMeasurer {
    fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
        (text.chars().count() as f32 * font.size * 0.5, font.size)
    }
}

#[test]
fn fullscreen_element_becomes_the_only_viewport_layout_root() {
    let page = Page::parse(
        r#"<p>outside before</p>
           <section id="player" style="background-color:#123456"><p>fullscreen content</p></section>
           <p>outside after</p>"#,
        "https://example.test/",
    );
    let player = page.dom.elements_named("section").next().unwrap();
    player.set_fullscreen(true);

    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 640.0, 360.0, &mut measurer);
    let text = output
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    assert!(text.contains("fullscreen content"), "{text}");
    assert!(!text.contains("outside before"), "{text}");
    assert!(!text.contains("outside after"), "{text}");
    assert_eq!(output.background, Color::BLACK);
    assert_eq!(
        output.node_bounds[&player.id()],
        RectF {
            x: 0.0,
            y: 0.0,
            width: 640.0,
            height: 360.0,
        }
    );
}
