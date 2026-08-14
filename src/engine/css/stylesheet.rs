//! Stylesheet rule and declaration parsing.

use super::*;

#[derive(Debug)]
pub(super) struct Rule {
    pub(super) selector: Selector,
    pub(super) declarations: Vec<Declaration>,
    pub(super) order: u32,
    pub(super) base_url: String,
}

#[derive(Debug, Clone)]
pub(super) struct Declaration {
    pub(super) name: String,
    pub(super) value: String,
    pub(super) important: bool,
}

pub(super) fn parse_stylesheet(
    css: &str,
    base_url: &str,
    viewport_width: f32,
    next_order: &mut u32,
    output: &mut Vec<Rule>,
) {
    let css = strip_comments(css);
    parse_rule_list(&css, base_url, viewport_width, next_order, output);
}

pub(super) fn parse_rule_list(
    css: &str,
    base_url: &str,
    viewport_width: f32,
    next_order: &mut u32,
    output: &mut Vec<Rule>,
) {
    let mut cursor = 0;
    while cursor < css.len() {
        cursor = skip_css_whitespace(css, cursor);
        if cursor >= css.len() {
            break;
        }
        let Some(open) = find_css_delimiter(css, cursor, '{') else {
            break;
        };
        let prelude = css[cursor..open].trim();
        let Some(close) = find_matching_brace(css, open) else {
            break;
        };
        let body = &css[open + 1..close];
        if prelude.starts_with("@media") {
            if media_matches(prelude, viewport_width) {
                parse_rule_list(body, base_url, viewport_width, next_order, output);
            }
        } else if prelude.starts_with("@supports") {
            if supports::supports_matches(prelude) {
                parse_rule_list(body, base_url, viewport_width, next_order, output);
            }
        } else if !prelude.starts_with('@') {
            let declarations = parse_declarations(body);
            for selector_text in split_css_top_level(prelude, ',') {
                if let Some(selector) = parse_selector(selector_text.trim()) {
                    output.push(Rule {
                        selector,
                        declarations: declarations.clone(),
                        order: *next_order,
                        base_url: base_url.to_string(),
                    });
                    *next_order = next_order.wrapping_add(1);
                }
            }
        }
        cursor = close + 1;
    }
}

pub(super) fn strip_comments(css: &str) -> String {
    let mut output = String::with_capacity(css.len());
    let mut cursor = 0;
    while let Some(start_offset) = css[cursor..].find("/*") {
        let start = cursor + start_offset;
        output.push_str(&css[cursor..start]);
        let Some(end_offset) = css[start + 2..].find("*/") else {
            return output;
        };
        cursor = start + 2 + end_offset + 2;
    }
    output.push_str(&css[cursor..]);
    output
}

pub(super) fn parse_declarations(body: &str) -> Vec<Declaration> {
    split_css_top_level(body, ';')
        .filter_map(|declaration| {
            let (name, value) = split_css_once(declaration, ':')?;
            let name = name.trim();
            let name = if name.starts_with("--") {
                name.to_string()
            } else {
                name.to_ascii_lowercase()
            };
            let (value, important) = split_important_annotation(value);
            (!name.is_empty() && !value.is_empty()).then_some(Declaration {
                name,
                value: value.to_string(),
                important,
            })
        })
        .collect()
}

fn split_important_annotation(value: &str) -> (&str, bool) {
    let value = value.trim();
    let Some(bang) = value.rfind('!') else {
        return (value, false);
    };
    if value[bang + 1..].trim().eq_ignore_ascii_case("important") {
        (value[..bang].trim_end(), true)
    } else {
        (value, false)
    }
}
