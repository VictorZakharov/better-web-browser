//! Native shader translation may proceed before link submission on the GL owner.
use super::{Command, Kind, Result, WebGl, gl, pending_links};

const COMPLETION_STATUS: u32 = 0x91b1;

impl WebGl {
    pub(super) fn submit_program_link(&mut self, id: u32, native: u32) -> Result<()> {
        // Linking the current executable has immediate GLES state semantics;
        // ANGLE resolves it synchronously, so never defer that case.
        if !self.extensions.available_parallel_compile || self.program == id {
            unsafe { gl::LinkProgram(native) };
            return self.driver_result();
        }
        if self.pending_links.len() == pending_links::LIMIT {
            // Bounded backpressure, rather than a growing per-page job list.
            self.flush_program_links(1)?;
        }
        let shaders = self.objects.link_shaders(id)?;
        let link = pending_links::Link {
            id,
            native,
            shaders,
        };
        // Program and attached shader references already belong to Objects.
        // Mutation/deletion barriers below retain their link-time state.
        self.pending_links.push(link).map_err(|_| gl::OUT_OF_MEMORY)
    }

    pub(super) fn flush_program_links(&mut self, count: usize) -> Result<()> {
        let mut first_error = None;
        for _ in 0..count {
            let Some(link) = self.pending_links.pop() else {
                break;
            };
            self.prepare_compiler_event(false, link.id)?;
            // ANGLE may block here for translations not yet ready, as ordinary
            // status/reflection/use operations are permitted to do by KHR.
            unsafe { gl::LinkProgram(link.native) };
            if let Err(error) = self.driver_result() {
                first_error.get_or_insert(error);
            } else if let Err(error) = self.record_compiler_event(false, link.id, link.native) {
                first_error.get_or_insert(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    /// Only ready translation jobs are submitted here; no wait, fake completed
    /// shader, sleep, or callback into an author realm is involved.
    pub(super) fn progress_program_links(&mut self, maximum: usize) -> Result<()> {
        self.retire_ready_compilers(4)?;
        for _ in 0..maximum {
            // Completion polls and idle progress must not wait on a full native
            // mailbox. Ordinary compile submission and explicit barriers retain
            // their separate bounded backpressure contract.
            if !super::compiler_workers::accepts_ready_link() {
                break;
            }
            if self.compiler_events.len(false) >= super::compiler_events::PROGRAMS {
                break;
            }
            let Some(link) = self.pending_links.front() else {
                break;
            };
            let mut ready = true;
            for shader in &link.shaders {
                let mut completed = 0;
                unsafe { gl::GetShaderiv(shader.native, COMPLETION_STATUS, &mut completed) };
                ready &= completed != 0;
            }
            self.driver_result()?;
            if !ready {
                break;
            }
            self.flush_program_links(1)?;
        }
        Ok(())
    }

    pub(super) fn compiler_barrier(&mut self, command: &Command) -> Result<()> {
        if self.pending_links.is_empty() {
            return Ok(());
        }
        let count = match command.op.as_str() {
            "shaderSource" | "compileShader" => self.pending_links.through_shader(command.u(0)?),
            // The completion query alone must not wait. All other program
            // observations and mutations resolve the requested invocation first.
            "getProgramParameter" if command.u(1)? == COMPLETION_STATUS => 0,
            "attachShader"
            | "detachShader"
            | "linkProgram"
            | "validateProgram"
            | "useProgram"
            | "deleteProgram"
            | "getProgramParameter"
            | "getProgramInfoLog"
            | "getAttribLocation"
            | "bindAttribLocation"
            | "getUniformLocation"
            | "getUniform"
            | "getActiveAttrib"
            | "getActiveUniform"
            | "getAttachedShaders"
            | "transformFeedbackVaryings"
            | "getTransformFeedbackVarying"
            | "getUniformIndices"
            | "getFragDataLocation"
            | "getActiveUniforms"
            | "getUniformBlockIndex"
            | "getActiveUniformBlockParameter"
            | "getActiveUniformBlockName"
            | "uniformBlockBinding" => self.pending_links.through_program(command.u(0)?),
            // Native extension/capture state and explicit driver barriers cannot
            // overtake a previously requested link invocation.
            "enableExtension"
            | "getError"
            | "flush"
            | "finish"
            | "beginTransformFeedback"
            | "endTransformFeedback"
            | "pauseTransformFeedback"
            | "resumeTransformFeedback" => self.pending_links.len(),
            _ => 0,
        };
        self.flush_program_links(count)
    }
}

impl super::objects::Objects {
    pub(super) fn link_shaders(&self, id: u32) -> Result<Vec<pending_links::Shader>> {
        self.attached_shaders(id)?
            .iter()
            .map(|&id| {
                self.get(id, Kind::Shader)
                    .map(|shader| pending_links::Shader {
                        id,
                        native: shader.native,
                    })
            })
            .collect()
    }
}

impl super::BackendContexts {
    pub(super) fn progress_compilers(&mut self) {
        for context in self.contexts.values_mut() {
            if context.pending_links.is_empty()
                && context.compiler_events.len(true) == 0
                && context.compiler_events.len(false) == 0
            {
                continue;
            }
            if context.native.make_current().is_err() {
                context.objects.poisoned = true;
                continue;
            }
            if let Err(error) = context.progress_program_links(1) {
                context.error(error);
            }
        }
    }
}
