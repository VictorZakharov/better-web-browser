use super::{DecodedImage, MAX_INLINE_SVGS, Page, bounded_utf8_prefix};
use crate::engine::css::StyleSet;
use crate::engine::dom::{Node, NodeId, NodeRef};
use crate::limits::MAX_SVG_SOURCE_BYTES;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
mod admission;
#[cfg(test)]
mod compositing_tests;
mod decoder;
#[cfg(test)]
mod deferred_tests;
mod embedded_fonts;
mod expansion;
mod fonts;
#[cfg(test)]
mod paint_cascade_tests;
#[cfg(test)]
mod publication_tests;
#[cfg(test)]
mod reference_tests;
mod references;
mod serialize;
#[cfg(test)]
mod tests;
#[cfg(all(test, windows))]
mod text_length_tests;
#[cfg(all(test, windows))]
mod text_tests;
mod urls;
mod viewport;
pub(super) use viewport::State;

const MAX_INLINE_SVG_DIAGNOSTICS: usize = 8;
const MAX_INLINE_SVG_DIAGNOSTIC_BYTES: usize = 512;

impl Page {
    /// Caller has proved an exact selector-only color/decoration change, with
    /// no DOM/rule/removal work. Existing SVG currentColor rasters may change;
    /// an empty already-discovered cache cannot acquire a new SVG in this case.
    pub(crate) fn refresh_inline_svg_colors(&mut self) {
        if !self.inline_svgs.versions.is_empty() {
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
        self.inline_svgs.retain(&active_ids, &active_keys);
        self.images
            .retain(|key, _| !key.starts_with("inline-svg:") || active_keys.contains(key));
        self.image_updates
            .retain(|key| self.images.contains_key(key));

        let font_input = fonts::Input::new(&self.fonts);
        let mut reference_index = references::IndexCache::default();
        for svg in svgs {
            let key = inline_svg_key(&svg);
            let styles = self.cached_styles.as_ref().map(|(_, _, styles)| &**styles);
            if styles.is_some_and(|styles| super::render_visibility::suppressed(&svg, styles)) {
                // Keep the DOM and referenced definitions intact. No new stamp
                // is installed: revealing the subtree recomputes the complete
                // current source, inherited styles, references and font input.
                // https://www.w3.org/TR/SVG2/render.html#Rendered-vs-NonRendered
                continue;
            }
            let input = InlineSvgInput::with_index(&svg, styles, &font_input, &mut reference_index);
            let version = input.version;
            let changed = self.inline_svgs.versions.get(&svg.id()).copied() != Some(version);
            if !changed {
                continue;
            }
            self.inline_svgs.versions.insert(svg.id(), version);
            let result = if self.inline_svgs.can_admit(&key, &input) {
                self.inline_svgs
                    .prepare(svg.id(), key.clone(), &input)
                    .map(|()| false)
            } else {
                // Optional source-cache exhaustion retains the bounded natural
                // decoder. It never drops otherwise-supported SVG rendering.
                self.inline_svgs.forget(&key);
                input
                    .decode()
                    .and_then(|image| self.install_decoded_image(key.clone(), image))
                    .map(|()| true)
            };
            match result {
                Ok(rasterized) => {
                    if rasterized {
                        self.image_updates.insert(key);
                    }
                }
                Err(error) => {
                    self.inline_svgs.forget(&key);
                    self.images.remove(&key);
                    self.image_updates.remove(&key);
                    self.report_inline_svg_error(svg.id(), &error);
                }
            }
        }
    }

    fn report_inline_svg_error(&mut self, node: NodeId, error: &str) {
        if self
            .diagnostics
            .iter()
            .filter(|message| message.starts_with("inline SVG "))
            .count()
            < MAX_INLINE_SVG_DIAGNOSTICS
        {
            let message = format!("inline SVG {:032x}: {error}", node.to_wire());
            self.diagnostics.push(
                bounded_utf8_prefix(&message, MAX_INLINE_SVG_DIAGNOSTIC_BYTES)
                    .0
                    .to_string(),
            );
        }
    }
}

pub(crate) fn inline_svg_key(node: &NodeRef) -> String {
    format!("inline-svg:{:032x}", node.id().to_wire())
}

pub(super) struct InlineSvgInput {
    pub version: u64,
    source: Result<String, String>,
    fonts: std::sync::Arc<[crate::engine::font::WebFont]>,
}

impl InlineSvgInput {
    pub fn new(node: &NodeRef, styles: Option<&StyleSet>) -> Self {
        Self::with_fonts(node, styles, &[])
    }

    pub fn with_fonts(
        node: &NodeRef,
        styles: Option<&StyleSet>,
        fonts: &[crate::engine::font::WebFont],
    ) -> Self {
        Self::with_font_input(node, styles, &fonts::Input::new(fonts))
    }

    fn with_font_input(
        node: &NodeRef,
        styles: Option<&StyleSet>,
        font_input: &fonts::Input,
    ) -> Self {
        Self::with_index(
            node,
            styles,
            font_input,
            &mut references::IndexCache::default(),
        )
    }

    fn with_index(
        node: &NodeRef,
        styles: Option<&StyleSet>,
        font_input: &fonts::Input,
        reference_index: &mut references::IndexCache,
    ) -> Self {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        // Key the raster by its actual decoder input. DOM mutation generations also
        // advance for animation samples (including compositor opacity/transform)
        // that do not change this SVG drawing. Re-rasterizing filters for every such
        // sample stalls the rendering checkpoint and the document's script tasks.
        // Serialization includes author attributes, text and resolved currentColor,
        // so real drawing changes still invalidate the cached pixels.
        let source = serialize::source_with_index(node, styles, reference_index);
        source.hash(&mut hash);
        let has_text = source
            .as_ref()
            .is_ok_and(|source| source.contains("<text") || source.contains("<tspan"));
        let fonts = if has_text {
            font_input.version().hash(&mut hash);
            font_input.snapshot()
        } else {
            std::sync::Arc::default()
        };
        Self {
            version: hash.finish(),
            source,
            fonts,
        }
    }

    pub fn decode(self) -> Result<DecodedImage, String> {
        let source = self.source?;
        decode_svg_with_fonts(
            source.as_bytes(),
            "inline SVG",
            crate::engine::image_decode::DecodeLimits::PAGE,
            &self.fonts,
        )
    }
}

#[cfg(test)]
pub(super) fn decode_inline_svg(
    node: &NodeRef,
    styles: Option<&StyleSet>,
) -> Result<DecodedImage, String> {
    InlineSvgInput::new(node, styles).decode()
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
    decode_svg_with_fonts(source, description, limits, &[])
}

fn decode_svg_with_fonts(
    source: &[u8],
    description: &str,
    limits: crate::engine::image_decode::DecodeLimits,
    fonts: &[crate::engine::font::WebFont],
) -> Result<DecodedImage, String> {
    if source.len() > MAX_SVG_SOURCE_BYTES {
        return Err(format!(
            "{description} exceeds the {MAX_SVG_SOURCE_BYTES}-byte limit"
        ));
    }
    let source = decoder::payload(source)?;
    let mut options = decoder::options(limits);
    let document = admission::document(&source)?;
    let embedded = embedded_fonts::collect(&document);
    let fonts = fonts.iter().chain(&embedded).cloned().collect::<Vec<_>>();
    self::fonts::configure(&mut options, &fonts);
    let tree = resvg::usvg::Tree::from_xmltree(&document, &options)
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
