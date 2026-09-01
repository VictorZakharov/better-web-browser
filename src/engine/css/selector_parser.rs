//! Selector tokenization and parsing.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PseudoElement {
    Before,
    After,
}

pub(super) fn parse_style_rule_selector(input: &str) -> Option<(Selector, Option<PseudoElement>)> {
    let input = input.trim();
    let lower = input.to_ascii_lowercase();
    let (origin, pseudo) = [
        ("::before", PseudoElement::Before),
        ("::after", PseudoElement::After),
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
    let mut selector = parse_selector(origin)?;
    if pseudo.is_some() {
        selector.specificity.tags = selector.specificity.tags.saturating_add(1);
    }
    Some((selector, pseudo))
}

pub(super) fn parse_selector(input: &str) -> Option<Selector> {
    if input.is_empty() || input.contains("::") {
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
                let (compound, compound_specificity) = parse_compound_selector(&text)?;
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
            SelectorToken::Combinator(_) => {}
        }
    }
    if compounds.is_empty() || expect_compound || combinators.len() + 1 != compounds.len() {
        return None;
    }
    Some(Selector {
        compounds,
        combinators,
        specificity,
    })
}

pub(super) enum SelectorToken {
    Compound(String),
    Combinator(Combinator),
}

pub(super) fn selector_tokens(input: &str) -> Vec<SelectorToken> {
    let mut tokens = Vec::new();
    let mut start = 0;
    let mut depth = 0_i32;
    let mut in_attribute = 0_i32;
    let mut pending_space = false;
    for (index, character) in input.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => depth = (depth - 1).max(0),
            '[' => in_attribute += 1,
            ']' => in_attribute = (in_attribute - 1).max(0),
            '>' | '+' | '~' if depth == 0 && in_attribute == 0 => {
                if start < index {
                    let text = input[start..index].trim();
                    if !text.is_empty() {
                        tokens.push(SelectorToken::Compound(text.to_string()));
                    }
                }
                let combinator = match character {
                    '>' => Combinator::Child,
                    '+' => Combinator::AdjacentSibling,
                    '~' => Combinator::GeneralSibling,
                    _ => unreachable!(),
                };
                tokens.push(SelectorToken::Combinator(combinator));
                start = index + character.len_utf8();
                pending_space = false;
            }
            character if character.is_whitespace() && depth == 0 && in_attribute == 0 => {
                if start < index {
                    let text = input[start..index].trim();
                    if !text.is_empty() {
                        tokens.push(SelectorToken::Compound(text.to_string()));
                    }
                }
                start = index + character.len_utf8();
                pending_space = true;
            }
            _ => {
                if pending_space {
                    if !matches!(tokens.last(), Some(SelectorToken::Combinator(_))) {
                        tokens.push(SelectorToken::Combinator(Combinator::Descendant));
                    }
                    pending_space = false;
                }
            }
        }
    }
    if start < input.len() {
        let text = input[start..].trim();
        if !text.is_empty() {
            tokens.push(SelectorToken::Compound(text.to_string()));
        }
    }
    while matches!(tokens.last(), Some(SelectorToken::Combinator(_))) {
        tokens.pop();
    }
    tokens
}

pub(super) fn parse_compound_selector(input: &str) -> Option<(CompoundSelector, Specificity)> {
    let mut compound = CompoundSelector::default();
    let mut specificity = Specificity::default();
    let bytes = input.as_bytes();
    let mut cursor = 0;
    if bytes.first().is_some_and(|byte| *byte == b'*') {
        cursor = 1;
    } else if bytes
        .first()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'-')
    {
        let end = consume_identifier(bytes, cursor);
        compound.tag = Some(input[cursor..end].to_ascii_lowercase());
        specificity.tags += 1;
        cursor = end;
    }

    while cursor < bytes.len() {
        match bytes[cursor] {
            b'#' => {
                let end = consume_identifier(bytes, cursor + 1);
                if end == cursor + 1 {
                    return None;
                }
                compound.id = Some(input[cursor + 1..end].to_string());
                specificity.ids += 1;
                cursor = end;
            }
            b'.' => {
                let end = consume_identifier(bytes, cursor + 1);
                if end == cursor + 1 {
                    return None;
                }
                compound.classes.push(input[cursor + 1..end].to_string());
                specificity.classes += 1;
                cursor = end;
            }
            b':' => {
                let name_end = consume_identifier(bytes, cursor + 1);
                let name = input[cursor + 1..name_end].to_ascii_lowercase();
                cursor = name_end;
                if cursor < bytes.len() && bytes[cursor] == b'(' {
                    let end = find_matching_parenthesis(input, cursor)?;
                    let argument = input[cursor + 1..end].trim();
                    match name.as_str() {
                        "is" | "where" | "not" => {
                            if let Some(selectors) = parse_simple_selector_list(argument) {
                                if name != "where" {
                                    let argument_specificity = selectors
                                        .iter()
                                        .map(simple_selector_specificity)
                                        .max()
                                        .unwrap_or_default();
                                    specificity.ids += argument_specificity.ids;
                                    specificity.classes += argument_specificity.classes;
                                    specificity.tags += argument_specificity.tags;
                                }
                                if name == "not" {
                                    compound.not.push(selectors);
                                } else {
                                    compound.any_of.push(selectors);
                                }
                            } else {
                                compound.never_matches = true;
                            }
                        }
                        _ => compound.never_matches = true,
                    }
                    cursor = end + 1;
                } else {
                    specificity.classes += 1;
                    match name.as_str() {
                        "link" | "any-link" => compound.requires_link = true,
                        "first-child" => compound.requires_first_child = true,
                        "first-of-type" => compound.requires_first_of_type = true,
                        "last-child" => compound.requires_last_child = true,
                        "root" => compound.requires_root = true,
                        "enabled" => compound.requires_enabled = true,
                        "disabled" => compound.requires_disabled = true,
                        "fullscreen" => compound.requires_fullscreen = true,
                        "hover" | "active" | "focus" | "visited" | "focus-visible" => {
                            compound.never_matches = true
                        }
                        _ => compound.never_matches = true,
                    }
                }
            }
            b'[' => {
                let relative_end = input[cursor + 1..].find(']')?;
                let end = cursor + 1 + relative_end;
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

pub(super) fn parse_attribute_selector(input: &str) -> Option<AttributeSelector> {
    let mut expression = input.trim();
    let mut case_insensitive = false;
    if let Some((modifier_start, modifier)) = expression.char_indices().next_back() {
        let prefix = &expression[..modifier_start];
        if matches!(modifier, 'i' | 'I' | 's' | 'S')
            && prefix.chars().next_back().is_some_and(char::is_whitespace)
        {
            case_insensitive = matches!(modifier, 'i' | 'I');
            expression = prefix.trim_end();
        }
    }

    let operators = [
        ("~=", AttributeOperator::Includes),
        ("|=", AttributeOperator::DashMatch),
        ("^=", AttributeOperator::Prefix),
        ("$=", AttributeOperator::Suffix),
        ("*=", AttributeOperator::Substring),
        ("=", AttributeOperator::Equals),
    ];
    for (token, operator) in operators {
        if let Some(index) = expression.find(token) {
            let name = expression[..index].trim().to_ascii_lowercase();
            let value = expression[index + token.len()..]
                .trim()
                .trim_matches(['\'', '"'])
                .to_string();
            return (!name.is_empty()).then_some(AttributeSelector {
                name,
                operator,
                value,
                case_insensitive,
            });
        }
    }

    let name = expression.to_ascii_lowercase();
    (!name.is_empty()).then_some(AttributeSelector {
        name,
        operator: AttributeOperator::Exists,
        value: String::new(),
        case_insensitive,
    })
}

pub(super) fn parse_simple_selector(input: &str) -> Option<SimpleSelector> {
    if let Some(id) = input.strip_prefix('#') {
        Some(SimpleSelector::Id(id.to_string()))
    } else if let Some(class) = input.strip_prefix('.') {
        Some(SimpleSelector::Class(class.to_string()))
    } else if input.starts_with('[') && input.ends_with(']') {
        parse_attribute_selector(&input[1..input.len() - 1]).map(SimpleSelector::Attribute)
    } else if !input.is_empty() {
        Some(SimpleSelector::Tag(input.to_ascii_lowercase()))
    } else {
        None
    }
}

pub(super) fn parse_simple_selector_list(input: &str) -> Option<Vec<SimpleSelector>> {
    let selectors = split_css_top_level(input, ',')
        .map(str::trim)
        .map(parse_simple_selector)
        .collect::<Option<Vec<_>>>()?;
    (!selectors.is_empty()).then_some(selectors)
}

pub(super) fn simple_selector_specificity(selector: &SimpleSelector) -> Specificity {
    match selector {
        SimpleSelector::Id(_) => Specificity {
            ids: 1,
            ..Specificity::default()
        },
        SimpleSelector::Class(_) => Specificity {
            classes: 1,
            ..Specificity::default()
        },
        SimpleSelector::Attribute(_) => Specificity {
            classes: 1,
            ..Specificity::default()
        },
        SimpleSelector::Tag(_) => Specificity {
            tags: 1,
            ..Specificity::default()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzz_regression_non_ascii_attribute_suffix_does_not_panic() {
        let attribute = parse_attribute_selector("data-value=\"x\"�").unwrap();

        assert_eq!(attribute.name, "data-value");
        assert!(!attribute.case_insensitive);
    }

    #[test]
    fn attribute_modifiers_accept_css_whitespace_without_byte_slicing() {
        assert!(
            parse_attribute_selector("data-value=\"x\"\tI")
                .unwrap()
                .case_insensitive
        );
        assert!(
            !parse_attribute_selector("data-value=\"x\"\nS")
                .unwrap()
                .case_insensitive
        );
    }

    #[test]
    fn generated_pseudo_elements_use_the_originating_selector() {
        let (selector, pseudo) = parse_style_rule_selector(".card:before").unwrap();
        assert_eq!(pseudo, Some(PseudoElement::Before));
        assert_eq!(selector.specificity.classes, 1);
        assert_eq!(selector.specificity.tags, 1);

        let (selector, pseudo) = parse_style_rule_selector("#footer::AFTER").unwrap();
        assert_eq!(pseudo, Some(PseudoElement::After));
        assert_eq!(selector.specificity.ids, 1);
        assert_eq!(selector.specificity.tags, 1);
        assert!(parse_style_rule_selector(".card::marker").is_none());
    }
}
