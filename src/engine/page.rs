use super::css::{Display, StyleSet, media_matches, parse_length};
use super::dom::{self, Dom, Node, NodeData, NodeRef};
use super::font::{WebFont, WebFontFace, decode_web_font, discover_font_faces};
use super::script::{self, ScriptInput, ScriptOutcome, ScriptRuntime};
use crate::navigation::resolve_url;
use image::ImageReader;
use std::collections::{HashMap, HashSet};
use std::io::Cursor;

const MAX_STYLESHEETS: usize = 16;
const MAX_IMAGES: usize = 64;
const MAX_SCRIPTS: usize = 64;
const MAX_DECODED_IMAGE_PIXELS: u64 = 32 * 1024 * 1024;
const MAX_INLINE_SVGS: usize = 64;
const MAX_WEB_FONTS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PageResource {
    Stylesheet {
        url: String,
    },
    Image {
        url: String,
    },
    Script {
        url: String,
    },
    Font {
        url: String,
        family: String,
        weight: u16,
        italic: bool,
    },
}

#[derive(Debug, Clone)]
pub struct PageScript {
    pub node: NodeRef,
    pub source_url: String,
    pub code: Option<String>,
    pub blocks_first_paint: bool,
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
    base_url: String,
    pub resources: Vec<PageResource>,
    pub scripts: Vec<PageScript>,
    pub external_stylesheets: Vec<String>,
    stylesheet_sources: Vec<(String, String)>,
    cached_styles: Option<(f32, StyleSet)>,
    pub images: HashMap<String, DecodedImage>,
    pub fonts: Vec<WebFont>,
    responsive_viewport_width: f32,
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
        let title = dom.title();
        let base_url = document_base_url(&dom, source_url);
        let responsive_viewport_width = 1280.0;
        let (resources, scripts) = discover_resources(&dom, &base_url, responsive_viewport_width);

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
            base_url,
            resources,
            scripts,
            external_stylesheets: Vec::new(),
            stylesheet_sources: Vec::new(),
            cached_styles: None,
            images,
            fonts: Vec::new(),
            responsive_viewport_width,
        }
    }

    pub fn add_stylesheet(&mut self, css: String) {
        let source_url = self.base_url.clone();
        self.add_stylesheet_from(&source_url, css);
    }

    pub fn add_stylesheet_from(&mut self, source_url: &str, css: String) {
        self.cached_styles = None;
        self.stylesheet_sources
            .push((source_url.to_string(), css.clone()));
        self.external_stylesheets.push(css);
    }

    pub fn add_font(
        &mut self,
        url: String,
        family: String,
        weight: u16,
        italic: bool,
        bytes: &[u8],
    ) -> Result<(), String> {
        if self.fonts.iter().any(|font| {
            font.family.eq_ignore_ascii_case(&family)
                && font.weight == weight
                && font.italic == italic
        }) {
            return Ok(());
        }
        let face = WebFontFace {
            family,
            weight,
            italic,
            url,
        };
        self.fonts.push(decode_web_font(&face, bytes)?);
        Ok(())
    }

    pub fn add_script(&mut self, url: &str, code: String) {
        for script in &mut self.scripts {
            if script.source_url == url && script.code.is_none() {
                script.code = Some(code.clone());
            }
        }
    }

    pub fn execute_scripts(&mut self) -> ScriptOutcome {
        self.execute_script_phase(false, None)
    }

    pub fn execute_first_paint_scripts(&mut self) -> ScriptOutcome {
        self.execute_script_phase(true, None)
    }

    pub fn execute_first_paint_scripts_with_loader(
        &mut self,
        dynamic_script_loader: &mut script::DynamicScriptLoader<'_>,
    ) -> ScriptOutcome {
        self.execute_script_phase(true, Some(dynamic_script_loader))
    }

    /// Starts the first-paint scripts and retains their realm for post-load work.
    ///
    /// The caller must keep the returned runtime on the same thread as this page. Both objects
    /// share the page DOM, allowing later timer callbacks to participate in style and layout
    /// invalidation without transferring a Boa context across threads.
    pub fn start_first_paint_script_runtime_with_loader(
        &mut self,
        dynamic_script_loader: &mut script::DynamicScriptLoader<'_>,
    ) -> (Option<ScriptRuntime>, ScriptOutcome) {
        self.start_script_phase(true, Some(dynamic_script_loader))
    }

    fn execute_script_phase(
        &mut self,
        first_paint_only: bool,
        dynamic_script_loader: Option<&mut script::DynamicScriptLoader<'_>>,
    ) -> ScriptOutcome {
        self.start_script_phase(first_paint_only, dynamic_script_loader)
            .1
    }

    fn start_script_phase(
        &mut self,
        first_paint_only: bool,
        dynamic_script_loader: Option<&mut script::DynamicScriptLoader<'_>>,
    ) -> (Option<ScriptRuntime>, ScriptOutcome) {
        self.cached_styles = None;
        let inputs = self
            .scripts
            .iter()
            .filter(|script| !first_paint_only || script.blocks_first_paint)
            .filter_map(|script| {
                script.code.as_ref().map(|code| ScriptInput {
                    node: script.node.clone(),
                    source_url: script.source_url.clone(),
                    code: code.clone(),
                    finish_lifecycle: true,
                })
            })
            .collect::<Vec<_>>();
        let (runtime, mut outcome) = if inputs.is_empty() {
            (None, ScriptOutcome::default())
        } else {
            let mut runtime = ScriptRuntime::new(self.dom.document.clone(), &self.source_url);
            let outcome = runtime.execute_initial_with_loader(&inputs, dynamic_script_loader);
            (runtime.is_active().then_some(runtime), outcome)
        };
        for missing in self
            .scripts
            .iter()
            .filter(|script| !first_paint_only || script.blocks_first_paint)
            .filter(|script| script.code.is_none())
        {
            outcome.errors.push(format!(
                "{}: script could not be loaded",
                missing.source_url
            ));
        }
        self.title = self.dom.title();
        (runtime, outcome)
    }

    pub fn resource_blocks_first_paint(&self, resource: &PageResource) -> bool {
        match resource {
            PageResource::Script { url } => self
                .scripts
                .iter()
                .any(|script| script.source_url.as_str() == url && script.blocks_first_paint),
            PageResource::Font { .. } => false,
            _ => true,
        }
    }

    pub fn refresh_resources(&mut self, viewport_width: f32) {
        self.base_url = document_base_url(&self.dom, &self.source_url);
        self.responsive_viewport_width = viewport_width.max(1.0);
        let (resources, _) =
            discover_resources(&self.dom, &self.base_url, self.responsive_viewport_width);
        for resource in resources {
            if !matches!(resource, PageResource::Script { .. })
                && !self.resources.contains(&resource)
            {
                self.resources.push(resource);
            }
        }
        let mut available_faces = Vec::new();
        for (source_url, css) in &self.stylesheet_sources {
            available_faces.extend(discover_font_faces(css, source_url));
        }
        let styles = StyleSet::from_sources(
            &self.dom,
            &self.base_url,
            &self.stylesheet_sources,
            viewport_width.max(1.0),
        );
        let mut known_images = self
            .resources
            .iter()
            .filter_map(|resource| match resource {
                PageResource::Image { url } => Some(url.clone()),
                _ => None,
            })
            .collect::<HashSet<_>>();
        let mut requested_faces = Vec::<(String, u16, bool)>::new();
        for node in Node::descendants(&self.dom.document) {
            let style = styles.get(&node);
            if style.display == Display::None || !style.visibility {
                continue;
            }
            if known_images.len() < MAX_IMAGES
                && let Some(url) = style.background_image.as_ref()
                && known_images.insert(url.clone())
            {
                self.resources
                    .push(PageResource::Image { url: url.clone() });
            }
            let family = style
                .font_family
                .split(',')
                .next()
                .unwrap_or("")
                .trim()
                .trim_matches(['\'', '"'])
                .to_ascii_lowercase();
            if !family.is_empty()
                && !requested_faces.iter().any(|(requested, weight, italic)| {
                    requested == &family && *weight == style.font_weight && *italic == style.italic
                })
            {
                requested_faces.push((family, style.font_weight, style.italic));
            }
        }
        let mut selected_faces = Vec::<WebFontFace>::new();
        for (family, weight, italic) in requested_faces {
            let Some(face) = available_faces
                .iter()
                .filter(|face| face.family.eq_ignore_ascii_case(&family))
                .min_by_key(|face| {
                    (
                        u8::from(face.italic != italic),
                        face.weight.abs_diff(weight),
                    )
                })
            else {
                continue;
            };
            if !selected_faces.contains(face) {
                selected_faces.push(face.clone());
            }
        }
        for face in selected_faces.into_iter().take(MAX_WEB_FONTS) {
            let resource = PageResource::Font {
                url: face.url,
                family: face.family,
                weight: face.weight,
                italic: face.italic,
            };
            if !self.resources.contains(&resource) {
                self.resources.push(resource);
            }
        }
        self.cached_styles = Some((viewport_width.max(1.0), styles));
        for svg in self.dom.elements_named("svg").take(MAX_INLINE_SVGS) {
            let key = inline_svg_key(&svg);
            if !self.images.contains_key(&key)
                && let Ok(image) = decode_inline_svg(&svg)
            {
                self.images.insert(key, image);
            }
        }
    }

    pub fn immediate_refresh_url(&self) -> Option<String> {
        self.dom.elements_named("meta").find_map(|node| {
            let http_equiv = node.attr("http-equiv")?;
            if !http_equiv.trim().eq_ignore_ascii_case("refresh") {
                return None;
            }
            let content = node.attr("content")?;
            let target = parse_immediate_refresh_target(&content)?;
            resolve_url(&self.base_url, target)
        })
    }

    pub fn add_image(&mut self, url: String, bytes: &[u8]) -> Result<(), String> {
        if looks_like_svg(bytes) {
            let image = decode_svg(bytes, "external SVG")?;
            self.images.insert(url, image);
            return Ok(());
        }
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
        StyleSet::from_sources(
            &self.dom,
            &self.base_url,
            &self.stylesheet_sources,
            viewport_width,
        )
    }

    pub fn cached_style(&self, viewport_width: f32) -> Option<&StyleSet> {
        self.cached_styles
            .as_ref()
            .filter(|(width, _)| (*width - viewport_width).abs() < 0.5)
            .map(|(_, styles)| styles)
    }

    pub(crate) fn image_url(&self, node: &NodeRef) -> Option<String> {
        resolve_image_url(node, &self.base_url, self.responsive_viewport_width)
    }
}

fn document_base_url(dom: &Dom, source_url: &str) -> String {
    dom.elements_named("base")
        .find_map(|node| node.attr("href"))
        .and_then(|href| resolve_url(source_url, &href))
        .unwrap_or_else(|| source_url.to_string())
}

fn discover_resources(
    dom: &Dom,
    base_url: &str,
    viewport_width: f32,
) -> (Vec<PageResource>, Vec<PageScript>) {
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
        if seen_stylesheets.len() >= MAX_STYLESHEETS {
            break;
        }
        if let Some(url) = link
            .attr("href")
            .and_then(|href| resolve_url(base_url, &href))
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
        if let Some(url) = resolve_image_url(&node, base_url, viewport_width)
            && seen_images.insert(url.clone())
        {
            resources.push(PageResource::Image { url });
        }
    }

    let mut scripts = Vec::new();
    let mut seen_script_urls = HashSet::new();
    for node in Node::descendants(&dom.document) {
        if node.tag_name() != Some("script") || scripts.len() >= MAX_SCRIPTS {
            continue;
        }
        let script_type = node.attr("type").unwrap_or_default();
        if !script::is_classic_javascript_type(&script_type) {
            continue;
        }
        if let Some(url) = node
            .attr("src")
            .and_then(|source| resolve_url(base_url, &source))
        {
            let blocks_first_paint = node.attr("async").is_none();
            if seen_script_urls.insert(url.clone()) {
                resources.push(PageResource::Script { url: url.clone() });
            }
            scripts.push(PageScript {
                node,
                source_url: url,
                code: None,
                blocks_first_paint,
            });
        } else {
            let source_url = format!("{}#inline-script-{}", base_url, scripts.len() + 1);
            let code = node.text_content();
            scripts.push(PageScript {
                node,
                source_url,
                code: Some(code),
                blocks_first_paint: true,
            });
        }
    }
    (resources, scripts)
}

pub(crate) fn resolve_image_url(
    node: &NodeRef,
    base_url: &str,
    viewport_width: f32,
) -> Option<String> {
    let source = node
        .attr("data-src")
        .filter(|source| !source.trim().is_empty())
        .or_else(|| {
            node.attr("data-lazy-src")
                .filter(|source| !source.trim().is_empty())
        })
        .or_else(|| picture_source(node, viewport_width))
        .or_else(|| responsive_source(node, viewport_width))
        .or_else(|| node.attr("src"))
        .or_else(|| node.attr("href"))?;
    resolve_url(base_url, source.trim())
}

fn picture_source(node: &NodeRef, viewport_width: f32) -> Option<String> {
    if node.tag_name() != Some("img") {
        return None;
    }
    let picture = node
        .parent()
        .filter(|parent| parent.tag_name() == Some("picture"))?;
    for source in picture.children.borrow().iter() {
        if source.id() == node.id() {
            break;
        }
        if source.tag_name() != Some("source")
            || source
                .attr("media")
                .is_some_and(|media| !media_matches(&media, viewport_width))
            || source
                .attr("type")
                .is_some_and(|kind| !supported_image_type(&kind))
        {
            continue;
        }
        if let Some(candidate) = responsive_source(source, viewport_width) {
            return Some(candidate);
        }
    }
    None
}

fn responsive_source(node: &NodeRef, viewport_width: f32) -> Option<String> {
    let srcset = node.attr("srcset")?;
    let slot_width = source_size(
        node.attr("sizes").as_deref().unwrap_or("100vw"),
        viewport_width,
    );
    preferred_srcset_candidate(&srcset, slot_width, 2.0)
}

#[derive(Debug)]
struct ImageCandidate<'a> {
    url: &'a str,
    density: f32,
}

fn preferred_srcset_candidate(
    srcset: &str,
    slot_width: f32,
    target_density: f32,
) -> Option<String> {
    let mut candidates = srcset
        .split(',')
        .filter_map(|candidate| {
            let mut parts = candidate.split_ascii_whitespace();
            let url = parts.next()?.trim();
            if url.is_empty() {
                return None;
            }
            let descriptor = parts.next();
            let density = match descriptor {
                Some(value) if value.ends_with('w') => {
                    value[..value.len() - 1].parse::<f32>().ok()? / slot_width.max(1.0)
                }
                Some(value) if value.ends_with('x') => {
                    value[..value.len() - 1].parse::<f32>().ok()?
                }
                Some(_) => return None,
                None => 1.0,
            };
            (density.is_finite() && density > 0.0).then_some(ImageCandidate { url, density })
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.density.total_cmp(&right.density));
    candidates
        .iter()
        .find(|candidate| candidate.density >= target_density)
        .or_else(|| candidates.last())
        .map(|candidate| candidate.url.to_string())
}

fn source_size(sizes: &str, viewport_width: f32) -> f32 {
    for entry in sizes
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
    {
        let (condition, length) = if entry.starts_with('(') {
            let Some(close) = entry.find(')') else {
                continue;
            };
            (Some(&entry[..=close]), entry[close + 1..].trim())
        } else {
            (None, entry)
        };
        if condition.is_some_and(|condition| !media_matches(condition, viewport_width)) {
            continue;
        }
        if let Some(size) = parse_length(length)
            .and_then(|length| length.resolve(viewport_width, 16.0))
            .filter(|size| *size >= 0.0)
        {
            return size;
        }
    }
    viewport_width
}

fn supported_image_type(kind: &str) -> bool {
    matches!(
        kind.split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "image/bmp"
            | "image/gif"
            | "image/jpeg"
            | "image/png"
            | "image/svg+xml"
            | "image/vnd.microsoft.icon"
            | "image/webp"
            | "image/x-icon"
    )
}

fn parse_immediate_refresh_target(content: &str) -> Option<&str> {
    let (delay, directive) = content.split_once(';')?;
    if delay.trim().parse::<f64>().ok()? > 0.0 {
        return None;
    }
    let (name, target) = directive.trim().split_once('=')?;
    if !name.trim().eq_ignore_ascii_case("url") {
        return None;
    }
    let target = target
        .trim()
        .trim_matches(|character| matches!(character, '\'' | '"'))
        .trim();
    (!target.is_empty()).then_some(target)
}

pub(crate) fn inline_svg_key(node: &NodeRef) -> String {
    format!("inline-svg:{:032x}", node.id().to_wire())
}

fn decode_inline_svg(node: &NodeRef) -> Result<DecodedImage, String> {
    let mut source = String::new();
    serialize_svg_node(node, &mut source, true);
    decode_svg(source.as_bytes(), "inline SVG")
}

fn looks_like_svg(bytes: &[u8]) -> bool {
    let prefix = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]);
    let prefix = prefix.trim_start_matches('\u{feff}').trim_start();
    prefix.starts_with("<svg") || (prefix.starts_with("<?xml") && prefix.contains("<svg"))
}

fn decode_svg(source: &[u8], description: &str) -> Result<DecodedImage, String> {
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

#[cfg(test)]
mod tests;
