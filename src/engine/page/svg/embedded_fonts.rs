//! SVG image documents can use embedded fonts, but cannot fetch ambient URLs.
//! Reuse CSS Fonts descriptor parsing and the validated WOFF/SFNT decoder.
use crate::engine::css::{media::MediaEnvironment, stylesheet::font_faces};
use crate::engine::font::{WebFont, decode_web_font};
use crate::limits::MAX_FONT_BYTES;
use data_url::DataUrl;
use resvg::usvg::roxmltree::Document;

const MAX_FACES: usize = 64;
const MAX_FONT_WORKING_BYTES: usize = 32 * 1024 * 1024;

pub(super) fn collect(document: &Document<'_>) -> Vec<WebFont> {
    let mut fonts = Vec::new();
    let mut attempted = 0usize;
    let mut remaining = MAX_FONT_WORKING_BYTES;
    for node in document.descendants().filter(|node| {
        node.is_element()
            && node.tag_name().namespace() == Some("http://www.w3.org/2000/svg")
            && node.tag_name().name() == "style"
    }) {
        if node
            .attribute("type")
            .is_some_and(|mime| !mime.eq_ignore_ascii_case("text/css"))
        {
            continue;
        }
        let css = node
            .children()
            .filter_map(|child| child.text())
            .collect::<String>();
        for face in
            font_faces::collect_for_svg_image(&css, MediaEnvironment::new(300.0, 150.0, 1.0, false))
        {
            if attempted == MAX_FACES || remaining == 0 {
                return fonts;
            }
            attempted += 1;
            for url in std::iter::once(&face.url).chain(face.fallback_urls.iter()) {
                // Never call a file/network resolver in image-document mode. Local()
                // candidates are already declined by the shared descriptor parser.
                if !url
                    .get(..5)
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case("data:"))
                    || url.len() > MAX_FONT_BYTES * 2
                {
                    continue;
                }
                let Ok(data) = DataUrl::process(url) else {
                    continue;
                };
                let mut bytes = Vec::new();
                let limit = remaining.min(MAX_FONT_BYTES);
                let decoded = data.decode(|chunk| {
                    if chunk.len() > limit.saturating_sub(bytes.len()) {
                        return Err(());
                    }
                    bytes.extend_from_slice(chunk);
                    Ok(())
                });
                if decoded.is_err() {
                    continue;
                }
                let Ok(font) = decode_web_font(&face, &bytes) else {
                    continue;
                };
                let cost = bytes.len().saturating_add(font.sfnt.len());
                if cost > remaining {
                    continue;
                }
                remaining -= cost;
                fonts.push(font);
                break;
            }
        }
    }
    fonts
}

#[cfg(test)]
mod tests;
