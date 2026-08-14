//! DOM facade: stable node identity, mutations, document queries, and HTML tree construction.

mod cloning;
mod document;
mod mutation;
mod node;
mod tree_sink;
pub use document::{Dom, parse, parse_with_scripting};
pub use node::{Descendants, ElementData, Node, NodeData, NodeId, NodeRef};

#[cfg(test)]
mod tests;
