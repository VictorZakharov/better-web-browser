//! CSS facade for values, parsing, selector matching, cascade, and media evaluation.

mod cascade;
mod cssom;
mod media;
mod properties;
mod rule_index;
mod selector_match;
mod selector_model;
mod selector_parser;
mod shorthands;
mod stylesheet;
mod supports;
mod syntax;
mod user_agent;
mod value_parser;
mod values;
mod variables;

pub use cascade::StyleSet;
pub use values::{
    AlignItems, BackgroundSize, BoxSizing, Color, ComputedStyle, Display, Edges, FlexDirection,
    Float, JustifyContent, Length, ListStyleType, Position, ResolvedEdges, TextAlign, WhiteSpace,
};

pub(crate) use cssom::resolved_property_value;
pub(crate) use media::media_matches;
pub(crate) use user_agent::is_hidden_by_html_rendering;
pub(crate) use value_parser::parse_length;

use super::dom::{self, Dom, NodeId, NodeRef};
use cssparser::color::{parse_hash_color, parse_named_color};
use cssparser::{Parser, ParserInput, ToCss, Token};
use properties::apply_declaration;
use selector_match::selector_matches;
use selector_model::*;
use selector_parser::parse_selector;
use shorthands::*;
use std::collections::HashMap;
use std::sync::Arc;
use stylesheet::{Declaration, Rule, parse_declarations, parse_stylesheet};
use syntax::*;
use user_agent::apply_user_agent_defaults;
use value_parser::{consume_identifier, parse_color};
use variables::{apply_custom_properties, apply_resolved_declaration};

#[cfg(test)]
use variables::substitute_variables;

#[cfg(test)]
mod tests;
