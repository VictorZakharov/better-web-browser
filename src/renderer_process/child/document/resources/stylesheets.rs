//! CSS imports use the same response validation as linked stylesheets, not a raw text bypass.
//! https://www.w3.org/TR/css-cascade-5/#content-type
use crate::engine::Page;
use crate::fetch::{FetchResponse, Origin};

pub(super) fn valid_response(page: &Page, response: &FetchResponse) -> bool {
    let css_type = response.content_type().is_none_or(|value| {
        value
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .eq_ignore_ascii_case("text/css")
    });
    if css_type {
        return true;
    }
    // The historical MIME exception is restricted to same-origin quirks documents.
    page.dom.quirks_mode.get() == html5ever::tree_builder::QuirksMode::Quirks
        && Origin::parse(&page.source_url)
            .ok()
            .zip(Origin::parse(response.final_url().as_str()).ok())
            .is_some_and(|(document, sheet)| document.is_same_origin(&sheet))
}
