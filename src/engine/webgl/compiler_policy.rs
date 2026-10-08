//! Compiler scheduling is browser policy, distinct from author extension admission.
use super::{extensions::Extensions, gl};

impl Extensions {
    pub(super) fn initialize_compiler(&mut self) -> Result<(), String> {
        if !self.available_parallel_compile {
            return Ok(());
        }
        // KHR_parallel_shader_compile permits an implementation to compile/link
        // in parallel even when the page has not enabled its completion-query API.
        // The delegate's shared four-worker pool supplies the actual concurrency
        // bound, not ANGLE's per-context hardware_concurrency default.
        // https://registry.khronos.org/webgl/extensions/KHR_parallel_shader_compile/
        if !self.enable_simple(c"GL_KHR_parallel_shader_compile", true)
            || unsafe { gl::GetError() } != gl::NO_ERROR
        {
            return Err("Enable bounded native ANGLE shader compilation failed".into());
        }
        // Do not set parallel_compile here: public COMPLETION_STATUS_KHR still
        // requires getExtension, and no native thread-control API is exposed.
        Ok(())
    }
}
