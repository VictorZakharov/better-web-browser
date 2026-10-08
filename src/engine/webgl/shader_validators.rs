//! Reuse the pinned validator's built-ins, not author results or native shaders.
use super::{ApiVersion, gl, shader_validation_cache::Environment};
use mozangle::shaders::{self, BuiltInResources, Output, ShaderValidator};

// Two stage slots, periodically retired even if an environment is unchanged.
// ANGLE frees its per-compilation tree through TScopedPoolAllocator, but its
// output/reflection vectors may retain allocator capacity. Do not retain a
// validator indefinitely after unusually large or failed author input.
const MAX_USES: usize = 16;
const RETAIN_SOURCE_BYTES: usize = 128 * 1024;

struct Entry {
    environment: Environment,
    validator: ShaderValidator,
    uses: usize,
}

#[derive(Default)]
pub(super) struct Validators {
    stages: [Option<Entry>; 2],
    #[cfg(test)]
    pub(super) constructions: usize,
}

fn stage(kind: u32) -> Result<usize, String> {
    match kind {
        gl::VERTEX_SHADER => Ok(0),
        gl::FRAGMENT_SHADER => Ok(1),
        _ => Err("Invalid WebGL validator stage".into()),
    }
}

impl Validators {
    pub fn clear(&mut self) {
        self.stages = [None, None];
    }

    pub fn needs_resources(&self, environment: Environment) -> Result<bool, String> {
        Ok(self.stages[stage(environment.kind)?]
            .as_ref()
            .is_none_or(|entry| entry.environment != environment))
    }

    pub fn prepare(
        &mut self,
        environment: Environment,
        resources: &BuiltInResources,
    ) -> Result<(), String> {
        let index = stage(environment.kind)?;
        // Drop the previous stage before constructing its replacement, so no
        // third handle is retained during an extension-environment transition.
        self.stages[index] = None;
        // Native contexts also own initialization. Never finalize the shared
        // compiler while their asynchronous jobs or peer validators are alive.
        shaders::initialize().map_err(str::to_owned)?;
        let validator = if environment.api == ApiVersion::Two {
            ShaderValidator::for_webgl2(environment.kind, Output::Essl, resources)
        } else {
            ShaderValidator::for_webgl(environment.kind, Output::Essl, resources)
        }
        .ok_or_else(|| "Could not construct the WebGL shader validator".to_owned())?;
        self.stages[index] = Some(Entry {
            environment,
            validator,
            uses: 0,
        });
        #[cfg(test)]
        {
            self.constructions += 1;
        }
        Ok(())
    }

    pub fn compile(&mut self, environment: Environment, source: &str) -> Result<String, String> {
        let index = stage(environment.kind)?;
        let entry = self.stages[index]
            .as_mut()
            .filter(|entry| entry.environment == environment)
            .ok_or_else(|| "WebGL validator environment was not prepared".to_owned())?;
        // Each real compile clears preceding diagnostics, symbols, extension
        // directives, and reflection in pinned TCompiler::compileTreeImpl.
        // Its public Compile contract supports repeated use of one handle.
        let mut options = shaders::CompileOptions::mozangle();
        options.set_emulateGLDrawID(u64::from(environment.multi_draw));
        let result = match entry.validator.compile(&[source], options) {
            Ok(()) => Ok(entry.validator.object_code()),
            Err(reason) => {
                let mut log = entry.validator.info_log();
                if log.is_empty() {
                    log = reason.into();
                }
                super::shader_validation::truncate_log(&mut log);
                Err(log)
            }
        };
        entry.uses += 1;
        let large_output = result
            .as_ref()
            .is_ok_and(|code| code.len() > super::MAX_SHADER_SOURCE_BYTES * 8);
        if result.is_err()
            || large_output
            || source.len() > RETAIN_SOURCE_BYTES
            || entry.uses >= MAX_USES
        {
            // Copy all author-visible output first; native compiler state does
            // not escape this thread-confined owner and is safe to destroy now.
            self.stages[index] = None;
        }
        result
    }
}

#[cfg(test)]
mod tests;
