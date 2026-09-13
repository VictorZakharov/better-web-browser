use super::*;
use crate::engine::{Page, TextMeasurer, layout_page};

struct FixedMeasurer;
impl TextMeasurer for FixedMeasurer {
    fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
        (text.len() as f32 * font.size * 0.5, font.size)
    }
}

#[test]
fn all_border_edges_survive_layout_presentation_and_control_ipc() {
    let page = Page::parse(
        "<style>div,button,input{border:4px solid red;border-right-color:green;border-bottom-color:blue;border-left-color:transparent}</style><div>block</div><button>button</button><input value=text>",
        "https://example.test/",
    );
    let output = layout_page(&page, 600.0, 800.0, &mut FixedMeasurer);
    let expected = [
        Color::rgb(255, 0, 0),
        Color::rgb(0, 128, 0),
        Color::rgb(0, 0, 255),
        Color::TRANSPARENT,
    ];
    let mut presentation = sample();
    presentation.layout.items = output.items;
    let decoded = RendererPresentation::decode(&presentation.encode().unwrap()).unwrap();
    let mut borders = 0;
    let mut controls = 0;
    for item in &decoded.layout.items {
        match item {
            DisplayItem::BorderRect { colors, .. } => {
                assert_eq!(*colors, expected);
                borders += 1;
            }
            DisplayItem::Control(spec) => {
                assert_eq!(spec.border_colors, expected);
                controls += 1;
            }
            _ => {}
        }
    }
    assert!(borders >= 2, "{borders}");
    assert!(controls >= 2, "{controls}");
}
