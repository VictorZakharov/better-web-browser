//! Functional pseudo-classes and their argument selector lists.

use super::*;
use cssparser::{Parser, ParserInput};

#[allow(clippy::too_many_arguments)]
pub(super) fn parse_functional_pseudo(
    name: &str,
    argument: &str,
    depth: usize,
    parent: Option<&[Selector]>,
    parent_specificity: Option<Specificity>,
    inside_has: bool,
    compound: &mut CompoundSelector,
    specificity: &mut Specificity,
) -> Option<()> {
    match name {
        "is" | "where" | "not" => parse_selector_list_pseudo(
            name,
            argument,
            depth,
            parent,
            parent_specificity,
            inside_has,
            compound,
            specificity,
        )?,
        "has" if !inside_has => parse_has(
            argument,
            depth,
            parent,
            parent_specificity,
            compound,
            specificity,
        )?,
        "lang" => {
            let ranges = split_css_top_level(argument, ',')
                .map(parse_language_range)
                .collect::<Option<Vec<_>>>()?;
            if ranges.is_empty() {
                return None;
            }
            compound.languages.push(ranges);
            specificity.classes = specificity.classes.saturating_add(1);
        }
        "dir" => {
            let direction = match argument.to_ascii_lowercase().as_str() {
                "ltr" => TextDirection::Ltr,
                "rtl" => TextDirection::Rtl,
                _ => return None,
            };
            compound.directions.push(direction);
            specificity.classes = specificity.classes.saturating_add(1);
        }
        "nth-child" | "nth-last-child" | "nth-of-type" | "nth-last-of-type" => {
            let (nth, filter_specificity) = structural::parse_nth_selector(
                name,
                argument,
                depth,
                parent,
                parent_specificity,
                inside_has,
            )?;
            specificity.classes = specificity
                .classes
                .saturating_add(1)
                .saturating_add(filter_specificity.classes);
            specificity.ids = specificity.ids.saturating_add(filter_specificity.ids);
            specificity.tags = specificity.tags.saturating_add(filter_specificity.tags);
            compound.nth.push(nth);
        }
        _ => compound.never_matches = true,
    }
    Some(())
}

fn parse_language_range(source: &str) -> Option<String> {
    let mut tokenizer = ParserInput::new(source);
    let mut parser = Parser::new(&mut tokenizer);
    let value = parser.expect_ident_or_string().ok()?.to_string();
    parser.is_exhausted().then_some(value)
}

#[allow(clippy::too_many_arguments)]
fn parse_selector_list_pseudo(
    name: &str,
    argument: &str,
    depth: usize,
    parent: Option<&[Selector]>,
    parent_specificity: Option<Specificity>,
    inside_has: bool,
    compound: &mut CompoundSelector,
    specificity: &mut Specificity,
) -> Option<()> {
    let parts = split_css_top_level(argument, ',')
        .map(str::trim)
        .collect::<Vec<_>>();
    let forgiving = name != "not";
    let selectors = parts
        .iter()
        .filter(|part| !part.is_empty())
        .map(|part| {
            parse_selector_with_depth(part, depth + 1, parent, parent_specificity, inside_has)
                .filter(selector_is_supported)
        })
        .collect::<Vec<_>>();
    if !forgiving
        && (parts.is_empty()
            || parts.iter().any(|part| part.is_empty())
            || selectors.iter().any(Option::is_none))
    {
        return None;
    }
    let selectors = selectors.into_iter().flatten().collect::<Vec<_>>();
    if selectors.is_empty() {
        compound.never_matches = true;
    } else {
        if name != "where" {
            add_specificity(
                specificity,
                selectors
                    .iter()
                    .map(|selector| selector.specificity)
                    .max()
                    .unwrap_or_default(),
            );
        }
        let kind = match name {
            "is" => FunctionalSelectorKind::Is,
            "where" => FunctionalSelectorKind::Where,
            _ => FunctionalSelectorKind::Not,
        };
        compound
            .functional
            .push(FunctionalSelector { kind, selectors });
    }
    Some(())
}

fn parse_has(
    argument: &str,
    depth: usize,
    parent: Option<&[Selector]>,
    parent_specificity: Option<Specificity>,
    compound: &mut CompoundSelector,
    specificity: &mut Specificity,
) -> Option<()> {
    let parts = split_css_top_level(argument, ',')
        .map(str::trim)
        .collect::<Vec<_>>();
    if parts.iter().any(|part| part.is_empty()) {
        return None;
    }
    let mut relatives = Vec::new();
    let mut argument_specificity = Specificity::default();
    for part in parts {
        let explicit_scope = part.strip_prefix(":scope").filter(|tail| {
            tail.is_empty()
                || tail.starts_with(char::is_whitespace)
                || tail.starts_with(['>', '+', '~', ':', '.', '#', '['])
        });
        let relative = explicit_scope.unwrap_or(part).trim_start();
        let source = if explicit_scope.is_some() {
            part.to_string()
        } else {
            format!(":scope {part}")
        };
        let selector =
            parse_selector_with_depth(&source, depth + 1, parent, parent_specificity, true)?;
        if !selector_is_supported(&selector) {
            return None;
        }
        let mut member_specificity = selector.specificity;
        if explicit_scope.is_none() {
            member_specificity.classes = member_specificity.classes.saturating_sub(1);
        }
        argument_specificity = argument_specificity.max(member_specificity);
        let simple = selector.compounds.len() == 2;
        let search = match (relative.as_bytes().first(), simple) {
            (Some(b'>'), true) => RelativeSearch::Children,
            (Some(b'+'), true) => RelativeSearch::NextSibling,
            (Some(b'~'), true) => RelativeSearch::LaterSiblings,
            (Some(b'+' | b'~'), false) => RelativeSearch::FollowingSiblings,
            _ => RelativeSearch::Descendants,
        };
        relatives.push(RelativeSelector { selector, search });
    }
    if relatives.is_empty() {
        return None;
    }
    // The synthetic :scope anchor contributes zero to :has() specificity.
    add_specificity(specificity, argument_specificity);
    compound.has.push(relatives);
    Some(())
}

fn add_specificity(into: &mut Specificity, argument: Specificity) {
    into.ids = into.ids.saturating_add(argument.ids);
    into.classes = into.classes.saturating_add(argument.classes);
    into.tags = into.tags.saturating_add(argument.tags);
}
