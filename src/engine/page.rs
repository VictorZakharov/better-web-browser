mod canvas;
pub(crate) use canvas::intrinsic_size as canvas_intrinsic_size;
mod element_images;
mod embedded;
mod font_loading;
mod font_requests;
mod integrity;
pub(crate) use integrity::{resource_integrity, stylesheet_crossorigin};
mod link_preloads;
mod media;
mod media_sources;
pub(crate) use media_sources::MediaSourceAdvance;
#[cfg(test)]
mod modern_image_tests;
mod parsing;
mod preload;
mod refresh;
mod rendering;
mod resource_events;
mod resource_hints;
mod resource_types;
mod responsive_images;
pub use resource_types::{MediaElementKind, PageResource, PageScript, PreloadAs};
mod resources;
pub(crate) use resources::prepare_script as prepare_written_script;
mod scripts;
mod snapshot;
mod stylesheets;
pub(crate) use stylesheets::{is_stylesheet, stylesheet_dependencies};
mod svg;

pub(crate) use self::media::MEDIA_VIDEO_PLACEHOLDER;
pub(crate) use self::preload::discover_script_preloads;
use self::resources::{discover_resources, document_base_url, resolve_image_url};
pub(crate) use self::svg::inline_svg_key;
use self::svg::{decode_svg, looks_like_svg};
pub(crate) use self::svg::{
    decode_svg_with_limits as decode_svg_image_with_limits, looks_like_svg as looks_like_svg_image,
};
use super::css::media::MediaEnvironment;
use super::css::{StyleRefreshStats, StyleSet};
use super::dom::{self, Dom, Node, NodeId, NodeRef};
use super::font::WebFont;
use super::script::{
    self, ScriptFetchOptions, ScriptInput, ScriptKind, ScriptOutcome, ScriptRuntime,
};
use crate::limits::{
    MAX_IMAGE_SOURCE_BYTES, MAX_INLINE_SVGS, MAX_PAGE_IMAGES as MAX_IMAGES, MAX_SCRIPT_BYTES,
    MAX_STYLE_IMAGES, MAX_WEB_FONTS, bounded_utf8_prefix,
};
use crate::navigation::resolve_url;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    /// Immutable decoded pixels shared by the owning page and in-flight presentations.
    ///
    /// Presentation encoding borrows these bytes. Cloning a resource must not duplicate a full
    /// image or video frame merely to cross the renderer's bounded outbound queue.
    pub bgra: Arc<[u8]>,
}

#[derive(Debug)]
pub struct Page {
    pub dom: Dom,
    pub title: String,
    pub source_url: String,
    pub character_set: String,
    base_url: String,
    pub resources: Vec<PageResource>,
    media_selections: HashMap<NodeId, media_sources::MediaSelection>,
    next_media_selection_id: u64,
    pub scripts: Vec<PageScript>,
    pub external_stylesheets: Vec<String>,
    stylesheet_sources: Vec<crate::engine::css::StylesheetSource>,
    stylesheet_discovery: Option<(u64, usize, MediaEnvironment)>,
    cached_styles: Option<(f32, f32, StyleSet)>,
    pub images: HashMap<String, DecodedImage>,
    image_origin_clean: HashMap<String, bool>,
    image_updates: HashSet<String>,
    scripting_enabled: bool,
    hidden_media_video: HashSet<NodeId>,
    inline_svg_versions: HashMap<NodeId, u64>,
    pub fonts: Vec<WebFont>,
    pub diagnostics: Vec<String>,
    media_environment: MediaEnvironment,
    layout_viewport: (f32, f32),
}

impl Page {
    pub fn parse(html: &str, source_url: &str) -> Self {
        Self::parse_internal(html, source_url, false)
    }

    pub fn parse_scripted(html: &str, source_url: &str) -> Self {
        Self::parse_internal(html, source_url, true)
    }

    fn parse_internal(html: &str, source_url: &str, scripting_enabled: bool) -> Self {
        let dom = dom::parse_with_scripting(html, scripting_enabled);
        if scripting_enabled {
            for node in dom.elements_named("noscript") {
                node.set_attr("style", "display: none");
            }
        }
        let mut page = Self::from_dom(dom, source_url);
        page.scripting_enabled = scripting_enabled;
        page
    }

    pub(crate) fn from_dom(dom: Dom, source_url: &str) -> Self {
        let title = dom.title();
        let diagnostics = dom
            .errors
            .borrow()
            .iter()
            .filter(|error| error.starts_with("safety limit:"))
            .cloned()
            .collect();
        let base_url = document_base_url(&dom, source_url);
        let media_environment = MediaEnvironment::new(1280.0, 720.0, 1.0, false);
        let (resources, scripts) =
            discover_resources(&dom, source_url, &base_url, media_environment);

        let mut images = HashMap::new();
        let mut inline_svg_versions = HashMap::new();
        for svg in Node::shadow_including_descendants(&dom.document)
            .filter(|node| node.tag_name() == Some("svg"))
            .take(MAX_INLINE_SVGS)
        {
            let input = svg::InlineSvgInput::new(&svg, None);
            inline_svg_versions.insert(svg.id(), input.version);
            if let Ok(image) = input.decode() {
                let _ =
                    media::install_initial_decoded_image(&mut images, inline_svg_key(&svg), image);
            }
        }
        media::install_placeholder(&dom, &mut images);

        let mut page = Self {
            dom,
            title,
            source_url: source_url.to_string(),
            character_set: "UTF-8".to_string(),
            base_url,
            resources,
            media_selections: HashMap::new(),
            next_media_selection_id: 1,
            scripts,
            external_stylesheets: Vec::new(),
            stylesheet_sources: Vec::new(),
            stylesheet_discovery: None,
            cached_styles: None,
            images,
            image_updates: HashSet::new(),
            image_origin_clean: HashMap::new(),
            scripting_enabled: true,
            hidden_media_video: HashSet::new(),
            inline_svg_versions,
            fonts: Vec::new(),
            diagnostics,
            media_environment,
            layout_viewport: (1280.0, 720.0),
        };
        page.refresh_media_sources();
        page.install_embedded_images();
        page.discover_stylesheet_dependencies();
        page
    }

    pub(crate) fn from_frame_document(
        document: NodeRef,
        source_url: &str,
        stylesheets: Vec<crate::engine::css::StylesheetSource>,
        quirks_mode: bool,
        media_environment: MediaEnvironment,
    ) -> Self {
        let quirks = if quirks_mode {
            html5ever::tree_builder::QuirksMode::Quirks
        } else {
            html5ever::tree_builder::QuirksMode::NoQuirks
        };
        let mut page = Self::from_dom(Dom::from_existing_document(document, quirks), source_url);
        page.stylesheet_sources = stylesheets;
        page.media_environment = media_environment;
        page
    }

    pub(crate) fn set_media_environment(&mut self, environment: MediaEnvironment) {
        if (self.media_environment.resolution_dppx - environment.resolution_dppx).abs() >= 0.01
            || self.media_environment.prefers_dark_color_scheme
                != environment.prefers_dark_color_scheme
        {
            self.cached_styles = None;
        }
        self.media_environment = environment;
    }

    pub(crate) fn set_layout_viewport(&mut self, width: f32, height: f32) {
        self.layout_viewport = (width, height);
    }

    pub fn add_stylesheet(&mut self, css: String) -> bool {
        let source_url = self.base_url.clone();
        self.add_stylesheet_from(&source_url, css)
    }

    pub fn add_stylesheet_from(&mut self, source_url: &str, css: String) -> bool {
        self.install_stylesheet(super::css::StylesheetSource::injected(source_url, css))
    }

    #[cfg(test)]
    pub(crate) fn add_linked_stylesheet(&mut self, source_url: &str, css: String) -> bool {
        self.install_stylesheet(super::css::StylesheetSource::linked(source_url, css))
    }

    pub fn add_script(
        &mut self,
        url: &str,
        kind: ScriptKind,
        fetch_options: ScriptFetchOptions,
        code: String,
    ) -> bool {
        if code.len() > MAX_SCRIPT_BYTES {
            self.diagnostics.push(format!(
                "script {url} exceeded the {MAX_SCRIPT_BYTES}-byte limit"
            ));
            return false;
        }
        let mut installed = false;
        for script in &mut self.scripts {
            if script.source_url == url
                && script.kind == kind
                && script.fetch_options == fetch_options
                && script.code.is_none()
            {
                script.code = Some(code.clone());
                installed = true;
            }
        }
        installed
    }

    pub fn immediate_refresh_url(&self) -> Option<String> {
        self.dom.elements_named("meta").find_map(|node| {
            let http_equiv = node.attr("http-equiv")?;
            if !http_equiv.trim().eq_ignore_ascii_case("refresh") {
                return None;
            }
            let content = node.attr("content")?;
            let target = refresh::parse_immediate_refresh_target(&content)?;
            resolve_url(&self.base_url, target)
        })
    }

    pub fn add_image(&mut self, url: String, bytes: &[u8]) -> Result<(), String> {
        // Replacement bytes cannot inherit an earlier response's readback authority.
        self.image_origin_clean.remove(&url);
        if bytes.len() > MAX_IMAGE_SOURCE_BYTES {
            return Err(format!(
                "image source exceeds the {MAX_IMAGE_SOURCE_BYTES}-byte limit"
            ));
        }
        if looks_like_svg(bytes) {
            let image = decode_svg(bytes, "external SVG")?;
            return self.install_decoded_image(url, image);
        }
        let image = super::image_decode::decode(
            bytes,
            super::image_decode::DecodeLimits::PAGE,
            Default::default(),
        )?;
        self.install_decoded_image(url, image.into_premultiplied_bgra())
    }

    pub fn image_url(&self, node: &NodeRef) -> Option<String> {
        if node.tag_name() == Some("canvas") {
            return canvas::image_url(self, node);
        }
        if node.tag_name() == Some("svg") {
            let key = inline_svg_key(node);
            return self.images.contains_key(&key).then_some(key);
        }
        if let Some(url) = media::image_url(self, node) {
            return Some(url);
        }
        resolve_image_url(node, &self.base_url, self.media_environment)
    }

    pub(crate) fn canvas_is_replaced(&self) -> bool {
        self.scripting_enabled
    }
}

#[cfg(test)]
mod tests;
