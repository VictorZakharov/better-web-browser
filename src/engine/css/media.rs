//! Media Queries Level 4 syntax and evaluation shared by CSS and CSSOM View.
//!
//! A parsed query is either valid, invalid (not all), or evaluates to
//! unknown. Keeping those states separate prevents negation of unsupported
//! future media features from accidentally matching.
//! https://drafts.csswg.org/mediaqueries-4/#mq-syntax

mod feature;
mod lexer;
mod parser;
#[cfg(test)]
mod tests;

use self::parser::{MediaQuery, parse_list};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Truth {
    True,
    False,
    Unknown,
}

impl Truth {
    fn not(self) -> Self {
        match self {
            Self::True => Self::False,
            Self::False => Self::True,
            Self::Unknown => Self::Unknown,
        }
    }

    fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::False, _) | (_, Self::False) => Self::False,
            (Self::True, Self::True) => Self::True,
            _ => Self::Unknown,
        }
    }

    fn or(self, other: Self) -> Self {
        match (self, other) {
            (Self::True, _) | (_, Self::True) => Self::True,
            (Self::False, Self::False) => Self::False,
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MediaEnvironment {
    pub(crate) viewport_width: f32,
    pub(crate) viewport_height: f32,
    pub(crate) resolution_dppx: f32,
    pub(crate) prefers_dark_color_scheme: bool,
}

impl MediaEnvironment {
    pub(crate) fn new(width: f32, height: f32, dppx: f32, dark: bool) -> Self {
        Self {
            viewport_width: width.max(1.0),
            viewport_height: height.max(1.0),
            resolution_dppx: dppx.max(0.01),
            prefers_dark_color_scheme: dark,
        }
    }

    pub(crate) fn with_viewport(self, width: f32, height: f32) -> Self {
        Self::new(
            width,
            height,
            self.resolution_dppx,
            self.prefers_dark_color_scheme,
        )
    }
}

pub(crate) fn media_matches_for_environment(prelude: &str, environment: MediaEnvironment) -> bool {
    let input = prelude.trim();
    let queries = super::at_rule_prelude(input, "media").unwrap_or(input);
    let list = parse_list(queries);
    list.is_empty() || list.iter().any(|query| query.matches(environment))
}

#[cfg(test)]
pub(crate) fn media_query_matches(query: &str, environment: MediaEnvironment) -> bool {
    let list = parse_list(query);
    list.is_empty() || list.iter().any(|item| item.matches(environment))
}

/// CSSOM View serializes malformed components to "not all", recovering
/// independently at the next top-level comma.
pub(crate) fn serialize_media_query_list(input: &str) -> String {
    parse_list(input)
        .iter()
        .map(MediaQuery::serialize)
        .collect::<Vec<_>>()
        .join(", ")
}
