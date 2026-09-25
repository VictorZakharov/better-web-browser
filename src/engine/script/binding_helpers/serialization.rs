//! DOM Parsing and Serialization: HTML fragment and XML node serialization.

mod html;
mod xml;

pub(in crate::engine::script) use html::{serialize_children, serialize_html_node};
pub(in crate::engine::script) use xml::serialize_xml_node;
