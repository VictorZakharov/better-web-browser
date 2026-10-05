//! Normalize fragment bytes once for DOM lookup and the isolated SVG decoder.
use crate::engine::fragment_navigation::decode_fragment;
use crate::limits::MAX_SVG_SOURCE_BYTES;
use cssparser::{Parser, ParserInput, serialize_string};

pub(super) fn fragment(url: &str) -> Option<String> {
    url.strip_prefix('#')
        .filter(|id| !id.is_empty())
        .map(decode_fragment)
}

pub(super) fn href(value: &str) -> String {
    fragment(value).map_or_else(|| value.to_owned(), |id| format!("#{id}"))
}

pub(super) fn paint(value: &str) -> Result<String, String> {
    if value.len() > MAX_SVG_SOURCE_BYTES {
        return Err("SVG URL input exceeds source budget".into());
    }
    let mut input = ParserInput::new(value);
    let mut parser = Parser::new(&mut input);
    let mut output = String::new();
    let mut copied = parser.position();
    while !parser.is_exhausted() {
        let start = parser.position();
        if let Ok(url) = parser.try_parse(|parser| parser.expect_url()) {
            if let Some(id) = fragment(&url) {
                output.push_str(parser.slice(copied..start));
                output.push_str("url(");
                if id.chars().all(|character| {
                    !character.is_whitespace()
                        && !character.is_control()
                        && !matches!(character, '"' | '\'' | '(' | ')' | '\\')
                }) {
                    output.push('#');
                    output.push_str(&id);
                } else {
                    serialize_string(&format!("#{id}"), &mut output)
                        .map_err(|_| "serialize SVG URL")?;
                }
                output.push(')');
                copied = parser.position();
            }
        } else {
            let _ = parser.next();
        }
        if output.len() > MAX_SVG_SOURCE_BYTES {
            return Err("SVG URL normalization exceeds source budget".into());
        }
    }
    output.push_str(parser.slice_from(copied));
    if output.len() > MAX_SVG_SOURCE_BYTES {
        return Err("SVG URL normalization exceeds source budget".into());
    }
    Ok(output)
}

pub(super) fn is_paint_attribute(local: &str) -> bool {
    matches!(
        local,
        "fill"
            | "stroke"
            | "filter"
            | "clip-path"
            | "mask"
            | "marker"
            | "marker-start"
            | "marker-mid"
            | "marker-end"
            | "style"
    )
}
