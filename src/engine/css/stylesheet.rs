//! Stylesheet rule and declaration parsing.

use super::media::MediaEnvironment;
use super::*;
use crate::limits::{
    MAX_CSS_DECLARATIONS_PER_RULE, MAX_CSS_NESTING_DEPTH, MAX_CSS_RULES_PER_STYLESHEET,
    MAX_CSS_SOURCE_BYTES, MAX_PAGE_CSS_RULES, bounded_utf8_prefix,
};

#[derive(Debug)]
pub(super) struct Rule {
    pub(super) selector: Selector,
    pub(super) pseudo: Option<PseudoElement>,
    pub(super) host_condition: Option<Selector>,
    pub(super) declarations: Vec<Declaration>,
    pub(super) order: u32,
    pub(super) base_url: String,
    pub(super) scope: RuleScope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RuleScope {
    Document,
    Shadow(NodeId),
    Host(NodeId),
    Slotted(NodeId),
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
    media_environment: MediaEnvironment,
    next_order: &mut u32,
    output: &mut Vec<Rule>,
    scope: RuleScope,
) {
    if output.len() >= MAX_PAGE_CSS_RULES {
        return;
    }
    let (css, _) = bounded_utf8_prefix(css, MAX_CSS_SOURCE_BYTES);
    let css = strip_comments(css);
    let rule_limit = output
        .len()
        .saturating_add(MAX_CSS_RULES_PER_STYLESHEET)
        .min(MAX_PAGE_CSS_RULES);
    parse_rule_list(
        &css,
        base_url,
        media_environment,
        next_order,
        output,
        scope,
        0,
        rule_limit,
    );
}

fn parse_rule_list(
    css: &str,
    base_url: &str,
    media_environment: MediaEnvironment,
    next_order: &mut u32,
    output: &mut Vec<Rule>,
    scope: RuleScope,
    nesting_depth: usize,
    rule_limit: usize,
) {
    if nesting_depth >= MAX_CSS_NESTING_DEPTH || output.len() >= rule_limit {
        return;
    }
    let mut cursor = 0;
    while cursor < css.len() && output.len() < rule_limit {
        cursor = skip_css_whitespace(css, cursor);
        if cursor >= css.len() {
            break;
        }
        if css.as_bytes()[cursor] == b'@'
            && let Some(semicolon) = find_css_delimiter(css, cursor, ';')
            && find_css_delimiter(css, cursor, '{').is_none_or(|open| semicolon < open)
        {
            // CSS Syntax allows statement at-rules such as @charset and @import to end with a
            // semicolon. They do not own the next qualified-rule block. Unknown statements are
            // ignored here; resource loading handles imports separately.
            cursor = semicolon + 1;
            continue;
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
            if media::media_matches_for_environment(prelude, media_environment) {
                parse_rule_list(
                    body,
                    base_url,
                    media_environment,
                    next_order,
                    output,
                    scope,
                    nesting_depth + 1,
                    rule_limit,
                );
            }
        } else if prelude.starts_with("@supports") {
            if supports::supports_matches(prelude) {
                parse_rule_list(
                    body,
                    base_url,
                    media_environment,
                    next_order,
                    output,
                    scope,
                    nesting_depth + 1,
                    rule_limit,
                );
            }
        } else if !prelude.starts_with('@') {
            let declarations = parse_declarations(body);
            for selector_text in split_css_top_level(prelude, ',') {
                if output.len() >= rule_limit {
                    break;
                }
                let Some((selector_text, rule_scope, host_condition_text)) =
                    scoped_selector(selector_text.trim(), scope)
                else {
                    continue;
                };
                let host_condition = match host_condition_text {
                    Some(condition) => {
                        let Some(condition) = parse_selector(condition) else {
                            continue;
                        };
                        Some(condition)
                    }
                    None => None,
                };
                if let Some((mut selector, pseudo)) = parse_style_rule_selector(selector_text) {
                    if let Some(condition) = host_condition.as_ref() {
                        selector.specificity.ids = selector
                            .specificity
                            .ids
                            .saturating_add(condition.specificity.ids);
                        selector.specificity.classes = selector
                            .specificity
                            .classes
                            .saturating_add(condition.specificity.classes);
                        selector.specificity.tags = selector
                            .specificity
                            .tags
                            .saturating_add(condition.specificity.tags);
                    }
                    output.push(Rule {
                        selector,
                        pseudo,
                        host_condition,
                        declarations: declarations.clone(),
                        order: *next_order,
                        base_url: base_url.to_string(),
                        scope: rule_scope,
                    });
                    *next_order = next_order.wrapping_add(1);
                }
            }
        }
        cursor = close + 1;
    }
}

fn scoped_selector(selector: &str, scope: RuleScope) -> Option<(&str, RuleScope, Option<&str>)> {
    let RuleScope::Shadow(root) = scope else {
        return Some((selector, scope, None));
    };
    if selector == ":host" {
        return Some(("*", RuleScope::Host(root), None));
    }
    if let Some(condition) = selector
        .strip_prefix(":host(")
        .and_then(|value| value.strip_suffix(')'))
    {
        return Some((condition.trim(), RuleScope::Host(root), None));
    }
    if let Some(slotted) = selector
        .strip_prefix("::slotted(")
        .and_then(|value| value.strip_suffix(')'))
    {
        return Some((slotted.trim(), RuleScope::Slotted(root), None));
    }
    if let Some((condition, descendant)) = split_host_descendant(selector) {
        return Some((descendant, RuleScope::Shadow(root), Some(condition)));
    }
    // Complex :host/::slotted selectors need selector-tree boundary representation. Ignoring
    // them is safer than leaking a shadow rule into the document tree.
    if selector.contains(":host") || selector.contains("::slotted") {
        return None;
    }
    Some((selector, scope, None))
}

fn split_host_descendant(selector: &str) -> Option<(&str, &str)> {
    let after_host = selector.strip_prefix(":host")?;
    let (condition, remainder) = if after_host.starts_with('(') {
        let open = selector.len() - after_host.len();
        let close = find_matching_parenthesis(selector, open)?;
        (&selector[open + 1..close], &selector[close + 1..])
    } else {
        ("*", after_host)
    };
    let descendant = remainder.trim_start();
    if descendant.is_empty() || descendant.starts_with(['>', '+', '~']) {
        return None;
    }
    Some((condition.trim(), descendant))
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
        .take(MAX_CSS_DECLARATIONS_PER_RULE)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_large_earlier_stylesheet_does_not_starve_later_cascade_rules() {
        let first = ".early{color:red}".repeat(20_000);
        let mut rules = Vec::new();
        let mut order = 0;
        let environment = MediaEnvironment::new(1280.0, 720.0, 1.0, false);

        parse_stylesheet(
            &first,
            "https://example.test/early.css",
            environment,
            &mut order,
            &mut rules,
            RuleScope::Document,
        );
        parse_stylesheet(
            ".late{display:block}",
            "https://example.test/late.css",
            environment,
            &mut order,
            &mut rules,
            RuleScope::Document,
        );

        assert_eq!(rules.len(), 20_001);
        assert_eq!(rules.last().unwrap().order, 20_000);
    }

    #[test]
    fn semicolon_at_rules_do_not_consume_the_following_qualified_rule() {
        let mut rules = Vec::new();
        let mut order = 0;
        let environment = MediaEnvironment::new(1280.0, 720.0, 1.0, false);

        parse_stylesheet(
            r#"@charset "UTF-8";@import url("theme.css");
               .player{position:relative;width:100%;height:100%}"#,
            "https://example.test/player.css",
            environment,
            &mut order,
            &mut rules,
            RuleScope::Document,
        );

        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].selector.compounds[0].classes, ["player"]);
        assert_eq!(
            rules[0]
                .declarations
                .iter()
                .map(|declaration| (declaration.name.as_str(), declaration.value.as_str()))
                .collect::<Vec<_>>(),
            [
                ("position", "relative"),
                ("width", "100%"),
                ("height", "100%")
            ]
        );
    }
}
