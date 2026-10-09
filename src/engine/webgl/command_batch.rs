//! Bounded, ordered void commands; observations always drain the native owner.
//! WebGL setters return no value. Their GL errors remain observable in command
//! order through getError, rather than needing a thread round trip per setter.
// Setters retain bounded ownership while amortizing native-thread handoffs.
// Observations and task boundaries drain earlier commands regardless of size.
const MAX_COMMANDS: usize = 128;
const MAX_COMMAND_BYTES: usize = 1024;
pub(crate) const MAX_NUMERIC_VALUES: usize = 64;

/// Converted numeric setters only: no names, source strings, bytes or replies.
/// The ordinary native dispatcher still validates every operation's GL shape.
pub(crate) struct NumericCommand(super::Command);

impl NumericCommand {
    pub(crate) fn new(op: String, i: Vec<i64>, f: Vec<f64>) -> Option<Self> {
        if !Pending::operation(&op)
            || i.len().checked_add(f.len())? > MAX_NUMERIC_VALUES
            || i.iter()
                .any(|value| value.unsigned_abs() > 9_007_199_254_740_991)
        {
            return None;
        }
        Some(Self(super::Command {
            op,
            i,
            f,
            text: String::new(),
        }))
    }

    pub(super) fn into_command(self) -> super::Command {
        self.0
    }
}

pub(super) enum Entry {
    Json(String),
    Numeric(NumericCommand),
}

#[derive(Default)]
pub(super) struct Pending {
    commands: Vec<(u32, Entry)>,
}

impl Pending {
    pub(super) fn candidate(source: &str) -> bool {
        if source.len() > MAX_COMMAND_BYTES {
            return false;
        }
        // Only the canonical bridge encoding takes this optimization. Other
        // encodings still execute normally. The complete command is parsed and
        // validated by the native owner, including duplicate/unknown fields.
        let Some(tail) = source.strip_prefix("{\"op\":\"") else {
            return false;
        };
        let Some((name, suffix)) = tail.split_once('"') else {
            return false;
        };
        if !suffix.starts_with([',', '}']) {
            return false;
        }
        Self::operation(name)
    }

    pub(super) fn operation(name: &str) -> bool {
        matches!(
            name,
            "bindBuffer"
                | "bufferData"
                | "bindBufferBase"
                | "bindBufferRange"
                | "bindTexture"
                | "activeTexture"
                | "texParameteri"
                | "texParameterf"
                | "bindFramebuffer"
                | "bindRenderbuffer"
                | "framebufferTexture2D"
                | "framebufferTextureLayer"
                | "framebufferRenderbuffer"
                | "bindVertexArray"
                | "bindVertexArrayOES"
                | "vertexAttribPointer"
                | "vertexAttribIPointer"
                | "vertexAttribDivisor"
                | "vertexAttribDivisorANGLE"
                | "enableVertexAttribArray"
                | "disableVertexAttribArray"
                | "enable"
                | "disable"
                | "viewport"
                | "scissor"
                | "clearColor"
                | "clearDepth"
                | "clearStencil"
                | "clear"
                | "colorMask"
                | "depthMask"
                | "depthFunc"
                | "depthRange"
                | "blendColor"
                | "blendFunc"
                | "blendFuncSeparate"
                | "blendEquation"
                | "blendEquationSeparate"
                | "stencilMask"
                | "stencilMaskSeparate"
                | "stencilFunc"
                | "stencilFuncSeparate"
                | "stencilOp"
                | "stencilOpSeparate"
                | "cullFace"
                | "frontFace"
                | "polygonOffset"
                | "sampleCoverage"
                | "lineWidth"
                | "pixelStorei"
                | "useProgram"
                | "uniform1f"
                | "uniform2f"
                | "uniform3f"
                | "uniform4f"
                | "uniform1i"
                | "uniform2i"
                | "uniform3i"
                | "uniform4i"
                | "uniform1fv"
                | "uniform2fv"
                | "uniform3fv"
                | "uniform4fv"
                | "uniform1iv"
                | "uniform2iv"
                | "uniform3iv"
                | "uniform4iv"
                | "uniformMatrix2fv"
                | "uniformMatrix3fv"
                | "uniformMatrix4fv"
                | "uniform1ui"
                | "uniform2ui"
                | "uniform3ui"
                | "uniform4ui"
                | "uniform1uiv"
                | "uniform2uiv"
                | "uniform3uiv"
                | "uniform4uiv"
                | "uniformMatrix2x3fv"
                | "uniformMatrix2x4fv"
                | "uniformMatrix3x2fv"
                | "uniformMatrix3x4fv"
                | "uniformMatrix4x2fv"
                | "uniformMatrix4x3fv"
                | "drawArrays"
                | "drawElements"
                | "drawArraysInstanced"
                | "drawElementsInstanced"
                | "drawArraysInstancedANGLE"
                | "drawElementsInstancedANGLE"
                | "drawRangeElements"
                | "drawBuffers"
                | "drawBuffersWEBGL"
                | "readBuffer"
                | "beginQuery"
                | "endQuery"
                | "beginTransformFeedback"
                | "endTransformFeedback"
                | "pauseTransformFeedback"
                | "resumeTransformFeedback"
                | "bindTransformFeedback"
                | "deleteBuffer"
                | "deleteTexture"
                | "deleteFramebuffer"
                | "deleteRenderbuffer"
                | "deleteShader"
                | "deleteProgram"
                | "deleteVertexArray"
                | "deleteVertexArrayOES"
                | "bridgeError"
        )
    }

    pub(super) fn push(&mut self, id: u32, source: &str) {
        debug_assert!(Self::candidate(source));
        debug_assert!(!self.full());
        self.commands.push((id, Entry::Json(source.into())));
    }

    pub(super) fn push_numeric(&mut self, id: u32, command: NumericCommand) {
        debug_assert!(!self.full());
        self.commands.push((id, Entry::Numeric(command)));
    }

    pub(super) fn full(&self) -> bool {
        self.commands.len() == MAX_COMMANDS
    }

    pub(super) fn take(&mut self) -> Vec<(u32, Entry)> {
        std::mem::take(&mut self.commands)
    }

    pub(super) fn remove(&mut self, id: u32) {
        self.commands.retain(|(context, _)| *context != id);
    }

    pub(super) fn clear(&mut self) {
        self.commands.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_bounded_canonical_void_operations_are_batch_candidates() {
        for op in [
            "bindBuffer",
            "drawArrays",
            "clear",
            "uniform4f",
            "bridgeError",
            "uniformMatrix4fv",
            "uniformMatrix2x3fv",
            "uniform4uiv",
        ] {
            assert!(Pending::candidate(&format!(
                r#"{{"op":"{op}","i":[],"f":[],"text":""}}"#
            )));
        }
        for source in [
            r#"{"op":"getError"}"#,
            r#"{"op":"createBuffer"}"#,
            r#"{"op":"finish"}"#,
            r#"{"op":"flush"}"#,
            r#"{"op":"fenceSync"}"#,
            r#"{"op":"getQueryParameter"}"#,
            r#"{"op":"readPixels"}"#,
            r#"{"op":"getBufferSubData"}"#,
            r#"{"op":"enableExtension"}"#,
            r#"{"op":"compileShader"}"#,
            r#"{"op":"clearSuffix"}"#,
            r#"{"op":"clear"suffix:0}"#,
            r#"{"i":[],"op":"clear"}"#,
            r#"{"op": "clear"}"#,
            r#"{"op":"getError","text":"clear"}"#,
        ] {
            assert!(!Pending::candidate(source), "{source}");
        }
        let too_large = format!(
            r#"{{"op":"clear","text":"{}"}}"#,
            "x".repeat(MAX_COMMAND_BYTES)
        );
        assert!(!Pending::candidate(&too_large));
    }

    #[test]
    fn queue_bounds_order_and_context_retirement_are_independent() {
        let mut queue = Pending::default();
        for index in 0..MAX_COMMANDS {
            queue.push(
                (index % 2) as u32 + 1,
                &format!(r#"{{"op":"clear","i":[{index}]}}"#),
            );
            assert_eq!(queue.full(), index + 1 == MAX_COMMANDS);
        }
        queue.remove(1);
        let taken = queue.take();
        assert_eq!(taken.len(), MAX_COMMANDS / 2);
        for (index, (id, command)) in taken.iter().enumerate() {
            assert_eq!(*id, 2);
            let Entry::Json(command) = command else {
                panic!("JSON entry")
            };
            assert_eq!(
                *command,
                format!(r#"{{"op":"clear","i":[{}]}}"#, index * 2 + 1)
            );
        }
        assert!(!queue.full());
        queue.push(3, r#"{"op":"clear"}"#);
        queue.clear();
        assert!(queue.take().is_empty());
    }
}
