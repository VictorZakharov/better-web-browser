//! Hidden input uses the same focused-control value events as native editing.
use super::*;
use better_web_browser::renderer_protocol::{FocusInput, TextInput};

impl BrowserState {
    pub(super) fn set_benchmark_control_value(
        &mut self,
        selector: &str,
        value: String,
    ) -> Result<(), String> {
        let target = self
            .page_diagnostics
            .selectors
            .iter()
            .find(|diagnostics| diagnostics.selector == selector)
            .and_then(|diagnostics| diagnostics.matches.first())
            .and_then(|node| DocumentNodeId::new(node.node_id).ok())
            .ok_or_else(|| format!("benchmark control selector did not match: {selector}"))?;
        let (document, sequence) = self
            .next_renderer_input()
            .ok_or_else(|| "benchmark text has no active renderer document".to_string())?;
        if !self.submit_renderer_input(DocumentInput::Focus(FocusInput {
            document,
            sequence,
            focused: true,
            target: Some(target),
        })) {
            return Err("benchmark control focus was rejected by the renderer".into());
        }
        let end = value.encode_utf16().count() as u32;
        let (document, sequence) = self
            .next_renderer_input()
            .ok_or_else(|| "benchmark text lost its renderer document".to_string())?;
        if self.submit_renderer_input(DocumentInput::Text(TextInput {
            document,
            sequence,
            target,
            value,
            selection_start: end,
            selection_end: end,
        })) {
            Ok(())
        } else {
            Err("benchmark text was rejected by the renderer".into())
        }
    }
}
