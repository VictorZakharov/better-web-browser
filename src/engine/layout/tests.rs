#![cfg(test)]
//! Layout regression tests grouped by area (mirrors `crate::engine::css::tests`).
use super::*;

mod anonymous_blocks;
mod controls;
mod css_math;
mod edge_cases;
mod file_input;
mod flex_sizing;
mod general;
mod math_functions;
mod pseudo;
mod shadow_manual;
mod text_direction;
mod truncation;
mod truncation_dynamic;
