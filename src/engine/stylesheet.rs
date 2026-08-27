//! Shared stylesheet data passed between the script bindings and native cascade.

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AdoptedStyleSheet {
    pub base_url: String,
    pub media: String,
    pub source: String,
}
