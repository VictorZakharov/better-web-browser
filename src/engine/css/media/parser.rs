//! Media-query grammar and three-valued condition tree.

use super::lexer::{Token, tokenize, tokenize_list_prefix};
use super::{MediaEnvironment, Truth, feature};

const MAX_CONDITION_DEPTH: usize = 24;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum MediaQuery {
    Invalid,
    Type {
        name: String,
        condition: Option<Condition>,
        negated: bool,
    },
    Condition(Condition),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Condition {
    Feature(String),
    GeneralEnclosed(String),
    Group(Box<Self>),
    Not(Box<Self>),
    And(Vec<Self>),
    Or(Vec<Self>),
}

pub(super) fn parse_list(input: &str) -> Vec<MediaQuery> {
    if input.trim().is_empty() {
        return Vec::new();
    }
    // CSS Syntax consumes an unclosed block through the end of the list, but
    // queries before that block's top-level comma remain valid.
    let (tokens, complete) = tokenize_list_prefix(input);
    if tokens.is_empty() && complete {
        return Vec::new();
    }
    let mut queries = Vec::new();
    let mut begin = 0;
    for (index, token) in tokens.iter().enumerate() {
        if *token == Token::Comma {
            queries.push(parse_query(&tokens[begin..index]));
            begin = index + 1;
        }
    }
    queries.push(if complete {
        parse_query(&tokens[begin..])
    } else {
        MediaQuery::Invalid
    });
    queries
}

fn parse_query(tokens: &[Token]) -> MediaQuery {
    if tokens.is_empty() {
        return MediaQuery::Invalid;
    }
    let mut first = 0;
    let mut negated = false;
    let mut only = false;
    if let Token::Ident(ident) = &tokens[0] {
        match ident.as_str() {
            "not" => {
                negated = true;
                first += 1;
            }
            "only" => {
                only = true;
                first += 1;
            }
            _ => {}
        }
    }
    if let Some(Token::Ident(name)) = tokens.get(first)
        && valid_media_type(name)
    {
        let condition = if first + 1 == tokens.len() {
            None
        } else if is_ident(tokens.get(first + 1), "and") {
            parse_condition(&tokens[first + 2..], false, 0)
        } else {
            return MediaQuery::Invalid;
        };
        if first + 1 < tokens.len() && condition.is_none() {
            return MediaQuery::Invalid;
        }
        return MediaQuery::Type {
            name: name.clone(),
            condition,
            negated,
        };
    }
    if only {
        return MediaQuery::Invalid;
    }
    parse_condition(tokens, true, 0)
        .map(MediaQuery::Condition)
        .unwrap_or(MediaQuery::Invalid)
}

fn valid_media_type(name: &str) -> bool {
    !matches!(name, "not" | "only" | "and" | "or" | "layer")
        && name
            .chars()
            .next()
            .is_some_and(|first| first.is_alphabetic() || first == '_' || first == '-')
}

fn is_ident(token: Option<&Token>, wanted: &str) -> bool {
    matches!(token, Some(Token::Ident(ident)) if ident == wanted)
}

fn parse_condition(tokens: &[Token], allow_or: bool, depth: usize) -> Option<Condition> {
    if depth >= MAX_CONDITION_DEPTH {
        return None;
    }
    if is_ident(tokens.first(), "not") {
        if tokens.len() != 2 {
            return None;
        }
        return Some(Condition::Not(Box::new(parse_term(&tokens[1], depth + 1)?)));
    }
    let mut terms = vec![parse_term(tokens.first()?, depth + 1)?];
    let mut connector = None;
    let mut cursor = 1;
    while cursor < tokens.len() {
        let current = match tokens.get(cursor)? {
            Token::Ident(ident) if ident == "and" => "and",
            Token::Ident(ident) if ident == "or" && allow_or => "or",
            _ => return None,
        };
        if connector.is_some_and(|previous| previous != current) {
            return None;
        }
        connector = Some(current);
        terms.push(parse_term(tokens.get(cursor + 1)?, depth + 1)?);
        cursor += 2;
    }
    match connector {
        Some("and") => Some(Condition::And(terms)),
        Some("or") => Some(Condition::Or(terms)),
        _ => terms.pop(),
    }
}

fn parse_term(token: &Token, depth: usize) -> Option<Condition> {
    if depth >= MAX_CONDITION_DEPTH {
        return None;
    }
    match token {
        Token::Group(body) => {
            let nested = tokenize(body)
                .ok()
                .and_then(|tokens| parse_condition(&tokens, true, depth + 1));
            Some(match nested {
                Some(condition) => Condition::Group(Box::new(condition)),
                None => Condition::Feature(body.clone()),
            })
        }
        Token::Function(name, body) => Some(Condition::GeneralEnclosed(format!("{name}({body})"))),
        _ => None,
    }
}

impl MediaQuery {
    pub(super) fn matches(&self, environment: MediaEnvironment) -> bool {
        self.evaluate(environment) == Truth::True
    }

    fn evaluate(&self, environment: MediaEnvironment) -> Truth {
        match self {
            Self::Invalid => Truth::False,
            Self::Type {
                name,
                condition,
                negated,
            } => {
                let medium = if matches!(name.as_str(), "all" | "screen") {
                    Truth::True
                } else {
                    Truth::False
                };
                let result = condition.as_ref().map_or(medium, |condition| {
                    medium.and(condition.evaluate(environment))
                });
                if *negated { result.not() } else { result }
            }
            Self::Condition(condition) => condition.evaluate(environment),
        }
    }

    pub(super) fn serialize(&self) -> String {
        // Preserve well-formed but unknown features: MediaQueryList reevaluates
        // them as the environment changes. Only grammar-invalid components
        // serialize to `not all`.
        match self {
            Self::Invalid => "not all".into(),
            Self::Condition(condition) => condition.serialize(),
            Self::Type {
                name,
                condition,
                negated,
            } => {
                let body = if let Some(condition) = condition {
                    if name == "all" && !negated {
                        condition.serialize()
                    } else {
                        format!("{name} and {}", condition.serialize())
                    }
                } else {
                    name.clone()
                };
                if *negated {
                    format!("not {body}")
                } else {
                    body
                }
            }
        }
    }
}

impl Condition {
    fn evaluate(&self, environment: MediaEnvironment) -> Truth {
        match self {
            Self::Feature(source) => feature::evaluate(source, environment),
            Self::GeneralEnclosed(_) => Truth::Unknown,
            Self::Group(child) => child.evaluate(environment),
            Self::Not(child) => child.evaluate(environment).not(),
            Self::And(children) => children.iter().fold(Truth::True, |result, child| {
                result.and(child.evaluate(environment))
            }),
            Self::Or(children) => children.iter().fold(Truth::False, |result, child| {
                result.or(child.evaluate(environment))
            }),
        }
    }

    fn serialize(&self) -> String {
        match self {
            Self::Feature(source) => format!("({})", feature::canonical(source)),
            Self::GeneralEnclosed(source) => source.clone(),
            Self::Group(child) => format!("({})", child.serialize()),
            Self::Not(child) => format!("not {}", child.serialize()),
            Self::And(children) => children
                .iter()
                .map(Self::serialize)
                .collect::<Vec<_>>()
                .join(" and "),
            Self::Or(children) => children
                .iter()
                .map(Self::serialize)
                .collect::<Vec<_>>()
                .join(" or "),
        }
    }
}
