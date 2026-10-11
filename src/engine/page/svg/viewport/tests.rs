use super::*;
use crate::engine::css::media::MediaEnvironment;
use crate::engine::dom;
mod benchmark;
mod lifetime_tests;
mod raster_tests;

const URL: &str = "https://example.test/";

struct Measurer;
impl crate::engine::TextMeasurer for Measurer {
    fn measure(&mut self, text: &str, font: &crate::engine::FontSpec) -> (f32, f32) {
        (text.len() as f32 * font.size * 0.5, font.size)
    }
}

fn live(markup: &str, density: f32) -> Page {
    let mut page = Page::from_dom(dom::parse(markup), URL);
    page.enable_inline_svg_viewports();
    page.set_media_environment(MediaEnvironment::new(800., 600., density, false));
    page
}

fn layout(page: &mut Page) -> LayoutOutput {
    page.refresh_resources_for_viewport(800., 600.);
    let output = crate::engine::layout_page(page, 800., 600., &mut Measurer);
    page.prepare_inline_svg_images(&output);
    output
}

fn svg(page: &Page) -> NodeRef {
    page.dom.elements_named("svg").next().unwrap()
}

fn image(page: &Page) -> &DecodedImage {
    &page.images[&inline_svg_key(&svg(page))]
}

fn pixel(image: &DecodedImage, x: u32, y: u32) -> &[u8] {
    let offset = (y as usize * image.width as usize + x as usize) * 4;
    &image.bgra[offset..offset + 4]
}

#[test]
fn live_svg_defers_pixels_until_layout_then_uses_display_size_and_density() {
    let mut page = live(
        "<svg width=700 height=330 viewBox='0 0 700 330' style='display:block;width:140px;height:66px'><rect width=700 height=330 fill=red /></svg>",
        1.25,
    );
    page.refresh_resources_for_viewport(800., 600.);
    let key = inline_svg_key(&svg(&page));
    assert!(
        !page.images.contains_key(&key),
        "no natural-size filter/raster before layout"
    );
    assert_eq!(page.layout_image_dimensions(&key), Some((700, 330)));
    let output = crate::engine::layout_page(&page, 800., 600., &mut Measurer);
    assert!(output.items.iter().any(|item| matches!(item, DisplayItem::Image { rect, url, .. } if url == &key && rect.width == 140. && rect.height == 66.)));
    page.prepare_inline_svg_images(&output);
    assert_eq!((image(&page).width, image(&page).height), (175, 83));
    assert_eq!(pixel(image(&page), 174, 82), [0, 0, 255, 255]);
    assert_eq!(page.layout_image_dimensions(&key), Some((700, 330)));
    assert!(page.has_image_update(&key));
}

#[test]
fn css_viewport_is_not_a_nonuniform_scale_of_the_intrinsic_bitmap() {
    for (aspect, expected_left, expected_top) in [
        ("xMidYMid meet", false, true),
        ("xMinYMin meet", true, true),
        ("xMidYMid slice", true, true),
        ("none", true, true),
    ] {
        let source = format!(
            "<svg width=40 height=40 viewBox='0 0 40 40' preserveAspectRatio='{aspect}' style='display:block;width:80px;height:40px'><rect width=40 height=40 fill=red /></svg>"
        );
        let mut page = live(&source, 1.);
        layout(&mut page);
        assert_eq!((image(&page).width, image(&page).height), (80, 40));
        assert_eq!(
            pixel(image(&page), 0, 20)[3] != 0,
            expected_left,
            "{aspect}"
        );
        assert_eq!(pixel(image(&page), 30, 0)[3] != 0, expected_top, "{aspect}");
        assert_eq!(
            pixel(image(&page), 79, 20)[3] != 0,
            matches!(aspect, "none" | "xMidYMid slice"),
            "{aspect}"
        );
    }
}

#[test]
fn without_viewbox_user_units_and_percentages_use_the_css_viewport() {
    let mut page = live(
        "<svg width=100 height=50 style='display:block;width:200px;height:100px'><rect width='50%' height='50%' fill=red /><rect x=150 y=75 width=25 height=10 fill=blue /></svg>",
        2.,
    );
    layout(&mut page);
    let image = image(&page);
    assert_eq!((image.width, image.height), (400, 200));
    assert_eq!(pixel(image, 199, 99), [0, 0, 255, 255]);
    assert_eq!(pixel(image, 200, 100), [0, 0, 0, 0]);
    assert_eq!(pixel(image, 310, 160), [255, 0, 0, 255]);
}

#[test]
fn used_viewport_does_not_rewrite_root_attributes_used_by_descendant_rules() {
    let mut page = live(
        "<svg width=100 height=100 viewBox='0 0 100 100' style='display:block;width:40px;height:40px'><style>svg[width='100'] rect{fill:blue}</style><rect width=100 height=100 /></svg>",
        1.,
    );
    layout(&mut page);
    assert_eq!(pixel(image(&page), 20, 20), [255, 0, 0, 255]);
    assert_eq!(svg(&page).attr("width").as_deref(), Some("100"));
}

#[test]
fn cssom_snapshots_keep_intrinsic_dimensions_without_source_or_pixels() {
    let mut page = live(
        "<svg width=100 height=50 viewBox='0 0 100 50' style='display:block;width:40px'><rect width=100 height=50 fill=red /></svg>",
        2.,
    );
    let full = layout(&mut page);
    let mut snapshot = page.layout_snapshot();
    let key = inline_svg_key(&svg(&page));
    assert_eq!(page.layout_image_dimensions(&key), Some((100, 50)));
    assert_eq!(snapshot.layout_image_dimensions(&key), Some((100, 50)));
    assert!(snapshot.inline_svgs.entries.is_empty());
    assert!(snapshot.images[&key].bgra.is_empty());
    let geometry = crate::engine::layout_page(&snapshot, 800., 600., &mut Measurer);
    assert_eq!(geometry.node_bounds, full.node_bounds);
    svg(&page).set_attr("style", "display:block;width:80px");
    let full = layout(&mut page);
    snapshot.synchronize_layout_snapshot(&page);
    let geometry = crate::engine::layout_page(&snapshot, 800., 600., &mut Measurer);
    assert_eq!(geometry.node_bounds, full.node_bounds);
    assert_eq!(snapshot.layout_image_dimensions(&key), Some((100, 50)));
}

#[test]
fn blob_and_standalone_image_decoding_retain_the_natural_pixel_contract() {
    let source = "<svg xmlns='http://www.w3.org/2000/svg' width='700' height='330' viewBox='0 0 700 330'><rect width='700' height='330' fill='red'/></svg>";
    let decoded = super::super::decode_svg(source.as_bytes(), "SVG image snapshot").unwrap();
    assert_eq!((decoded.width, decoded.height), (700, 330));
    let page = Page::parse(source, URL);
    assert_eq!((image(&page).width, image(&page).height), (700, 330));
    assert!(!page.inline_svgs.enabled);
}
