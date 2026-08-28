mod preload;
mod refresh;
mod resources;
mod scripts;
mod svg;

pub(crate) use self::preload::discover_script_preloads;
use self::resources::{discover_resources, document_base_url, resolve_image_url};
pub(crate) use self::svg::inline_svg_key;
use self::svg::{decode_inline_svg, decode_svg, looks_like_svg};
use super::css::{StyleRefreshStats, StyleSet};
use super::dom::{self, Dom, NodeRef};
use super::font::{WebFont, WebFontFace, decode_web_font};
use super::script::{
    self, ScriptFetchOptions, ScriptInput, ScriptKind, ScriptOutcome, ScriptRuntime,
};
use crate::limits::{
    MAX_CSS_SOURCE_BYTES, MAX_DECODED_IMAGE_BYTES, MAX_DECODED_IMAGE_DIMENSION,
    MAX_DECODED_IMAGE_PIXELS, MAX_EMBEDDED_IMAGE_URL_BYTES, MAX_IMAGE_SOURCE_BYTES,
    MAX_INLINE_SVGS, MAX_PAGE_IMAGES as MAX_IMAGES, MAX_SCRIPT_BYTES, MAX_STYLE_IMAGES,
    MAX_WEB_FONTS, bounded_utf8_prefix,
};
use crate::navigation::resolve_url;
use data_url::DataUrl;
use image::ImageReader;
use std::collections::HashMap;
use std::io::Cursor;

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
        kind: ScriptKind,
        fetch_options: ScriptFetchOptions,
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
    pub kind: ScriptKind,
    pub fetch_options: ScriptFetchOptions,
    pub blocks_first_paint: bool,
    pub executes_after_parsing: bool,
}

#[derive(Debug, Clone)]
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
    pub character_set: String,
    base_url: String,
    pub resources: Vec<PageResource>,
    pub scripts: Vec<PageScript>,
    pub external_stylesheets: Vec<String>,
    stylesheet_sources: Vec<(String, String)>,
    cached_styles: Option<(f32, f32, StyleSet)>,
    pub images: HashMap<String, DecodedImage>,
    pub fonts: Vec<WebFont>,
    pub diagnostics: Vec<String>,
    responsive_viewport_width: f32,
    prefers_dark_color_scheme: bool,
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
        let diagnostics = dom
            .errors
            .borrow()
            .iter()
            .filter(|error| error.starts_with("safety limit:"))
            .cloned()
            .collect();
        let base_url = document_base_url(&dom, source_url);
        let responsive_viewport_width = 1280.0;
        let (resources, scripts) = discover_resources(&dom, &base_url, responsive_viewport_width);

        let mut images = HashMap::new();
        for svg in dom.elements_named("svg").take(MAX_INLINE_SVGS) {
            if let Ok(image) = decode_inline_svg(&svg) {
                images.insert(inline_svg_key(&svg), image);
            }
        }

        let mut page = Self {
            dom,
            title,
            source_url: source_url.to_string(),
            character_set: "UTF-8".to_string(),
            base_url,
            resources,
            scripts,
            external_stylesheets: Vec::new(),
            stylesheet_sources: Vec::new(),
            cached_styles: None,
            images,
            fonts: Vec::new(),
            diagnostics,
            responsive_viewport_width,
            prefers_dark_color_scheme: false,
        };
        page.install_embedded_images();
        page
    }

    pub fn set_media_environment(&mut self, viewport_width: f32, prefers_dark_color_scheme: bool) {
        self.responsive_viewport_width = viewport_width.max(1.0);
        if self.prefers_dark_color_scheme != prefers_dark_color_scheme {
            self.prefers_dark_color_scheme = prefers_dark_color_scheme;
            self.cached_styles = None;
        }
    }

    pub fn add_stylesheet(&mut self, css: String) -> bool {
        let source_url = self.base_url.clone();
        self.add_stylesheet_from(&source_url, css)
    }

    pub fn add_stylesheet_from(&mut self, source_url: &str, css: String) -> bool {
        let (css, truncated) = bounded_utf8_prefix(&css, MAX_CSS_SOURCE_BYTES);
        if truncated {
            self.diagnostics.push(format!(
                "stylesheet {source_url} was truncated at {MAX_CSS_SOURCE_BYTES} bytes"
            ));
        }
        let css = css.to_string();
        self.cached_styles = None;
        self.stylesheet_sources
            .push((source_url.to_string(), css.clone()));
        self.external_stylesheets.push(css);
        true
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
            let target = parse_immediate_refresh_target(&content)?;
            resolve_url(&self.base_url, target)
        })
    }

    pub fn add_image(&mut self, url: String, bytes: &[u8]) -> Result<(), String> {
        if bytes.len() > MAX_IMAGE_SOURCE_BYTES {
            return Err(format!(
                "image source exceeds the {MAX_IMAGE_SOURCE_BYTES}-byte limit"
            ));
        }
        if looks_like_svg(bytes) {
            let image = decode_svg(bytes, "external SVG")?;
            self.images.insert(url, image);
            return Ok(());
        }
        let mut reader = ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|error| format!("detect image format: {error}"))?;
        // image's reader reserves the decoder-reported output size before allocating the output
        // buffer. Keep its own limit active as the primary allocation boundary, then validate the
        // exact pixel product below because independent width/height limits cannot express it.
        let mut decoder_limits = image::Limits::default();
        decoder_limits.max_image_width = Some(MAX_DECODED_IMAGE_DIMENSION);
        decoder_limits.max_image_height = Some(MAX_DECODED_IMAGE_DIMENSION);
        decoder_limits.max_alloc = Some(MAX_DECODED_IMAGE_BYTES);
        reader.limits(decoder_limits);
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

    pub(super) fn install_embedded_images(&mut self) {
        let urls = self
            .resources
            .iter()
            .filter_map(|resource| match resource {
                PageResource::Image { url }
                    if url.starts_with("data:") && !self.images.contains_key(url) =>
                {
                    Some(url.clone())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        for url in urls {
            let Ok(bytes) = decode_embedded_image(&url) else {
                continue;
            };
            let _ = self.add_image(url, &bytes);
        }
    }

    pub fn image_url(&self, node: &NodeRef) -> Option<String> {
        resolve_image_url(node, &self.base_url, self.responsive_viewport_width)
    }
}

fn decode_embedded_image(url: &str) -> Result<Vec<u8>, String> {
    if url.len() > MAX_EMBEDDED_IMAGE_URL_BYTES {
        return Err("embedded image URL is too large".into());
    }
    let data = DataUrl::process(url).map_err(|error| error.to_string())?;
    if data.mime_type().type_ != "image" {
        return Err("embedded resource is not an image".into());
    }
    let (bytes, _) = data
        .decode_to_vec()
        .map_err(|error| format!("decode embedded image: {error:?}"))?;
    Ok(bytes)
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
