//! Closed extension drawing-state and batch command routing.
use super::{Command, Result, WebGl};
use serde_json::Value;

impl WebGl {
    pub(super) fn dispatch_draw_extension(&mut self, command: &Command) -> Option<Result<Value>> {
        match command.op.as_str() {
            "enableiOES"
            | "disableiOES"
            | "blendEquationiOES"
            | "blendEquationSeparateiOES"
            | "blendFunciOES"
            | "blendFuncSeparateiOES"
            | "colorMaskiOES" => Some(self.indexed_blend_command(command)),
            "multiDrawArraysWEBGL"
            | "multiDrawElementsWEBGL"
            | "multiDrawArraysInstancedWEBGL"
            | "multiDrawElementsInstancedWEBGL" => Some(self.multi_draw_command(command)),
            _ => None,
        }
    }
}
