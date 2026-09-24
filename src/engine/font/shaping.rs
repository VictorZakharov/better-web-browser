//! Shared system/web-font selection and OpenType shaping for layout and Canvas.

mod catalog;
mod geometry;
mod shape;

pub(crate) use catalog::{FontCatalog, FontInstanceKey, SelectedFont};
pub(crate) use shape::TextShaper;
