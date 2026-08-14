// These responsibility-based chunks share one IIFE and are concatenated before Boa parses them.
pub(super) const BROWSER_BOOTSTRAP: &str = concat!(
    include_str!("bootstrap/core.js"),
    include_str!("bootstrap/events.js"),
    include_str!("bootstrap/nodes.js"),
    include_str!("bootstrap/elements.js"),
    include_str!("bootstrap/forms.js"),
    include_str!("bootstrap/document.js"),
    include_str!("bootstrap/platform.js"),
    include_str!("bootstrap/tasks.js"),
);
