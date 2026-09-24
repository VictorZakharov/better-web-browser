//! Link preload request construction, separate from authoritative resource discovery.
//! https://html.spec.whatwg.org/multipage/links.html#link-type-preload

use super::{PageResource, PreloadAs};
use crate::engine::css::media::{MediaEnvironment, media_matches_for_environment};
use crate::engine::dom::NodeRef;
use crate::engine::script::{self, ScriptFetchOptions, ScriptKind};
use crate::fetch::{CredentialsMode, RequestMode};
use crate::navigation::resolve_resource_url;

pub(super) fn discover_link_preload(
    link: &NodeRef,
    base_url: &str,
    environment: MediaEnvironment,
) -> Option<PageResource> {
    if link
        .attr("media")
        .is_some_and(|value| !media_matches_for_environment(&value, environment))
    {
        return None;
    }
    let rel = link.attr("rel")?;
    let module = rel
        .split_ascii_whitespace()
        .any(|token| token.eq_ignore_ascii_case("modulepreload"));
    let preload = rel
        .split_ascii_whitespace()
        .any(|token| token.eq_ignore_ascii_case("preload"));
    if !module && !preload {
        return None;
    }
    // Modulepreload's absent `as` defaults to script; other as-values require their own
    // module graph algorithms and must not be misrepresented as a classic script preload.
    let as_type = if module {
        if link
            .attr("as")
            .is_some_and(|value| !value.eq_ignore_ascii_case("script"))
        {
            return None;
        }
        PreloadAs::ModuleScript
    } else {
        match link.attr("as")?.to_ascii_lowercase().as_str() {
            "script" => PreloadAs::Script,
            "style" => PreloadAs::Style,
            "image" => PreloadAs::Image,
            "font" => PreloadAs::Font,
            _ => return None,
        }
    };
    if let Some(mime) = link.attr("type") {
        let mime = mime.split(';').next().unwrap_or("").trim();
        let supported = match as_type {
            PreloadAs::Script | PreloadAs::ModuleScript => script::is_classic_javascript_type(mime),
            PreloadAs::Style => mime.eq_ignore_ascii_case("text/css"),
            PreloadAs::Image => super::resources::supported_image_type(mime),
            PreloadAs::Font => matches!(
                mime.to_ascii_lowercase().as_str(),
                "font/woff" | "font/woff2" | "font/ttf" | "font/otf" | "application/font-woff"
            ),
        };
        if !supported {
            return None;
        }
    }
    let source = if as_type == PreloadAs::Image {
        link.attr("imagesrcset")
            .and_then(|srcset| {
                super::responsive_images::select_source(
                    &srcset,
                    link.attr("imagesizes").as_deref(),
                    link.attr("href").as_deref(),
                    environment,
                )
            })
            .or_else(|| link.attr("href"))
    } else {
        link.attr("href")
    }?;
    let url = resolve_resource_url(base_url, source.trim())?;
    let options = ScriptFetchOptions::for_element(
        if module {
            ScriptKind::Module
        } else {
            ScriptKind::Classic
        },
        link.attr("crossorigin").as_deref(),
        link.attr("referrerpolicy").as_deref(),
    );
    let mode = if as_type == PreloadAs::Font {
        RequestMode::Cors
    } else {
        options.mode
    };
    // Classic scripts include credentials by default. Modules and non-script destinations
    // instead use same-origin credentials unless crossorigin=use-credentials was requested.
    let credentials = if matches!(as_type, PreloadAs::Script | PreloadAs::ModuleScript) {
        options.credentials
    } else if link
        .attr("crossorigin")
        .is_some_and(|value| value.eq_ignore_ascii_case("use-credentials"))
    {
        CredentialsMode::Include
    } else {
        CredentialsMode::SameOrigin
    };
    Some(PageResource::Preload {
        url,
        as_type,
        mode,
        credentials,
        referrer_policy: options.referrer_policy,
        integrity: link.attr("integrity").unwrap_or_default(),
        nonce: link.attr("nonce"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Page;

    #[test]
    fn modulepreload_defaults_to_cors_and_same_origin_credentials() {
        let page = Page::parse_scripted(
            "<link rel=modulepreload href=/app.mjs nonce=trusted>",
            "https://example.test/",
        );
        let preload = page
            .resources
            .iter()
            .find(|resource| matches!(resource, PageResource::Preload { .. }))
            .unwrap();
        assert!(matches!(preload, PageResource::Preload {
            as_type: PreloadAs::ModuleScript, mode: RequestMode::Cors,
            credentials: CredentialsMode::SameOrigin, nonce: Some(nonce), ..
        } if nonce == "trusted"));
    }

    #[test]
    fn unsupported_modulepreload_destination_and_mime_do_not_fetch() {
        let page = Page::parse_scripted(
            "<link rel=modulepreload href=/a.mjs as=style><link rel=modulepreload href=/b.mjs type=image/png>",
            "https://example.test/",
        );
        assert!(
            !page
                .resources
                .iter()
                .any(|resource| matches!(resource, PageResource::Preload { .. }))
        );
    }

    #[test]
    fn woff2_preload_is_admitted_for_font_destination() {
        let page = Page::parse_scripted(
            "<link rel=preload as=font type=font/woff2 href=/font.woff2 crossorigin>",
            "https://example.test/",
        );
        assert!(page.resources.iter().any(|resource| matches!(resource,
            PageResource::Preload { as_type: PreloadAs::Font, url, .. }
                if url == "https://example.test/font.woff2")));
    }

    #[test]
    fn invalid_image_source_set_falls_back_to_href() {
        let page = Page::parse_scripted(
            "<link rel=preload as=image href=/fallback.png imagesrcset='bad.png 0w'>",
            "https://example.test/",
        );
        assert!(page.resources.iter().any(|resource| matches!(resource,
            PageResource::Preload { as_type: PreloadAs::Image, url, .. }
                if url == "https://example.test/fallback.png")));
    }

    #[test]
    fn image_preload_uses_sizes_and_actual_device_pixel_ratio() {
        let mut page = Page::parse_scripted(
            "<link rel=preload as=image href=/fallback.png imagesizes=400px \
             imagesrcset='/small.png 400w, /large.png 800w'>",
            "https://example.test/",
        );
        page.set_media_environment(MediaEnvironment::new(400.0, 720.0, 2.0, false));
        page.refresh_resources(400.0);
        assert!(page.resources.iter().any(|resource| matches!(resource,
            PageResource::Preload { as_type: PreloadAs::Image, url, .. }
                if url == "https://example.test/large.png")));
        assert!(!page.resources.iter().any(|resource| matches!(resource,
            PageResource::Preload { url, .. } if url == "https://example.test/fallback.png")));
    }

    #[test]
    fn preload_media_filters_before_a_network_resource_is_created() {
        let page = Page::parse_scripted(
            "<link rel=preload as=style href=/wide.css media='(min-width: 1500px)'>\
             <link rel=preload as=style href=/narrow.css media='(max-width: 1400px)'>",
            "https://example.test/",
        );
        assert!(!page.resources.iter().any(|resource| matches!(resource,
            PageResource::Preload { url, .. } if url.ends_with("wide.css"))));
        assert!(page.resources.iter().any(|resource| matches!(resource,
            PageResource::Preload { url, .. } if url.ends_with("narrow.css"))));
    }
}
