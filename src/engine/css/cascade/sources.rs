//! Resource payloads are not cascade order. Linked sheets acquire their position
//! from their current owner node; explicit engine-injected sheets have no owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StylesheetSource {
    pub(crate) base_url: String,
    pub(crate) source: String,
    pub(crate) owner_url: Option<String>,
    pub(crate) imports: Vec<crate::engine::css::imports::Import>,
}

impl StylesheetSource {
    pub(crate) fn injected(base_url: impl Into<String>, source: String) -> Self {
        Self {
            imports: crate::engine::css::imports::parse(&source),
            base_url: base_url.into(),
            source,
            owner_url: None,
        }
    }

    pub(crate) fn linked(url: &str, source: String) -> Self {
        Self {
            imports: crate::engine::css::imports::parse(&source),
            base_url: url.into(),
            source,
            owner_url: Some(url.into()),
        }
    }

    pub(crate) fn url(&self) -> &str {
        self.owner_url.as_deref().unwrap_or(&self.base_url)
    }
}
