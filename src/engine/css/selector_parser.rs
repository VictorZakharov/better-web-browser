//! Selector tokenization and parsing.

use super::*;
mod attributes;
mod functionals;
mod ident;
mod structural;
mod tokens;
pub(super) use attributes::parse_attribute_selector;
use tokens::selector_tokens;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PseudoElement {
    Before,
    After,
    Placeholder,
}

pub(super) fn parse_style_rule_selector(input: &str) -> Option<(Selector, Option<PseudoElement>)> {
    parse_style_rule_selector_with_parent(input, None, None)
}

pub(super) fn parse_style_rule_selector_with_parent(
    input: &str,
    parent: Option<&[Selector]>,
    parent_specificity: Option<Specificity>,
) -> Option<(Selector, Option<PseudoElement>)> {
    let input = input.trim();
    let lower = input.to_ascii_lowercase();
    let (origin, pseudo) = [
        ("::before", PseudoElement::Before),
        ("::after", PseudoElement::After),
        ("::placeholder", PseudoElement::Placeholder),
        (":before", PseudoElement::Before),
        (":after", PseudoElement::After),
    ]
    .into_iter()
    .find_map(|(suffix, pseudo)| {
        lower
            .strip_suffix(suffix)
            .map(|origin| (&input[..origin.len()], pseudo))
    })
    .map_or((input, None), |(origin, pseudo)| (origin, Some(pseudo)));
    let origin = if origin.trim().is_empty() {
        "*"
    } else {
        origin.trim()
    };
    let mut selector = if let Some(parent) = parent {
        let source = if contains_nesting_selector(origin) {
            origin.to_string()
        } else {
            format!("& {origin}")
        };
        parse_selector_with_depth(&source, 0, Some(parent), parent_specificity, false)?
    } else {
        parse_selector(origin)?
    };
    if pseudo.is_some() {
        selector.specificity.tags = selector.specificity.tags.saturating_add(1);
    }
    Some((selector, pseudo))
}

pub(super) fn parse_selector(input: &str) -> Option<Selector> {
    parse_selector_with_depth(input, 0, None, None, false)
}

fn contains_nesting_selector(input: &str) -> bool {
    let mut quote = None;
    let mut cursor = 0;
    while let Some(character) = ident::next_char(input, cursor) {
        if character == '\\'
            && let Some((_, end)) = ident::consume_escape(input, cursor)
        {
            cursor = end;
            continue;
        }
        match (quote, character) {
            (Some(active), candidate) if candidate == active => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(character),
            (None, '&') => return true,
            _ => {}
        }
        cursor += character.len_utf8();
    }
    false
}

// Functional selector lists are recursive. Bound parsing to avoid unbounded work on hostile CSS.
fn parse_selector_with_depth(
    input: &str,
    depth: usize,
    parent: Option<&[Selector]>,
    parent_specificity: Option<Specificity>,
    inside_has: bool,
) -> Option<Selector> {
    if depth > 8 {
        return None;
    }
    if input.is_empty() {
        return None;
    }
    let tokens = selector_tokens(input);
    if tokens.is_empty() {
        return None;
    }
    let mut compounds = Vec::new();
    let mut combinators = Vec::new();
    let mut specificity = Specificity::default();
    let mut expect_compound = true;
    for token in tokens {
        match token {
            SelectorToken::Compound(text) => {
                let (compound, compound_specificity) = parse_compound_selector_with_depth(
                    &text,
                    depth,
                    parent,
                    parent_specificity,
                    inside_has,
                )?;
                if !expect_compound {
                    combinators.push(Combinator::Descendant);
                }
                compounds.push(compound);
                specificity.ids += compound_specificity.ids;
                specificity.classes += compound_specificity.classes;
                specificity.tags += compound_specificity.tags;
                expect_compound = false;
            }
            SelectorToken::Combinator(combinator) if !expect_compound => {
                if combinators.len() < compounds.len() {
                    combinators.push(combinator);
                } else if let Some(last) = combinators.last_mut() {
                    *last = combinator;
                }
                expect_compound = true;
            }
            SelectorToken::Combinator(_) => return None,
        }
    }
    if compounds.is_empty() || expect_compound || combinators.len() + 1 != compounds.len() {
        return None;
    }
    Some(Selector {
        compounds: std::rc::Rc::new(compounds),
        combinators: std::rc::Rc::new(combinators),
        specificity,
    })
}

pub(super) enum SelectorToken {
    Compound(String),
    Combinator(Combinator),
}

fn parse_compound_selector_with_depth(
    input: &str,
    depth: usize,
    parent: Option<&[Selector]>,
    parent_specificity: Option<Specificity>,
    inside_has: bool,
) -> Option<(CompoundSelector, Specificity)> {
    let mut compound = CompoundSelector::default();
    let mut specificity = Specificity::default();
    let bytes = input.as_bytes();
    let mut cursor = 0;
    if bytes.first().is_some_and(|byte| *byte == b'*') {
        cursor = 1;
    } else if let Some((tag, end)) = ident::parse_identifier(input, cursor) {
        compound.tag = Some(tag.to_ascii_lowercase());
        specificity.tags += 1;
        cursor = end;
    }

    while cursor < bytes.len() {
        match bytes[cursor] {
            b'&' => {
                if let Some(parent) = parent {
                    if parent.is_empty() {
                        compound.never_matches = true;
                    } else {
                        let parent_specificity = parent_specificity.unwrap_or_else(|| {
                            parent
                                .iter()
                                .map(|selector| selector.specificity)
                                .max()
                                .unwrap_or_default()
                        });
                        specificity.ids = specificity.ids.saturating_add(parent_specificity.ids);
                        specificity.classes = specificity
                            .classes
                            .saturating_add(parent_specificity.classes);
                        specificity.tags = specificity.tags.saturating_add(parent_specificity.tags);
                        compound.functional.push(FunctionalSelector {
                            kind: FunctionalSelectorKind::Is,
                            selectors: parent.to_vec(),
                        });
                    }
                } else {
                    // CSS Nesting: a top-level `&` is :scope with zero specificity.
                    compound.requires_scope = true;
                }
                cursor += 1;
            }
            b'#' => {
                let (id, end) = ident::parse_identifier(input, cursor + 1)?;
                compound.id = Some(id);
                specificity.ids += 1;
                cursor = end;
            }
            b'.' => {
                let (class, end) = ident::parse_identifier(input, cursor + 1)?;
                compound.classes.push(class);
                specificity.classes += 1;
                cursor = end;
            }
            b':' if bytes.get(cursor + 1) == Some(&b':') => return None,
            b':' => {
                let (name, name_end) = ident::parse_identifier(input, cursor + 1)?;
                let name = name.to_ascii_lowercase();
                cursor = name_end;
                if cursor < bytes.len() && bytes[cursor] == b'(' {
                    let end = find_matching_parenthesis(input, cursor)?;
                    let argument = input[cursor + 1..end].trim();
                    functionals::parse_functional_pseudo(
                        &name,
                        argument,
                        depth,
                        parent,
                        parent_specificity,
                        inside_has,
                        &mut compound,
                        &mut specificity,
                    )?;
                    cursor = end + 1;
                } else {
                    specificity.classes += 1;
                    match name.as_str() {
                        "link" | "any-link" => compound.requires_link = true,
                        "first-child" => compound.requires_first_child = true,
                        "first-of-type" => compound.requires_first_of_type = true,
                        "last-child" => compound.requires_last_child = true,
                        "last-of-type" => compound.requires_last_of_type = true,
                        "only-child" => compound.requires_only_child = true,
                        "only-of-type" => compound.requires_only_of_type = true,
                        "empty" => compound.requires_empty = true,
                        "root" => compound.requires_root = true,
                        "scope" => compound.requires_scope = true,
                        "enabled" => compound.requires_enabled = true,
                        "disabled" => compound.requires_disabled = true,
                        "read-write" => compound.requires_read_write = true,
                        "read-only" => compound.requires_read_only = true,
                        "fullscreen" => compound.requires_fullscreen = true,
                        "hover" => compound.requires_hover = true,
                        "focus" => compound.requires_focus = true,
                        "focus-within" => compound.requires_focus_within = true,
                        "checked" => compound.requires_checked = true,
                        "indeterminate" => compound.requires_indeterminate = true,
                        "valid" => compound.requires_valid = true,
                        "invalid" => compound.requires_invalid = true,
                        "required" => compound.requires_required = true,
                        "optional" => compound.requires_optional = true,
                        "in-range" => compound.requires_in_range = true,
                        "out-of-range" => compound.requires_out_of_range = true,
                        "active" | "visited" | "focus-visible" => compound.never_matches = true,
                        _ => compound.never_matches = true,
                    }
                }
            }
            b'[' => {
                let end = tokens::find_attribute_end(input, cursor)?;
                let attribute = parse_attribute_selector(&input[cursor + 1..end])?;
                compound.attributes.push(attribute);
                specificity.classes += 1;
                cursor = end + 1;
            }
            _ => return None,
        }
    }
    Some((compound, specificity))
}

fn selector_is_supported(selector: &Selector) -> bool {
    selector.compounds.iter().all(|compound| {
        !compound.never_matches
            && compound
                .has
                .iter()
                .flatten()
                .all(|relative| selector_is_supported(&relative.selector))
            && compound
                .nth
                .iter()
                .flat_map(|nth| &nth.filter)
                .all(selector_is_supported)
            && compound
                .functional
                .iter()
                .all(|function| function.selectors.iter().all(selector_is_supported))
    })
}

#[cfg(test)]
#[path = "selector_parser_tests.rs"]
mod tests;
