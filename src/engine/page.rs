use super::css::{Display, StyleSet};
use super::dom::{self, Dom, Node, NodeData, NodeRef};
use super::font::{WebFont, WebFontFace, decode_web_font, discover_font_faces};
use super::script::{self, ScriptInput, ScriptOutcome};
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
        let (resources, scripts) = discover_resources(&dom, &base_url);

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
        self.execute_script_phase(false)
    }

    pub fn execute_first_paint_scripts(&mut self) -> ScriptOutcome {
        self.execute_script_phase(true)
    }

    fn execute_script_phase(&mut self, first_paint_only: bool) -> ScriptOutcome {
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
        let mut outcome = script::execute(self.dom.document.clone(), &self.source_url, &inputs);
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
        outcome
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
        let (resources, _) = discover_resources(&self.dom, &self.base_url);
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
        resolve_image_url(node, &self.base_url)
    }
}

fn document_base_url(dom: &Dom, source_url: &str) -> String {
    dom.elements_named("base")
        .find_map(|node| node.attr("href"))
        .and_then(|href| resolve_url(source_url, &href))
        .unwrap_or_else(|| source_url.to_string())
}

fn discover_resources(dom: &Dom, base_url: &str) -> (Vec<PageResource>, Vec<PageScript>) {
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
        if let Some(url) = resolve_image_url(&node, base_url)
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
        if !is_classic_javascript_type(&script_type) {
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

pub(crate) fn resolve_image_url(node: &NodeRef, base_url: &str) -> Option<String> {
    let source = node
        .attr("data-src")
        .filter(|source| !source.trim().is_empty())
        .or_else(|| {
            node.attr("data-lazy-src")
                .filter(|source| !source.trim().is_empty())
        })
        .or_else(|| {
            node.attr("srcset")
                .and_then(|srcset| preferred_srcset_candidate(&srcset))
        })
        .or_else(|| node.attr("src"))
        .or_else(|| node.attr("href"))?;
    resolve_url(base_url, source.trim())
}

fn preferred_srcset_candidate(srcset: &str) -> Option<String> {
    srcset
        .split(',')
        .filter_map(|candidate| candidate.split_ascii_whitespace().next())
        .rfind(|candidate| !candidate.is_empty())
        .map(str::to_string)
}

fn is_classic_javascript_type(script_type: &str) -> bool {
    matches!(
        script_type.trim().to_ascii_lowercase().as_str(),
        "" | "text/javascript"
            | "application/javascript"
            | "text/ecmascript"
            | "application/ecmascript"
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
    format!("inline-svg:{:x}", std::rc::Rc::as_ptr(node) as usize)
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
    fn prefers_lazy_and_high_density_image_sources_over_placeholders() {
        let page = Page::parse(
            r#"<img src="data:image/svg+xml,placeholder" data-src="portrait.jpg">
               <img src="small.jpg" srcset="small.jpg 1x, large.jpg 2x">"#,
            "https://example.com/posts/",
        );
        assert!(page.resources.contains(&PageResource::Image {
            url: "https://example.com/posts/portrait.jpg".into()
        }));
        assert!(page.resources.contains(&PageResource::Image {
            url: "https://example.com/posts/large.jpg".into()
        }));
    }

    #[test]
    fn requests_only_webfont_faces_used_by_computed_styles() {
        let mut page = Page::parse("<body><strong>text</strong></body>", "https://example.com/");
        page.add_stylesheet_from(
            "https://example.com/css/main.css",
            r#"
                @font-face { font-family: Used; font-weight: 400;
                    src: url(../fonts/used.woff) format("woff"); }
                @font-face { font-family: Used; font-weight: 700;
                    src: url(../fonts/used-bold.woff) format("woff"); }
                @font-face { font-family: Unused;
                    src: url(../fonts/unused.woff) format("woff"); }
                body { font-family: Used; }
            "#
            .into(),
        );
        page.refresh_resources(800.0);
        let fonts = page
            .resources
            .iter()
            .filter_map(|resource| match resource {
                PageResource::Font { url, .. } => Some(url.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            fonts,
            vec![
                "https://example.com/fonts/used.woff",
                "https://example.com/fonts/used-bold.woff"
            ]
        );
    }

    #[test]
    fn discovers_background_images_from_computed_styles() {
        let mut page = Page::parse(
            r#"<a class="logo"></a>"#,
            "https://example.com/articles/page.html",
        );
        page.add_stylesheet_from(
            "https://cdn.example/css/site.css",
            ".logo { background: no-repeat center url(../images/logo.svg) }".into(),
        );
        page.refresh_resources(800.0);
        assert!(page.resources.contains(&PageResource::Image {
            url: "https://cdn.example/images/logo.svg".into()
        }));
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
    fn rasterizes_external_svg_images() {
        let mut page = Page::parse("", "https://example.com/");
        page.add_image(
            "https://example.com/logo.svg".into(),
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="red"/></svg>"#,
        )
        .unwrap();
        let image = &page.images["https://example.com/logo.svg"];
        assert_eq!((image.width, image.height), (20, 10));
        assert!(image.bgra.chunks_exact(4).any(|pixel| pixel[3] != 0));
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

    #[test]
    fn resolves_immediate_meta_refresh_against_the_document_base() {
        let page = Page::parse(
            r#"
                <base href="https://example.com/base/">
                <meta http-equiv="refresh" content="0; URL='../landing?q=1&amp;x=2'">
            "#,
            "https://example.com/start",
        );
        assert_eq!(
            page.immediate_refresh_url().as_deref(),
            Some("https://example.com/landing?q=1&x=2")
        );
    }

    #[test]
    fn ignores_delayed_meta_refresh_for_immediate_navigation() {
        let page = Page::parse(
            r#"<meta http-equiv="refresh" content="5;url=/later">"#,
            "https://example.com/start",
        );
        assert_eq!(page.immediate_refresh_url(), None);
    }

    #[test]
    fn discovers_external_scripts_and_executes_dom_mutations() {
        let mut page = Page::parse_scripted(
            r#"
                <body><main id="app"></main>
                <script src="/library.js"></script>
                <script>
                    const item = document.createElement('p');
                    item.textContent = libraryMessage;
                    document.getElementById('app').appendChild(item);
                </script>
            "#,
            "https://example.com/start",
        );
        assert!(page.resources.contains(&PageResource::Script {
            url: "https://example.com/library.js".into()
        }));
        page.add_script(
            "https://example.com/library.js",
            "const libraryMessage = 'loaded';".into(),
        );
        let outcome = page.execute_scripts();
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(outcome.executed, 2);
        assert_eq!(
            page.dom.elements_named("p").next().unwrap().text_content(),
            "loaded"
        );
    }

    #[test]
    fn keeps_async_scripts_off_the_first_paint_path() {
        let mut page = Page::parse_scripted(
            r#"
                <body><div id="status">initial</div>
                <script async src="/analytics.js"></script>
                <script src="/application.js"></script>
            "#,
            "https://example.com/",
        );
        let analytics = PageResource::Script {
            url: "https://example.com/analytics.js".into(),
        };
        let application = PageResource::Script {
            url: "https://example.com/application.js".into(),
        };
        assert!(!page.resource_blocks_first_paint(&analytics));
        assert!(page.resource_blocks_first_paint(&application));

        page.add_script(
            "https://example.com/analytics.js",
            "document.getElementById('status').textContent = 'analytics';".into(),
        );
        page.add_script(
            "https://example.com/application.js",
            "document.getElementById('status').textContent = 'application';".into(),
        );
        let outcome = page.execute_first_paint_scripts();
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(outcome.executed, 1);
        assert_eq!(
            page.dom
                .elements_named("div")
                .next()
                .unwrap()
                .text_content(),
            "application"
        );
    }
}
