//! CSS Animations Level 1 computed settings. Parsing never mutates a winner on failure.
//! Names are case-sensitive CSS identifiers/strings, unlike timing keywords.
//! https://drafts.csswg.org/css-animations-1/#animation-shorthand

use super::super::{Parser, ParserInput, Token, split_css_top_level};
use super::transitions::{easing, time};

mod shorthand;
#[cfg(test)]
mod tests;

pub(crate) const PROPERTIES: [&str; 8] = [
    "animation-name",
    "animation-duration",
    "animation-delay",
    "animation-timing-function",
    "animation-iteration-count",
    "animation-direction",
    "animation-fill-mode",
    "animation-play-state",
];

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum AnimationName {
    None,
    Named(String),
    Quoted(String),
}

impl AnimationName {
    pub(crate) fn named(&self) -> Option<&str> {
        match self {
            Self::None => None,
            Self::Named(name) | Self::Quoted(name) => Some(name),
        }
    }
    pub(crate) fn parse(value: &str) -> Option<Self> {
        let mut input = ParserInput::new(value);
        let mut parser = Parser::new(&mut input);
        let token = parser.next().ok()?.clone();
        if !parser.is_exhausted() {
            return None;
        }
        match token {
            Token::Ident(name) if name.eq_ignore_ascii_case("none") => Some(Self::None),
            Token::Ident(name) if !reserved(&name) => Some(Self::Named(name.to_string())),
            Token::QuotedString(name) if !name.is_empty() => Some(Self::Quoted(name.to_string())),
            _ => None,
        }
    }

    pub(crate) fn css_text(&self) -> String {
        match self {
            Self::None => "none".into(),
            Self::Named(name) => {
                let mut text = String::new();
                cssparser::serialize_identifier(name, &mut text)
                    .expect("String writing cannot fail");
                text
            }
            Self::Quoted(name) => {
                if !name.is_empty() && !reserved(name) && !name.eq_ignore_ascii_case("none") {
                    let mut text = String::new();
                    cssparser::serialize_identifier(name, &mut text)
                        .expect("String writing cannot fail");
                    return text;
                }
                // Only reserved keywords retain quotes in CSSOM serialization.
                let mut text = String::new();
                cssparser::serialize_string(name, &mut text).expect("String writing cannot fail");
                text
            }
        }
    }
}

fn reserved(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "initial" | "inherit" | "unset" | "revert" | "revert-layer" | "default"
    )
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AnimationSettings {
    /// Tree scope of the winning name declaration, including :host and ::slotted rules.
    pub(crate) name_scope: Option<crate::engine::dom::NodeId>,
    pub(crate) names: Vec<AnimationName>,
    pub(crate) durations: Vec<f32>,
    pub(crate) delays: Vec<f32>,
    pub(crate) easings: Vec<String>,
    pub(crate) iterations: Vec<f64>,
    pub(crate) directions: Vec<String>,
    pub(crate) fills: Vec<String>,
    pub(crate) states: Vec<String>,
}

impl Default for AnimationSettings {
    fn default() -> Self {
        Self {
            name_scope: None,
            names: vec![AnimationName::None],
            durations: vec![0.0],
            delays: vec![0.0],
            easings: vec!["ease".into()],
            iterations: vec![1.0],
            directions: vec!["normal".into()],
            fills: vec!["none".into()],
            states: vec!["running".into()],
        }
    }
}

pub(crate) fn supported_property(name: &str) -> bool {
    name == "animation" || PROPERTIES.contains(&name)
}

pub(crate) fn apply(settings: &mut AnimationSettings, name: &str, value: &str) -> bool {
    // Construct a complete candidate before replacing any component.
    let mut next = settings.clone();
    let valid = match name {
        "animation" => shorthand::parse(value).map(|value| next = value),
        "animation-name" => list(value, AnimationName::parse).map(|v| next.names = v),
        "animation-duration" => list(value, |v| time(v, false)).map(|v| next.durations = v),
        "animation-delay" => list(value, |v| time(v, true)).map(|v| next.delays = v),
        "animation-timing-function" => list(value, |v| {
            let lower = v.to_ascii_lowercase();
            easing(&lower).map(str::to_owned)
        })
        .map(|v| next.easings = v),
        "animation-iteration-count" => list(value, iteration).map(|v| next.iterations = v),
        "animation-direction" => keywords(
            value,
            &["normal", "reverse", "alternate", "alternate-reverse"],
        )
        .map(|v| next.directions = v),
        "animation-fill-mode" => {
            keywords(value, &["none", "forwards", "backwards", "both"]).map(|v| next.fills = v)
        }
        "animation-play-state" => keywords(value, &["running", "paused"]).map(|v| next.states = v),
        _ => None,
    };
    if valid.is_none() {
        return false;
    }
    *settings = next;
    true
}

fn list<T>(value: &str, parse: impl Fn(&str) -> Option<T>) -> Option<Vec<T>> {
    let mut values = Vec::new();
    for part in split_css_top_level(value, ',') {
        if values.len() == 64 {
            return None;
        }
        values.push(parse(part.trim())?);
    }
    (!values.is_empty()).then_some(values)
}

fn keywords(value: &str, allowed: &[&str]) -> Option<Vec<String>> {
    list(value, |part| {
        let lower = part.to_ascii_lowercase();
        allowed.contains(&lower.as_str()).then_some(lower)
    })
}

pub(super) fn iteration(value: &str) -> Option<f64> {
    if value.eq_ignore_ascii_case("infinite") {
        return Some(f64::INFINITY);
    }
    let mut input = ParserInput::new(value);
    let mut parser = Parser::new(&mut input);
    let Token::Number { value, .. } = parser.next().ok()?.clone() else {
        return None;
    };
    (parser.is_exhausted() && value.is_finite() && value >= 0.0).then_some(f64::from(value))
}

impl AnimationSettings {
    pub(crate) fn copy_property(&mut self, source: &Self, property: &str) -> bool {
        match property {
            "animation" => self.clone_from(source),
            "animation-name" => {
                self.names.clone_from(&source.names);
                self.name_scope = source.name_scope;
            }
            "animation-duration" => self.durations.clone_from(&source.durations),
            "animation-delay" => self.delays.clone_from(&source.delays),
            "animation-timing-function" => self.easings.clone_from(&source.easings),
            "animation-iteration-count" => self.iterations.clone_from(&source.iterations),
            "animation-direction" => self.directions.clone_from(&source.directions),
            "animation-fill-mode" => self.fills.clone_from(&source.fills),
            "animation-play-state" => self.states.clone_from(&source.states),
            _ => return false,
        }
        true
    }

    pub(crate) fn property_value(&self, property: &str) -> Option<String> {
        Some(match property {
            "animation-name" => self
                .names
                .iter()
                .map(AnimationName::css_text)
                .collect::<Vec<_>>()
                .join(", "),
            "animation-duration" => super::transitions::serialize_times(&self.durations),
            "animation-delay" => super::transitions::serialize_times(&self.delays),
            "animation-timing-function" => self.easings.join(", "),
            "animation-direction" => self.directions.join(", "),
            "animation-fill-mode" => self.fills.join(", "),
            "animation-play-state" => self.states.join(", "),
            "animation-iteration-count" => self
                .iterations
                .iter()
                .map(|v| {
                    if v.is_infinite() {
                        "infinite".into()
                    } else {
                        v.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(", "),
            _ => return None,
        })
    }
}
