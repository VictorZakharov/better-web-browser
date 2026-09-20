//! A document-scoped reference, not renderer-provided URL/origin authority.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RequestClient {
    /// Zero selects the top-level document. Other IDs require a completed navigation.
    pub id: u64,
    /// Can only narrow the referenced origin to an opaque origin.
    pub opaque: bool,
}
