//! Bounded, ordered void commands; observations always drain the native owner.
//! WebGL setters return no value. Their GL errors remain observable in command
//! order through getError, rather than needing a thread round trip per setter.
const MAX_COMMANDS: usize = 32;
const MAX_COMMAND_BYTES: usize = 1024;

#[derive(Default)]
pub(super) struct Pending {
    commands: Vec<(u32, String)>,
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
        self.commands.push((id, source.into()));
    }

    pub(super) fn full(&self) -> bool {
        self.commands.len() == MAX_COMMANDS
    }

    pub(super) fn take(&mut self) -> Vec<(u32, String)> {
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
