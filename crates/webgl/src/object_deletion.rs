//! Acknowledged deletion keeps realm brands intact when native validation fails.
use super::{ApiVersion, Command, Result, WebGl, gl, json};
use serde_json::Value;

impl WebGl {
    pub(super) fn delete_core_object_checked(&mut self, c: &Command) -> Result<Value> {
        if self.options.api != ApiVersion::Two {
            return Err(gl::INVALID_OPERATION);
        }
        let op = match c.text.as_str() {
            "Sampler" => "deleteSampler",
            "Query" => "deleteQuery",
            "TransformFeedback" => "deleteTransformFeedback",
            "VertexArray" => "deleteVertexArray",
            "Sync" => "deleteSync",
            _ => return Err(gl::INVALID_ENUM),
        };
        self.dispatch(
            &Command {
                op: op.into(),
                i: vec![c.u(0)? as i64],
                f: vec![],
                text: String::new(),
            },
            None,
        )?;
        Ok(json!(true))
    }
}
