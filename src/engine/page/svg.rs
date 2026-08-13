use super::{DecodedImage, MAX_DECODED_IMAGE_PIXELS};
use crate::engine::dom::{NodeData, NodeRef};

pub(crate) fn inline_svg_key(node: &NodeRef) -> String {
    format!("inline-svg:{:032x}", node.id().to_wire())
}

pub(super) fn decode_inline_svg(node: &NodeRef) -> Result<DecodedImage, String> {
    let mut source = String::new();
    serialize_svg_node(node, &mut source, true);
    decode_svg(source.as_bytes(), "inline SVG")
}

pub(super) fn looks_like_svg(bytes: &[u8]) -> bool {
    let prefix = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]);
    let prefix = prefix.trim_start_matches('\u{feff}').trim_start();
    prefix.starts_with("<svg") || (prefix.starts_with("<?xml") && prefix.contains("<svg"))
}

pub(super) fn decode_svg(source: &[u8], description: &str) -> Result<DecodedImage, String> {
    let options = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(source, &options)
        .map_err(|error| format!("parse {description}: {error}"))?;
    let size = tree.size().to_int_size();
    let width = size.width();
    let height = size.height();
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_DECODED_IMAGE_PIXELS
    {
        return Err(format!("{description} has invalid dimensions"));
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| format!("allocate {description} pixels"))?;
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
