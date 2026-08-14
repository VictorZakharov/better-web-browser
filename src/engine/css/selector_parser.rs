//! Selector tokenization and parsing.

use super::*;

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
                        "root" => compound.requires_root = true,
                        "enabled" => compound.requires_enabled = true,
                        "disabled" => compound.requires_disabled = true,
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
    if expression.len() >= 2 {
        let suffix = &expression[expression.len() - 2..];
        if suffix.eq_ignore_ascii_case(" i") {
            case_insensitive = true;
            expression = expression[..expression.len() - 2].trim_end();
        } else if suffix.eq_ignore_ascii_case(" s") {
            expression = expression[..expression.len() - 2].trim_end();
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
        SimpleSelector::Tag(_) => Specificity {
            tags: 1,
            ..Specificity::default()
        },
    }
}
