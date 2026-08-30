//! CSS facade for values, parsing, selector matching, cascade, and media evaluation.

mod cascade;
mod change;
mod cssom;
mod fullscreen;
pub(crate) mod media;
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

pub use cascade::{StyleRefreshStats, StyleSet};
pub use values::{
    AlignItems, BackgroundSize, BoxSizing, Color, ComputedStyle, Display, Edges, FlexDirection,
    Float, JustifyContent, Length, ListStyleType, Position, ResolvedEdges, TextAlign, WhiteSpace,
};

use super::dom::{self, Dom, Node, NodeData, NodeId, NodeRef};
pub(crate) use cssom::resolved_property_value;
use cssparser::color::{parse_hash_color, parse_named_color};
use cssparser::{Parser, ParserInput, ToCss, Token};
use properties::{apply_declaration, parse_text_spacing};
use selector_match::selector_matches;
use selector_model::*;
use selector_parser::parse_selector;
use shorthands::*;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use stylesheet::{Declaration, Rule, RuleScope, parse_declarations, parse_stylesheet};
use syntax::*;
use user_agent::apply_user_agent_defaults;
pub(crate) use user_agent::is_hidden_by_html_rendering;
use value_parser::consume_identifier;
pub(crate) use value_parser::{parse_color, parse_length};
use variables::{apply_custom_properties, apply_resolved_declaration};

#[cfg(test)]
use variables::substitute_variables;

#[cfg(test)]
mod tests;
