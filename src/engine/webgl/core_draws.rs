//! Range hints never weaken the ordinary indexed-draw safety boundary.
use super::{ApiVersion, Command, Result, WebGl, gl};
use serde_json::Value;

impl WebGl {
    pub(super) fn draw_range_elements(&mut self, c: &Command) -> Result<Value> {
        if self.options.api != ApiVersion::Two {
            return Err(gl::INVALID_OPERATION);
        }
        let start = c.u(1)?;
        let end = c.u(2)?;
        if end < start {
            return Err(gl::INVALID_VALUE);
        }
        // WebGL2 §3.7.9 explicitly permits ignoring the range hints. Indices
        // outside [start,end] are not errors. Use the same owned index mirror,
        // primitive-restart handling and bounds checks as every indexed draw.
        let draw = Command {
            op: "drawElements".into(),
            i: vec![
                c.u(0)? as i64,
                c.n(3)? as i64,
                c.u(4)? as i64,
                c.i.get(5).copied().ok_or(gl::INVALID_VALUE)?,
            ],
            f: vec![],
            text: String::new(),
        };
        self.buffer_command(&draw, None)
    }
}
