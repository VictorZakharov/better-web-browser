//! CSS Cascade 6 scope boundaries and scoped selector preludes.
use super::*;
use crate::engine::css::selector_parser::parse_style_rule_selector_with_parent;

#[derive(Clone, Debug)]
pub(in crate::engine::css) struct CssScope {
    pub(in crate::engine::css) start: Option<Vec<Selector>>,
    pub(in crate::engine::css) end: Option<Vec<Selector>>,
    pub(in crate::engine::css) implicit_root: Option<NodeId>,
}

pub(super) fn parse_scope_prelude(
    prelude: &str,
    nested: bool,
    implicit_root: Option<NodeId>,
) -> Option<CssScope> {
    let (start_source, end_source) = boundary_sources(prelude)?;
    let start = start_source
        .map(|source| parse_boundary_selectors(source, nested))
        .transpose_option()?;
    let end = end_source
        .map(|source| parse_boundary_selectors(source, true))
        .transpose_option()?;
    Some(CssScope {
        start,
        end,
        implicit_root,
    })
}

pub(super) fn parse_scope_prelude_in_style(
    prelude: &str,
    parents: &[Selector],
    implicit_root: Option<NodeId>,
) -> Option<CssScope> {
    let (start_source, end_source) = boundary_sources(prelude)?;
    let start = start_source
        .map(|source| parse_style_parent_start(source, parents))
        .transpose_option()?;
    let end = end_source
        .map(|source| parse_boundary_selectors(source, true))
        .transpose_option()?;
    Some(CssScope {
        start,
        end,
        implicit_root,
    })
}

pub(super) fn parse_scope_contents(
    body: &str,
    depth: usize,
    layer: &[LayerSegment],
    context: &mut nesting::Context<'_>,
) {
    if depth >= MAX_CSS_NESTING_DEPTH {
        return;
    }
    for item in nesting::block_items(body) {
        if context.output.len() >= context.rule_limit {
            break;
        }
        match item {
            nesting::BlockItem::Declarations(text) => {
                nesting::emit_scoped_declarations(text, layer, context);
            }
            nesting::BlockItem::Rule { source, .. } => {
                parse_rule_list(
                    source,
                    context.base_url,
                    context.environment,
                    context.next_order,
                    context.output,
                    context.scope,
                    depth,
                    context.rule_limit,
                    layer,
                    context.css_scopes,
                    context.relative_scope_selectors,
                    context.implicit_scope_root,
                    context.next_anonymous,
                    context.events,
                );
            }
            nesting::BlockItem::Statement(text) => {
                if let Some(paths) = parse_layer_statement(text) {
                    for name in paths {
                        let mut path = layer.to_vec();
                        path.extend(name);
                        context.events.push(LayerEvent::Declare(path));
                    }
                }
            }
        }
    }
}

fn parse_style_parent_start(source: &str, parents: &[Selector]) -> Option<Vec<Selector>> {
    if parents.is_empty() {
        return None;
    }
    let specificity = parents.iter().map(|parent| parent.specificity).max();
    split_css_top_level(source, ',')
        .map(str::trim)
        .map(|member| {
            let (selector, pseudo) =
                parse_style_rule_selector_with_parent(member, Some(parents), specificity)?;
            (pseudo.is_none() && super::super::selector_parser::selector_is_supported(&selector))
                .then_some(selector)
        })
        .collect::<Option<Vec<_>>>()
        .filter(|selectors| !selectors.is_empty())
}

pub(crate) fn scope_boundary_text(prelude: &str) -> Option<(Option<String>, Option<String>)> {
    parse_scope_prelude(prelude, false, None)?;
    let (start, end) = boundary_sources(prelude)?;
    let serialize = |source: &str| {
        split_css_top_level(source, ',')
            .map(str::trim)
            .collect::<Vec<_>>()
            .join(", ")
    };
    Some((start.map(serialize), end.map(serialize)))
}

/// `scope(.card)` contains a selector, whereas `scope((.card) to (.limit))`
/// contains the same boundary grammar as an `@scope` prelude. Normalize only
/// for the cascade parser; CSSOM retains the actual function contents.
pub(crate) fn import_scope_prelude(contents: &str) -> Option<String> {
    let contents = contents.trim();
    let starts_with_to = contents
        .get(..2)
        .is_some_and(|word| word.eq_ignore_ascii_case("to"))
        && contents[2..].starts_with(char::is_whitespace);
    let prelude = if contents.is_empty() || contents.starts_with('(') || starts_with_to {
        contents.to_string()
    } else {
        format!("({contents})")
    };
    parse_scope_prelude(&prelude, false, None)?;
    Some(prelude)
}

fn boundary_sources(prelude: &str) -> Option<(Option<&str>, Option<&str>)> {
    let mut rest = prelude.trim();
    let start = if rest.starts_with('(') {
        let (contents, remainder) = parenthesized(rest)?;
        rest = remainder;
        Some(contents.trim())
    } else {
        None
    };
    let end = if rest.is_empty() {
        None
    } else {
        let after_to = rest
            .get(..2)
            .filter(|word| word.eq_ignore_ascii_case("to"))?;
        let remainder = &rest[after_to.len()..];
        if !remainder.starts_with(char::is_whitespace) {
            return None;
        }
        let (contents, tail) = parenthesized(remainder.trim_start())?;
        if !tail.is_empty() {
            return None;
        }
        Some(contents.trim())
    };
    Some((start, end))
}

trait TransposeOption<T> {
    fn transpose_option(self) -> Option<Option<T>>;
}

impl<T> TransposeOption<T> for Option<Option<T>> {
    fn transpose_option(self) -> Option<Option<T>> {
        match self {
            Some(Some(value)) => Some(Some(value)),
            Some(None) => None,
            None => Some(None),
        }
    }
}

fn parenthesized(input: &str) -> Option<(&str, &str)> {
    if !input.starts_with('(') {
        return None;
    }
    let close = find_matching_parenthesis(input, 0)?;
    Some((&input[1..close], input[close + 1..].trim()))
}

fn parse_boundary_selectors(input: &str, relative_to_outer: bool) -> Option<Vec<Selector>> {
    let mut selectors = Vec::new();
    for member in split_css_top_level(input, ',') {
        let member = member.trim();
        if member.is_empty() {
            return None;
        }
        let source = if relative_to_outer {
            scoped_selector_source(member)
        } else {
            member.to_string()
        };
        // Scope boundary lists are unforgiving; an unsupported member invalidates the rule.
        let selector = parse_selector(&source)?;
        if !super::super::selector_parser::selector_is_supported(&selector) {
            return None;
        }
        selectors.push(selector);
    }
    (!selectors.is_empty()).then_some(selectors)
}

/// The implicit anchor has zero specificity. An explicit :scope keeps class specificity.
pub(super) fn scoped_selector_source(source: &str) -> String {
    let source = source.trim();
    if has_scope_anchor(source) && !source.starts_with(['>', '+', '~']) {
        source.to_string()
    } else {
        format!(":where(:scope) {source}")
    }
}

fn has_scope_anchor(input: &str) -> bool {
    let mut quote = None;
    let mut brackets = 0_u32;
    let mut escaped = false;
    let mut cursor = 0;
    while cursor < input.len() {
        let character = input[cursor..].chars().next().unwrap();
        let width = character.len_utf8();
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if let Some(active) = quote {
            if character == active {
                quote = None;
            }
        } else {
            match character {
                '\'' | '"' => quote = Some(character),
                '[' => brackets += 1,
                ']' => brackets = brackets.saturating_sub(1),
                '&' if brackets == 0 => return true,
                ':' if brackets == 0 => {
                    let tail = &input[cursor + width..];
                    if tail
                        .get(..5)
                        .is_some_and(|word| word.eq_ignore_ascii_case("scope"))
                        && !tail[5..].starts_with(|ch: char| {
                            ch.is_ascii_alphanumeric() || ch == '-' || ch == '_'
                        })
                    {
                        return true;
                    }
                }
                _ => {}
            }
        }
        cursor += width;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundary_lists_are_unforgiving_and_pseudos_are_not_roots() {
        assert!(parse_scope_prelude("(.card, #item) to (.stop)", false, None).is_some());
        assert!(parse_scope_prelude("(.card, :unknown)", false, None).is_none());
        assert!(parse_scope_prelude("(.card::before)", false, None).is_none());
        assert!(parse_scope_prelude("(.card) extra", false, None).is_none());
        assert!(parse_scope_prelude("to (.stop)", false, None).is_some());
        assert!(parse_scope_prelude("(.card) to (> .stop)", false, None).is_some());
    }

    #[test]
    fn only_unanchored_scoped_selectors_get_an_implicit_root() {
        assert_eq!(scoped_selector_source("p"), ":where(:scope) p");
        assert_eq!(scoped_selector_source("> p"), ":where(:scope) > p");
        assert_eq!(scoped_selector_source(":scope > p"), ":scope > p");
        assert_eq!(scoped_selector_source("main & p"), "main & p");
        assert_eq!(
            scoped_selector_source("[data-x=':scope'] p"),
            ":where(:scope) [data-x=':scope'] p"
        );
    }

    #[test]
    fn boundary_text_normalizes_only_top_level_commas() {
        assert_eq!(
            scope_boundary_text("(.a,:is(.b, .c)) to (.x,.y)"),
            Some((Some(".a, :is(.b, .c)".into()), Some(".x, .y".into())))
        );
    }
}
