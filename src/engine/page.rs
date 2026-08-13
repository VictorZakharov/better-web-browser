mod resources;
mod svg;

use self::resources::{discover_resources, document_base_url, resolve_image_url};
pub(crate) use self::svg::inline_svg_key;
use self::svg::{decode_inline_svg, decode_svg, looks_like_svg};
use super::css::{Display, StyleSet};
use super::dom::{self, Dom, Node, NodeRef};
use super::font::{WebFont, WebFontFace, decode_web_font, discover_font_faces};
use super::script::{self, ScriptInput, ScriptOutcome, ScriptRuntime};
use crate::navigation::resolve_url;
use image::ImageReader;
use std::collections::{HashMap, HashSet};
use std::io::Cursor;

const MAX_DECODED_IMAGE_PIXELS: u64 = 32 * 1024 * 1024;
const MAX_IMAGES: usize = 64;
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

    /// Starts first-paint scripts and retains their same-thread realm for post-load work.
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
        let retains_non_blocking_scripts =
            first_paint_only && self.scripts.iter().any(|script| !script.blocks_first_paint);
        let (runtime, mut outcome) = if inputs.is_empty() && !retains_non_blocking_scripts {
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

#[cfg(test)]
mod tests;
