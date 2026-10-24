//! Use ANGLE's explicit WebGL validator before its native GLES compiler.
//! ESSL output encodes public names and lifts extension directives legally;
//! author source remains separate from the translated driver source.
use super::{MAX_SHADER_BYTES, MAX_SHADER_SOURCE_BYTES, WebGl};

impl WebGl {
    pub(super) fn translate_shader(&mut self, kind: u32, source: &str) -> Result<String, String> {
        let environment = super::shader_validation_cache::Environment {
            api: self.options.api,
            kind,
            derivatives: self.extensions.derivatives,
            frag_depth: self.extensions.frag_depth,
            texture_lod: self.extensions.texture_lod,
            draw_buffers: self.extensions.draw_buffers,
            multi_draw: self.extensions.multi_draw,
            max_draw_buffers: self.extensions.max_draw_buffers,
        };
        // Native capabilities are immutable for this context. Extension
        // admission changes the explicit environment; restore constructs a new
        // owner/cache. Reuse only successful validation, not transient failures.
        if let Some(translated) = self.shader_validation_cache.lookup(environment, source) {
            return Ok(translated);
        }
        if self.shader_validators.needs_resources(environment)? {
            let resources = super::shader_validator_resources::current(self);
            self.shader_validators.prepare(environment, &resources)?;
        }
        let translated = self.shader_validators.compile(environment, source)?;
        let translated = self.admit_shader_translation(source, translated)?;
        self.shader_validation_cache
            .insert(environment, source, &translated);
        Ok(translated)
    }

    fn admit_shader_translation(&self, source: &str, translated: String) -> Result<String, String> {
        let version_two = self.options.api == super::ApiVersion::Two;
        if translated.len() <= MAX_SHADER_SOURCE_BYTES * 8 {
            if version_two || self.extensions.multi_draw {
                // Preserve native draw-ID ownership in either version. In
                // WebGL2, original names also avoid ANGLE's 1024-byte
                // no-prefix boundary, which cannot be safely decoded by
                // stripping a textual prefix. Native validation stays active.
                return Ok(source.to_owned());
            }
            // Both compiler stages now use WebGL1's 256-byte token limit.
            // Undo only the translator's reversible author-name prefix
            // before the native compiler remangles names internally.
            Ok(translated_names(&translated))
        } else {
            Err("Translated WebGL shader exceeds the compiler output budget".into())
        }
    }
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
        assert_eq!(
            translated_names("_ulights[12]._u_utint"),
            "lights[12]._utint"
        );
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
