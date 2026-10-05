use super::{DecodedImage, MAX_INLINE_SVGS, Page, bounded_utf8_prefix};
use crate::engine::css::StyleSet;
use crate::engine::dom::{Node, NodeRef};
use crate::limits::MAX_SVG_SOURCE_BYTES;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
mod decoder;
mod expansion;
#[cfg(test)]
mod reference_tests;
mod references;
mod serialize;
#[cfg(test)]
mod tests;
mod urls;

const MAX_INLINE_SVG_DIAGNOSTICS: usize = 8;
const MAX_INLINE_SVG_DIAGNOSTIC_BYTES: usize = 512;

impl Page {
    /// Caller has proved an exact selector-only color/decoration change, with
    /// no DOM/rule/removal work. Existing SVG currentColor rasters may change;
    /// an empty already-discovered cache cannot acquire a new SVG in this case.
    pub(crate) fn refresh_inline_svg_colors(&mut self) {
        if !self.inline_svg_versions.is_empty() {
            self.refresh_inline_svgs();
        }
    }

    pub(super) fn refresh_inline_svgs(&mut self) {
        let svgs = Node::shadow_including_descendants(&self.dom.document)
            .filter(|node| node.tag_name() == Some("svg"))
            .take(MAX_INLINE_SVGS)
            .collect::<Vec<_>>();
        let active_ids = svgs.iter().map(|svg| svg.id()).collect::<HashSet<_>>();
        let active_keys = svgs.iter().map(inline_svg_key).collect::<HashSet<_>>();
        self.inline_svg_versions
            .retain(|node, _| active_ids.contains(node));
        self.images
            .retain(|key, _| !key.starts_with("inline-svg:") || active_keys.contains(key));

        for svg in svgs {
            let key = inline_svg_key(&svg);
            let styles = self.cached_styles.as_ref().map(|(_, _, styles)| styles);
            let version = inline_svg_version(&svg, styles);
            let changed = self.inline_svg_versions.get(&svg.id()).copied() != Some(version);
            if !changed {
                continue;
            }
            self.inline_svg_versions.insert(svg.id(), version);
            match decode_inline_svg(&svg, styles) {
                Ok(image) => {
                    let _ = self.install_decoded_image(key, image);
                }
                Err(error) => {
                    self.images.remove(&key);
                    if self
                        .diagnostics
                        .iter()
                        .filter(|message| message.starts_with("inline SVG "))
                        .count()
                        < MAX_INLINE_SVG_DIAGNOSTICS
                    {
                        let message = format!("inline SVG {:032x}: {error}", svg.id().to_wire());
                        self.diagnostics.push(
                            bounded_utf8_prefix(&message, MAX_INLINE_SVG_DIAGNOSTIC_BYTES)
                                .0
                                .to_string(),
                        );
                    }
                }
            }
        }
    }
}

pub(crate) fn inline_svg_key(node: &NodeRef) -> String {
    format!("inline-svg:{:032x}", node.id().to_wire())
}

pub(super) fn inline_svg_version(node: &NodeRef, styles: Option<&StyleSet>) -> u64 {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    // Key the raster by its actual decoder input. DOM mutation generations also
    // advance for animation samples (including compositor opacity/transform)
    // that do not change this SVG drawing. Re-rasterizing filters for every such
    // sample stalls the rendering checkpoint and the document's script tasks.
    // Serialization includes author attributes, text and resolved currentColor,
    // so real drawing changes still invalidate the cached pixels.
    serialize::source(node, styles).hash(&mut hash);
    hash.finish()
}

pub(super) fn decode_inline_svg(
    node: &NodeRef,
    styles: Option<&StyleSet>,
) -> Result<DecodedImage, String> {
    let source = serialize::source(node, styles)?;
    decode_svg(source.as_bytes(), "inline SVG")
}

pub(crate) fn looks_like_svg(bytes: &[u8]) -> bool {
    let prefix = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]);
    let prefix = prefix.trim_start_matches('\u{feff}').trim_start();
    prefix.starts_with("<svg") || (prefix.starts_with("<?xml") && prefix.contains("<svg"))
}

pub(crate) fn decode_svg(source: &[u8], description: &str) -> Result<DecodedImage, String> {
    decode_svg_with_limits(
        source,
        description,
        crate::engine::image_decode::DecodeLimits::PAGE,
    )
}

pub(crate) fn decode_svg_with_limits(
    source: &[u8],
    description: &str,
    limits: crate::engine::image_decode::DecodeLimits,
) -> Result<DecodedImage, String> {
    if source.len() > MAX_SVG_SOURCE_BYTES {
        return Err(format!(
            "{description} exceeds the {MAX_SVG_SOURCE_BYTES}-byte limit"
        ));
    }
    let source = decoder::payload(source)?;
    let options = decoder::options(limits);
    let tree = resvg::usvg::Tree::from_data(&source, &options)
        .map_err(|error| format!("parse {description}: {error}"))?;
    let size = tree.size().to_int_size();
    let width = size.width();
    let height = size.height();
    limits.rgba_len(width, height)?;
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
        bgra: bgra.into(),
    })
}
