//! Selectors Level 4 structural-function syntax.
use super::*;

pub(super) fn parse_nth_selector(
    name: &str,
    argument: &str,
    depth: usize,
    parent: Option<&[Selector]>,
    parent_specificity: Option<Specificity>,
    inside_has: bool,
) -> Option<(NthSelector, Specificity)> {
    let of_type = matches!(name, "nth-of-type" | "nth-last-of-type");
    let from_end = matches!(name, "nth-last-child" | "nth-last-of-type");
    let mut input = ParserInput::new(argument);
    let mut parser = Parser::new(&mut input);
    let (a, b) = cssparser::parse_nth(&mut parser).ok()?;
    let mut filter = Vec::new();
    if !parser.is_exhausted() {
        if of_type
            || !matches!(parser.next(), Ok(Token::Ident(name)) if name.eq_ignore_ascii_case("of"))
        {
            return None;
        }
        let remaining = argument[parser.position().byte_index()..].trim();
        if remaining.is_empty() {
            return None;
        }
        for member in split_css_top_level(remaining, ',') {
            let member = member.trim();
            if member.is_empty() {
                return None;
            }
            let selector = parse_selector_with_depth(
                member,
                depth + 1,
                parent,
                parent_specificity,
                inside_has,
            )?;
            if !selector_is_supported(&selector) {
                return None;
            }
            filter.push(selector);
        }
    }
    let filter_specificity = filter
        .iter()
        .map(|selector| selector.specificity)
        .max()
        .unwrap_or_default();
    Some((
        NthSelector {
            a,
            b,
            from_end,
            of_type,
            filter,
        },
        filter_specificity,
    ))
}
