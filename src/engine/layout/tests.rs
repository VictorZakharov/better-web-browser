#![cfg(test)]
//! Layout regression tests grouped by area (mirrors `crate::engine::css::tests`).
use super::*;

mod anonymous_blocks;
mod background_layers;
mod controls;
mod css_math;
mod edge_cases;
mod file_input;
mod flex_columns;
mod flex_sizing;
mod flex_text;
mod general;
mod implicit_grid;
mod masks;
mod math_functions;
mod normal_line_height;
mod pseudo;
mod scrollbar_styles;
mod shadow_manual;
mod text_direction;
mod truncation;
mod truncation_dynamic;
mod zoom;
