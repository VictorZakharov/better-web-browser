//! Bounded parser-side discovery for script fetches that may overlap authoritative parsing.

use super::PageResource;
use crate::engine::script::{self, ScriptFetchOptions, ScriptKind};
use crate::limits::{MAX_HTML_INPUT_BYTES, MAX_PAGE_SCRIPTS, bounded_utf8_prefix};
use crate::navigation::resolve_url;
use html5ever::tokenizer::states::RawKind;
use html5ever::tokenizer::{
    BufferQueue, EndTag, StartTag, Tag, TagToken, Token, TokenSink, TokenSinkResult, Tokenizer,
};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ScriptPreloads {
    pub(crate) first_paint: Vec<PageResource>,
    pub(crate) deferred: Vec<PageResource>,
}

pub(crate) fn discover_script_preloads(html: &str, document_url: &str) -> ScriptPreloads {
    let (html, _) = bounded_utf8_prefix(html, MAX_HTML_INPUT_BYTES);
    let input = BufferQueue::default();
    input.push_back(html.into());
    let tokenizer = Tokenizer::new(
        PreloadSink {
            state: RefCell::new(PreloadState::new(document_url)),
        },
        Default::default(),
    );
    let _ = tokenizer.feed(&input);
    tokenizer.end();
    let state = tokenizer.sink.state.into_inner();
    let mut result = ScriptPreloads::default();
    for preload in state.resources {
        // A modulepreload link owns the shared module response. Do not start a second
        // speculative script fetch before authoritative link discovery can reuse it.
        if let PageResource::Script {
            url,
            kind: ScriptKind::Module,
            fetch_options,
            script_source,
        } = &preload.resource
            && state.module_hints.contains(&(
                url.clone(),
                *fetch_options,
                script_source.nonce.clone(),
            ))
        {
            continue;
        }
        if preload.blocks_first_paint {
            result.first_paint.push(preload.resource);
        } else {
            result.deferred.push(preload.resource);
        }
    }
    result
}

struct PreloadSink {
    state: RefCell<PreloadState>,
}

struct PreloadState {
    base_url: String,
    base_seen: bool,
    template_depth: usize,
    script_count: usize,
    resources: Vec<DiscoveredPreload>,
    seen: HashMap<PageResource, usize>,
    module_hints: HashSet<(String, ScriptFetchOptions, Option<String>)>,
}

struct DiscoveredPreload {
    resource: PageResource,
    blocks_first_paint: bool,
}

impl PreloadState {
    fn new(document_url: &str) -> Self {
        Self {
            base_url: document_url.to_string(),
            base_seen: false,
            template_depth: 0,
            script_count: 0,
            resources: Vec::new(),
            seen: HashMap::new(),
            module_hints: HashSet::new(),
        }
    }

    fn process_start_tag(&mut self, tag: &Tag) {
        let name = tag.name.as_ref();
        if name == "template" {
            self.template_depth += 1;
            return;
        }
        if self.template_depth > 0 {
            return;
        }
        if name == "base" && !self.base_seen {
            if let Some(href) = attribute(tag, "href") {
                self.base_seen = true;
                if let Some(url) = resolve_url(&self.base_url, href) {
                    self.base_url = url;
                }
            }
            return;
        }
        if name == "link" {
            self.record_module_hint(tag);
            return;
        }
        if name != "script" || self.script_count >= MAX_PAGE_SCRIPTS {
            return;
        }
        let script_type = attribute(tag, "type").unwrap_or_default();
        let kind = if script_type.trim().eq_ignore_ascii_case("module") {
            ScriptKind::Module
        } else if script::is_classic_javascript_type(script_type) {
            ScriptKind::Classic
        } else {
            return;
        };
        if kind == ScriptKind::Classic && attribute(tag, "nomodule").is_some() {
            return;
        }
        self.script_count += 1;
        let fetch_options = ScriptFetchOptions::for_element(
            kind,
            attribute(tag, "crossorigin"),
            attribute(tag, "referrerpolicy"),
        );
        let Some(source) = attribute(tag, "src").filter(|source| !source.trim().is_empty()) else {
            return;
        };
        // Integrity is element-owned and must be checked before a response is retained as
        // executable code. Defer these to authoritative parser discovery so speculative bytes
        // cannot bypass the owner's expected digest.
        if attribute(tag, "integrity").is_some_and(|value| !value.trim().is_empty()) {
            return;
        }
        let Some(url) = resolve_url(&self.base_url, source.trim()) else {
            return;
        };
        let resource = PageResource::Script {
            url,
            kind,
            fetch_options,
            script_source: crate::fetch::csp::ScriptSource {
                nonce: attribute(tag, "nonce").map(str::to_owned),
                parser_inserted: true,
            },
        };
        let blocks_first_paint = kind == ScriptKind::Classic
            && attribute(tag, "async").is_none()
            && attribute(tag, "defer").is_none();
        if let Some(index) = self.seen.get(&resource).copied() {
            self.resources[index].blocks_first_paint |= blocks_first_paint;
        } else {
            self.seen.insert(resource.clone(), self.resources.len());
            self.resources.push(DiscoveredPreload {
                resource,
                blocks_first_paint,
            });
        }
    }

    fn record_module_hint(&mut self, tag: &Tag) {
        let has_rel = attribute(tag, "rel").is_some_and(|value| {
            value
                .split_ascii_whitespace()
                .any(|token| token.eq_ignore_ascii_case("modulepreload"))
        });
        if !has_rel
            || attribute(tag, "as").is_some_and(|value| !value.eq_ignore_ascii_case("script"))
            || attribute(tag, "integrity").is_some_and(|value| !value.trim().is_empty())
            || attribute(tag, "media").is_some()
        {
            return;
        }
        if let Some(mime) = attribute(tag, "type")
            && !script::is_classic_javascript_type(mime.split(';').next().unwrap_or("").trim())
        {
            return;
        }
        let Some(url) =
            attribute(tag, "href").and_then(|href| resolve_url(&self.base_url, href.trim()))
        else {
            return;
        };
        let options = ScriptFetchOptions::for_element(
            ScriptKind::Module,
            attribute(tag, "crossorigin"),
            attribute(tag, "referrerpolicy"),
        );
        self.module_hints
            .insert((url, options, attribute(tag, "nonce").map(str::to_owned)));
    }
}

impl TokenSink for PreloadSink {
    type Handle = ();

    fn process_token(&self, token: Token, _line_number: u64) -> TokenSinkResult<Self::Handle> {
        let TagToken(tag) = token else {
            return TokenSinkResult::Continue;
        };
        match tag.kind {
            StartTag => self.state.borrow_mut().process_start_tag(&tag),
            EndTag if tag.name.as_ref() == "template" => {
                let depth = &mut self.state.borrow_mut().template_depth;
                *depth = depth.saturating_sub(1);
            }
            EndTag => {}
        }
        if tag.kind != StartTag {
            return TokenSinkResult::Continue;
        }
        match tag.name.as_ref() {
            "script" => TokenSinkResult::RawData(RawKind::ScriptData),
            "title" | "textarea" => TokenSinkResult::RawData(RawKind::Rcdata),
            "style" | "xmp" | "iframe" | "noembed" | "noframes" | "noscript" => {
                TokenSinkResult::RawData(RawKind::Rawtext)
            }
            "plaintext" => TokenSinkResult::Plaintext,
            _ => TokenSinkResult::Continue,
        }
    }
}

fn attribute<'a>(tag: &'a Tag, name: &str) -> Option<&'a str> {
    tag.attrs
        .iter()
        .find(|attribute| attribute.name.local.as_ref() == name)
        .map(|attribute| attribute.value.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_unique_classic_and_module_scripts() {
        let resources = discover_script_preloads(
            r#"<script async src=a.js></script><script defer src=a.js></script>
               <script async type=module crossorigin=use-credentials
                       referrerpolicy=no-referrer src=m.js></script>"#,
            "https://example.com/app/index.html",
        );
        assert!(resources.first_paint.is_empty());
        assert_eq!(
            resources.deferred,
            vec![
                PageResource::Script {
                    url: "https://example.com/app/a.js".into(),
                    kind: ScriptKind::Classic,
                    fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
                    script_source: crate::fetch::csp::ScriptSource::default(),
                },
                PageResource::Script {
                    url: "https://example.com/app/m.js".into(),
                    kind: ScriptKind::Module,
                    fetch_options: ScriptFetchOptions::for_element(
                        ScriptKind::Module,
                        Some("use-credentials"),
                        Some("no-referrer"),
                    ),
                    script_source: crate::fetch::csp::ScriptSource::default(),
                }
            ]
        );
    }

    #[test]
    fn respects_base_and_skips_nomodule_and_template_contents() {
        let resources = discover_script_preloads(
            r#"<base><base href=/assets/><script nomodule src=legacy.js></script>
               <template><script src=not-fetched.js></script></template>
               <script src=app.js></script>"#,
            "https://example.com/page",
        );
        assert_eq!(
            resources.first_paint,
            vec![PageResource::Script {
                url: "https://example.com/assets/app.js".into(),
                kind: ScriptKind::Classic,
                fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
                script_source: crate::fetch::csp::ScriptSource::default(),
            }]
        );
        assert!(resources.deferred.is_empty());
    }

    #[test]
    fn ignores_script_like_text_in_raw_text_elements() {
        let resources = discover_script_preloads(
            r#"<style><script src=style.js></script></style>
               <script>"<script src=string.js></script>"</script>
               <textarea><script src=textarea.js></script></textarea>
               <script src=real.js></script>"#,
            "https://example.com/",
        );
        assert_eq!(
            resources.first_paint,
            vec![PageResource::Script {
                url: "https://example.com/real.js".into(),
                kind: ScriptKind::Classic,
                fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
                script_source: crate::fetch::csp::ScriptSource::default(),
            }]
        );
        assert!(resources.deferred.is_empty());
    }

    #[test]
    fn inline_scripts_consume_the_same_page_script_budget() {
        let mut html = "<script></script>".repeat(MAX_PAGE_SCRIPTS);
        html.push_str("<script src=too-late.js></script>");
        assert_eq!(
            discover_script_preloads(&html, "https://example.com/"),
            ScriptPreloads::default()
        );
    }
}
