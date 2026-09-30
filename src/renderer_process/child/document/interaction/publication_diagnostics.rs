//! Opt-in phase evidence for full native-input publications, never a publication trigger.

use crate::limits::MAX_RUNTIME_REPORT_ENTRIES;
use crate::renderer_protocol::DocumentInput;
use std::time::Duration;

pub(super) fn input_kind(input: &DocumentInput) -> &'static str {
    match input {
        DocumentInput::Wheel(_) => "wheel",
        DocumentInput::Pointer(_) => "pointer",
        DocumentInput::Keyboard(_) => "keyboard",
        DocumentInput::Text(_) => "text",
        DocumentInput::NativeText(_) => "native-text",
        DocumentInput::Selection(_) => "selection",
        DocumentInput::Focus(_) => "focus",
        DocumentInput::Scroll(_) => "scroll-feedback",
        DocumentInput::Lifecycle(_) => "lifecycle",
        DocumentInput::History(_) => "history",
    }
}

pub(super) struct Publication {
    pub input_kind: &'static str,
    pub sequence: u64,
    pub mutations: usize,
    pub render_requested: bool,
    pub forced_accessibility: bool,
    pub rebuilt_layout: bool,
    pub sticky_layers: usize,
    pub displaced_sticky_layers: usize,
    pub layout_items: usize,
    pub style_time: Duration,
    pub layout_time: Duration,
}

pub(super) fn append(diagnostics: &mut Vec<String>, diagnostic: Option<Publication>) {
    if diagnostics.len() >= MAX_RUNTIME_REPORT_ENTRIES {
        return;
    }
    let Some(diagnostic) = diagnostic else { return };
    diagnostics.push(format!(
        "input publication: kind={}, sequence={}, mutations={}, render_requested={}, forced_accessibility={}, rebuilt_layout={}, sticky_layers={} (displaced={}), layout_items={}, style_gate_ms={:.3}, layout_ms={:.3}",
        diagnostic.input_kind, diagnostic.sequence, diagnostic.mutations, diagnostic.render_requested,
        diagnostic.forced_accessibility, diagnostic.rebuilt_layout, diagnostic.sticky_layers,
        diagnostic.displaced_sticky_layers, diagnostic.layout_items,
        diagnostic.style_time.as_secs_f64() * 1000.0,
        diagnostic.layout_time.as_secs_f64() * 1000.0,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagnostic() -> Publication {
        Publication {
            input_kind: "scroll-feedback",
            sequence: 7,
            mutations: 0,
            render_requested: false,
            forced_accessibility: true,
            rebuilt_layout: false,
            sticky_layers: 1,
            displaced_sticky_layers: 0,
            layout_items: 2735,
            style_time: Duration::ZERO,
            layout_time: Duration::ZERO,
        }
    }

    #[test]
    fn disabled_diagnostics_do_not_change_quiet_runtime_effects() {
        let mut diagnostics = vec!["kept".into()];
        append(&mut diagnostics, None);
        assert_eq!(diagnostics, vec!["kept"]);
    }

    #[test]
    fn zero_offset_sticky_publication_is_distinct_from_layout_rebuild() {
        let mut diagnostics = Vec::new();
        append(&mut diagnostics, Some(diagnostic()));
        let line = &diagnostics[0];
        assert!(line.contains("kind=scroll-feedback, sequence=7"));
        assert!(
            line.contains(
                "render_requested=false, forced_accessibility=true, rebuilt_layout=false"
            )
        );
        assert!(line.contains("sticky_layers=1 (displaced=0)"));
        assert!(line.contains("style_gate_ms=0.000, layout_ms=0.000"));
    }

    #[test]
    fn full_diagnostic_budget_preserves_existing_entries() {
        let mut diagnostics = vec!["kept".into(); MAX_RUNTIME_REPORT_ENTRIES];
        append(&mut diagnostics, Some(diagnostic()));
        assert_eq!(diagnostics, vec!["kept"; MAX_RUNTIME_REPORT_ENTRIES]);
    }
}
