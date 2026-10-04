//! Use ANGLE's explicit WebGL validator before its native GLES compiler.
//! ESSL output encodes public names and lifts extension directives legally;
//! author source remains separate from the translated driver source.
use super::{MAX_SHADER_BYTES, WebGl, gl};
use mozangle::shaders::{self, BuiltInResources, Output, ShaderValidator};

impl WebGl {
    pub(super) fn translate_shader(&self, kind: u32, source: &str) -> Result<String, String> {
        // ANGLE's driver also owns compiler initialization. Initialize is idempotent;
        // do not finalize here, while other current contexts may still need it.
        shaders::initialize().map_err(str::to_owned)?;
        let mut range = [0; 2];
        let mut precision = 0;
        unsafe {
            gl::GetShaderPrecisionFormat(
                gl::FRAGMENT_SHADER,
                gl::HIGH_FLOAT,
                range.as_mut_ptr(),
                &mut precision,
            );
        }
        let mut resources = BuiltInResources {
            MaxVertexAttribs: limit(gl::MAX_VERTEX_ATTRIBS),
            MaxVertexUniformVectors: limit(gl::MAX_VERTEX_UNIFORM_VECTORS),
            MaxVaryingVectors: limit(gl::MAX_VARYING_VECTORS),
            MaxVertexTextureImageUnits: limit(gl::MAX_VERTEX_TEXTURE_IMAGE_UNITS),
            MaxCombinedTextureImageUnits: limit(gl::MAX_COMBINED_TEXTURE_IMAGE_UNITS),
            MaxTextureImageUnits: limit(gl::MAX_TEXTURE_IMAGE_UNITS),
            MaxFragmentUniformVectors: limit(gl::MAX_FRAGMENT_UNIFORM_VECTORS),
            OES_standard_derivatives: i32::from(self.extensions.derivatives),
            EXT_frag_depth: i32::from(self.extensions.frag_depth),
            EXT_shader_texture_lod: i32::from(self.extensions.texture_lod),
            EXT_draw_buffers: i32::from(self.extensions.draw_buffers),
            MaxDrawBuffers: if self.extensions.draw_buffers {
                self.extensions.max_draw_buffers as i32
            } else {
                1
            },
            FragmentPrecisionHigh: i32::from(precision > 0),
            HashFunction: None,
            ..BuiltInResources::default()
        };
        let version_two = self.options.api == super::ApiVersion::Two;
        if version_two {
            // ESSL300 limits are native scalar capabilities, not WebGL1's extension defaults.
            resources.MaxVertexOutputVectors = limit(0x9122) / 4;
            resources.MaxFragmentInputVectors = limit(0x9125) / 4;
            resources.MinProgramTexelOffset = limit(0x8904);
            resources.MaxProgramTexelOffset = limit(0x8905);
            resources.MaxFragmentUniformBlocks = limit(0x8a2d);
            resources.MaxVertexUniformBlocks = limit(0x8a2b);
            resources.MaxDrawBuffers = limit(0x8824);
        }
        let validator = if version_two {
            ShaderValidator::for_webgl2(kind, Output::Essl, &resources)
        } else {
            ShaderValidator::for_webgl(kind, Output::Essl, &resources)
        }
        .ok_or_else(|| "Could not construct the WebGL shader validator".to_owned())?;
        match validator.compile_and_translate(&[source]) {
            Ok(translated) if translated.len() <= MAX_SHADER_BYTES * 8 => {
                if version_two {
                    // Both validators enforce WebGL2 restrictions. Feed the native
                    // WebGL2 compiler the validated original names: unlike WebGL1,
                    // 1024-byte identifiers reach ANGLE's no-prefix boundary and
                    // cannot be safely decoded by stripping a textual prefix.
                    return Ok(source.to_owned());
                }
                // Both compiler stages now use WebGL1's 256-byte token limit.
                // Undo only the translator's reversible author-name prefix
                // before the native compiler remangles names internally.
                Ok(translated_names(&translated))
            }
            Ok(_) => Err("Translated WebGL shader exceeds the compiler output budget".into()),
            Err(reason) => {
                let mut log = validator.info_log();
                if log.is_empty() {
                    log = reason.into();
                }
                truncate_log(&mut log);
                Err(log)
            }
        }
    }
}

fn limit(pname: u32) -> i32 {
    let mut value = 0;
    // SAFETY: a closed scalar capability query with the owning context current.
    unsafe {
        gl::GetIntegerv(pname, &mut value);
    }
    value
}

pub(super) fn truncate_log(log: &mut String) {
    let mut end = log.len().min(MAX_SHADER_BYTES);
    while !log.is_char_boundary(end) {
        end -= 1;
    }
    log.truncate(end);
}

// The native API receives public names after the validated translator prefix
// is undone. Its own internal compiler mangling is not visible in reflection.
pub(super) fn driver_name(name: &str) -> String {
    name.to_owned()
}

pub(super) fn public_name(name: &str) -> String {
    name.to_owned()
}

fn translated_names(name: &str) -> String {
    // mozangle 0.7.1's pinned ANGLE HashNames.cpp uses a reversible _u
    // prefix when BuiltInResources.HashFunction is null. Apply to identifier
    // components only; leave numeric subscripts and punctuation untouched.
    // WebGL 1 rejects tokens longer than 256, below ANGLE's 1024-byte
    // no-prefix threshold. Every legal user identifier has exactly one prefix.
    map_identifiers(name, |word| {
        word.strip_prefix("_u").unwrap_or(word).to_owned()
    })
}

fn map_identifiers(name: &str, map: impl Fn(&str) -> String) -> String {
    let mut result = String::new();
    let mut start = 0;
    for (index, c) in name.char_indices() {
        if !c.is_ascii_alphanumeric() && c != '_' {
            let word = &name[start..index];
            if word.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
                result.push_str(&map(word));
            } else {
                result.push_str(word);
            }
            result.push(c);
            start = index + c.len_utf8();
        }
    }
    let word = &name[start..];
    if word.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
        result.push_str(&map(word));
    } else {
        result.push_str(word);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shader_name_mapping_preserves_identifier_components_and_subscripts() {
        assert_eq!(driver_name("lights[12]._utint"), "lights[12]._utint");
        assert_eq!(
            translated_names("_ulights[12]._u_utint"),
            "lights[12]._utint"
        );
        assert_eq!(driver_name("gl_Position"), "gl_Position");
        assert_eq!(public_name("gl_Position"), "gl_Position");
        assert_eq!(public_name("_ucolors[2]"), "_ucolors[2]");
        assert_eq!(translated_names("_ucolors[2]"), "colors[2]");
    }

    #[test]
    fn shader_identifier_budget_does_not_strip_long_author_prefix() {
        for length in [2, 255, 256] {
            let name = format!("_u{}", "a".repeat(length - 2));
            let encoded = format!("_u{name}");
            assert_eq!(encoded.len(), length + 2);
            assert_eq!(translated_names(&encoded), name);
            assert_eq!(public_name(&name), name);
        }
    }

    #[test]
    fn compiler_log_truncation_keeps_utf8_valid_and_respects_byte_budget() {
        let mut log = "é".repeat(MAX_SHADER_BYTES);
        truncate_log(&mut log);
        assert_eq!(log.len(), MAX_SHADER_BYTES);
        let mut log = format!("{}😀", "a".repeat(MAX_SHADER_BYTES - 1));
        truncate_log(&mut log);
        assert_eq!(log.len(), MAX_SHADER_BYTES - 1);
        let mut short = "native diagnostic".to_owned();
        truncate_log(&mut short);
        assert_eq!(short, "native diagnostic");
    }
}
