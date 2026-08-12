use super::css::StyleSet;
use super::dom::{self, Dom, Node, NodeData, NodeRef};
use crate::navigation::resolve_url;
use image::ImageReader;
use std::collections::{HashMap, HashSet};
use std::io::Cursor;

const MAX_STYLESHEETS: usize = 16;
const MAX_IMAGES: usize = 64;
const MAX_DECODED_IMAGE_PIXELS: u64 = 32 * 1024 * 1024;
const MAX_INLINE_SVGS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageResource {
    Stylesheet { url: String },
    Image { url: String },
}

#[derive(Debug)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

#[derive(Debug)]
pub struct Page {
    pub dom: Dom,
    pub title: String,
    pub source_url: String,
    pub resources: Vec<PageResource>,
    pub external_stylesheets: Vec<String>,
    pub images: HashMap<String, DecodedImage>,
}

impl Page {
    pub fn parse(html: &str, source_url: &str) -> Self {
        let dom = dom::parse(html);
        let title = dom.title();
        let base_url = dom
            .elements_named("base")
            .find_map(|node| node.attr("href"))
            .and_then(|href| resolve_url(source_url, &href))
            .unwrap_or_else(|| source_url.to_string());

        let mut resources = Vec::new();
        let mut seen_stylesheets = HashSet::new();
        for link in dom.elements_named("link") {
            let rel = link.attr("rel").unwrap_or_default();
            if !rel
                .split_ascii_whitespace()
                .any(|token| token.eq_ignore_ascii_case("stylesheet"))
            {
                continue;
            }
            if resources
                .iter()
                .filter(|resource| matches!(resource, PageResource::Stylesheet { .. }))
                .count()
                >= MAX_STYLESHEETS
            {
                break;
            }
            if let Some(url) = link
                .attr("href")
                .and_then(|href| resolve_url(&base_url, &href))
                && seen_stylesheets.insert(url.clone())
            {
                resources.push(PageResource::Stylesheet { url });
            }
        }

        let mut seen_images = HashSet::new();
        for node in Node::descendants(&dom.document) {
            if !matches!(node.tag_name(), Some("img" | "image")) {
                continue;
            }
            if seen_images.len() >= MAX_IMAGES {
                break;
            }
            if let Some(url) = node
                .attr("src")
                .or_else(|| node.attr("href"))
                .and_then(|src| resolve_url(&base_url, &src))
                && seen_images.insert(url.clone())
            {
                resources.push(PageResource::Image { url });
            }
        }

        let mut images = HashMap::new();
        for svg in dom.elements_named("svg").take(MAX_INLINE_SVGS) {
            if let Ok(image) = decode_inline_svg(&svg) {
                images.insert(inline_svg_key(&svg), image);
            }
        }

        Self {
            dom,
            title,
            source_url: source_url.to_string(),
            resources,
            external_stylesheets: Vec::new(),
            images,
        }
    }

    pub fn add_stylesheet(&mut self, css: String) {
        self.external_stylesheets.push(css);
    }

    pub fn add_image(&mut self, url: String, bytes: &[u8]) -> Result<(), String> {
        let reader = ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|error| format!("detect image format: {error}"))?;
        let image = reader
            .decode()
            .map_err(|error| format!("decode image: {error}"))?;
        let width = image.width();
        let height = image.height();
        if u64::from(width) * u64::from(height) > MAX_DECODED_IMAGE_PIXELS {
            return Err(format!("image is too large: {width}×{height}"));
        }
        let rgba = image.into_rgba8();
        let mut bgra = rgba.into_raw();
        for pixel in bgra.chunks_exact_mut(4) {
            let alpha = u16::from(pixel[3]);
            pixel[0] = ((u16::from(pixel[0]) * alpha + 127) / 255) as u8;
            pixel[1] = ((u16::from(pixel[1]) * alpha + 127) / 255) as u8;
            pixel[2] = ((u16::from(pixel[2]) * alpha + 127) / 255) as u8;
            pixel.swap(0, 2);
        }
        self.images.insert(
            url,
            DecodedImage {
                width,
                height,
                bgra,
            },
        );
        Ok(())
    }

    pub fn style(&self, viewport_width: f32) -> StyleSet {
        StyleSet::from_dom(&self.dom, &self.external_stylesheets, viewport_width)
    }
}

pub(crate) fn inline_svg_key(node: &NodeRef) -> String {
    format!("inline-svg:{:x}", std::rc::Rc::as_ptr(node) as usize)
}

fn decode_inline_svg(node: &NodeRef) -> Result<DecodedImage, String> {
    let mut source = String::new();
    serialize_svg_node(node, &mut source, true);
    let options = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(source.as_bytes(), &options)
        .map_err(|error| format!("parse inline SVG: {error}"))?;
    let size = tree.size().to_int_size();
    let width = size.width();
    let height = size.height();
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_DECODED_IMAGE_PIXELS
    {
        return Err("inline SVG has invalid dimensions".into());
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| "allocate inline SVG pixels".to_string())?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );
    let mut bgra = pixmap.take();
    for pixel in bgra.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Ok(DecodedImage {
        width,
        height,
        bgra,
    })
}

fn serialize_svg_node(node: &NodeRef, output: &mut String, root: bool) {
    match &node.data {
        NodeData::Element(element) => {
            let tag = element.name.local.as_ref();
            if tag.eq_ignore_ascii_case("script") {
                return;
            }
            output.push('<');
            output.push_str(tag);
            let attrs = element.attrs.borrow();
            let has_xmlns = attrs
                .iter()
                .any(|attribute| attribute.name.local.as_ref() == "xmlns");
            if root && !has_xmlns {
                output.push_str(" xmlns=\"http://www.w3.org/2000/svg\"");
            }
            for attribute in attrs.iter() {
                output.push(' ');
                output.push_str(attribute.name.local.as_ref());
                output.push_str("=\"");
                escape_xml(&attribute.value, output);
                output.push('"');
            }
            output.push('>');
            drop(attrs);
            for child in node.children.borrow().iter() {
                serialize_svg_node(child, output, false);
            }
            output.push_str("</");
            output.push_str(tag);
            output.push('>');
        }
        NodeData::Text(text) => escape_xml(&text.borrow(), output),
        _ => {}
    }
}

fn escape_xml(value: &str, output: &mut String) {
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&apos;"),
            character => output.push(character),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_and_resolves_page_resources() {
        let page = Page::parse(
            r#"
                <base href="https://cdn.example/assets/">
                <link rel="alternate stylesheet" href="theme.css">
                <img src="logo.png"><img src="logo.png">
            "#,
            "https://example.com/start",
        );
        assert_eq!(
            page.resources,
            vec![
                PageResource::Stylesheet {
                    url: "https://cdn.example/assets/theme.css".into()
                },
                PageResource::Image {
                    url: "https://cdn.example/assets/logo.png".into()
                }
            ]
        );
    }

    #[test]
    fn decodes_images_to_bgra() {
        let mut page = Page::parse("", "https://example.com/");
        let source = image::RgbaImage::from_pixel(1, 1, image::Rgba([12, 34, 56, 255]));
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(source)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        page.add_image("https://example.com/a.png".into(), png.get_ref())
            .unwrap();
        let image = &page.images["https://example.com/a.png"];
        assert_eq!((image.width, image.height), (1, 1));
        assert_eq!(image.bgra, vec![56, 34, 12, 255]);
    }

    #[test]
    fn rasterizes_inline_svg_without_a_browser_runtime() {
        let page = Page::parse(
            r#"<svg viewBox="0 0 24 24"><path d="M4 4h16v16H4z"/></svg>"#,
            "https://example.com/",
        );
        let svg = page.dom.elements_named("svg").next().unwrap();
        let image = &page.images[&inline_svg_key(&svg)];
        assert_eq!((image.width, image.height), (24, 24));
        assert!(image.bgra.chunks_exact(4).any(|pixel| pixel[3] != 0));
    }
}
