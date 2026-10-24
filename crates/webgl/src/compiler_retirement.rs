//! Retire ready ANGLE job payloads without waiting, inventing success, or deleting code.
use super::{Kind, Result, WebGl, compiler_events, gl};

impl WebGl {
    pub(super) fn prepare_compiler_event(&mut self, shader: bool, id: u32) -> Result<()> {
        if !self.extensions.available_parallel_compile {
            return Ok(());
        }
        // Recompile/relink resolves the preceding native event in ANGLE. The
        // browser record always describes the most recent invocation instead.
        self.compiler_events.remove(shader, id);
        self.retire_ready_compilers(4)?;
        let limit = if shader {
            compiler_events::SHADERS
        } else {
            compiler_events::PROGRAMS
        };
        if self.compiler_events.len(shader) >= limit {
            let event = self.compiler_events.pop(shader).ok_or(gl::OUT_OF_MEMORY)?;
            // Explicit submissions/barriers may apply bounded backpressure.
            // A completion poll never reaches this blocking path.
            self.resolve_compiler_event(shader, event)?;
        }
        Ok(())
    }

    pub(super) fn record_compiler_event(
        &mut self,
        shader: bool,
        id: u32,
        native: u32,
    ) -> Result<()> {
        if !self.extensions.available_parallel_compile {
            return Ok(());
        }
        self.compiler_events
            .record(shader, compiler_events::Event { id, native })
            .map_err(|_| gl::OUT_OF_MEMORY)
    }

    pub(super) fn retire_ready_compilers(&mut self, maximum: usize) -> Result<()> {
        if !self.extensions.available_parallel_compile {
            return Ok(());
        }
        for shader in [true, false] {
            let count = maximum.min(self.compiler_events.len(shader));
            for _ in 0..count {
                let event = self.compiler_events.pop(shader).ok_or(gl::OUT_OF_MEMORY)?;
                if !self.compiler_event_live(shader, event) {
                    continue;
                }
                let mut complete = 0;
                unsafe {
                    if shader {
                        gl::GetShaderiv(event.native, 0x91b1, &mut complete);
                    } else {
                        gl::GetProgramiv(event.native, 0x91b1, &mut complete);
                    }
                }
                self.driver_result()?;
                if complete != 0 {
                    // Completion queries do not release ANGLE's completed task
                    // payloads themselves. Resolve only after native readiness;
                    // false success is still a completed compilation/link.
                    self.resolve_compiler_event(shader, event)?;
                } else {
                    self.compiler_events
                        .record(shader, event)
                        .map_err(|_| gl::OUT_OF_MEMORY)?;
                }
            }
        }
        Ok(())
    }

    fn compiler_event_live(&self, shader: bool, event: compiler_events::Event) -> bool {
        self.objects
            .get(event.id, if shader { Kind::Shader } else { Kind::Program })
            .is_ok_and(|object| object.native == event.native && !object.native_deleted)
    }

    fn resolve_compiler_event(
        &mut self,
        shader: bool,
        event: compiler_events::Event,
    ) -> Result<()> {
        // Browser IDs never alias recycled native names. Deleted unattached
        // objects disappear from Objects and must never be queried here.
        if !self.compiler_event_live(shader, event) {
            return Ok(());
        }
        let mut success = 0;
        unsafe {
            if shader {
                gl::GetShaderiv(event.native, gl::COMPILE_STATUS, &mut success);
            } else {
                gl::GetProgramiv(event.native, gl::LINK_STATUS, &mut success);
            }
        }
        self.driver_result()
    }
}
