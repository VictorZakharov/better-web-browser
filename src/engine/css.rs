mod cascade;
mod change;
pub(crate) mod clip_path;
mod content;
mod css_wide;
mod cssom;
pub(crate) mod font_family;
mod fullscreen;
pub(crate) mod imports;
pub(crate) mod media;
mod properties;
mod rule_index;
mod scroll_spacing;
pub(crate) mod selector_match;
mod selector_model;
mod selector_parser;
mod selector_validity;
mod shorthands;
mod stylesheet;
pub(crate) mod supports;
mod syntax;
pub(crate) mod transform;
mod user_agent;
mod value_parser;
mod values;
mod variables;
use super::dom::{self, Dom, Node, NodeData, NodeId, NodeRef};
pub use cascade::{StyleRefreshStats, StyleSet, StylesheetSource};
pub use content::GeneratedContent;
pub(crate) use cssom::{diagnostic_custom_properties, resolved_property_value};
use cssparser::{Parser, ParserInput, ToCss, Token};
use properties::{apply_declaration, parse_text_spacing};
pub(crate) use selector_match::compile_selector_list;
use selector_model::*;
pub(super) use selector_parser::PseudoElement;
use selector_parser::{parse_selector, parse_style_rule_selector};
use shorthands::*;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use stylesheet::{Declaration, Rule, RuleScope, parse_declarations};
use syntax::*;
pub(crate) use user_agent::is_hidden_by_html_rendering;
pub(crate) use value_parser::{consume_identifier, parse_color, parse_length, parse_opacity};
pub use values::{
    AlignItems, AspectRatio, BackgroundSize, BoxSizing, Clear, Color, ComputedStyle,
    ContentAlignment, Display, Edges, FlexDirection, Float, JustifyContent, Length, ListStyleType,
    ObjectFit, ObjectPosition, Overflow, Position, ResolvedEdges, TextAlign, TextTransform,
    VerticalAlign, WhiteSpace,
};
use variables::{apply_custom_properties, apply_resolved_declaration};
mod tests;
