//! The shader and native API versions are one contract, never independently upgraded.
//! WebGL2's GLES3 backend is internal until the complete canvas interface is admitted.
//! https://registry.khronos.org/webgl/specs/latest/2.0/#1
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize)]
pub(super) enum ApiVersion {
    #[default]
    #[serde(rename = "webgl1")]
    One,
    #[serde(rename = "webgl2")]
    Two,
}

impl ApiVersion {
    pub(super) fn query_name_budget(self) -> usize {
        match self {
            Self::One => 256,
            // A uniform path can contain multiple 1024-byte identifiers and
            // array subscripts. Bound bridge memory, not a complete path to
            // the shader's individual-token limit.
            Self::Two => super::MAX_SHADER_BYTES,
        }
    }
    pub(super) fn client_version(self) -> i32 {
        match self {
            Self::One => 2,
            Self::Two => 3,
        }
    }

    pub(super) fn renderable_bit(self) -> i32 {
        match self {
            Self::One => 0x0004, // EGL_OPENGL_ES2_BIT
            Self::Two => 0x0040, // EGL_OPENGL_ES3_BIT
        }
    }

    pub(super) fn version_string(self) -> &'static str {
        match self {
            Self::One => "WebGL 1.0 (ANGLE)",
            Self::Two => "WebGL 2.0 (ANGLE)",
        }
    }

    pub(super) fn shader_version_string(self) -> &'static str {
        match self {
            Self::One => "WebGL GLSL ES 1.0 (ANGLE)",
            Self::Two => "WebGL GLSL ES 3.00 (ANGLE)",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_version_is_a_closed_contract_not_an_arbitrary_egl_integer() {
        assert_eq!(
            serde_json::from_str::<ApiVersion>("\"webgl1\"").unwrap(),
            ApiVersion::One
        );
        assert_eq!(
            serde_json::from_str::<ApiVersion>("\"webgl2\"").unwrap(),
            ApiVersion::Two
        );
        for value in ["3", "0", "null", "true", "\"gles31\"", "\"WEBGL2\""] {
            assert!(
                serde_json::from_str::<ApiVersion>(value).is_err(),
                "{value}"
            );
        }
        assert_eq!(ApiVersion::One.client_version(), 2);
        assert_eq!(ApiVersion::Two.client_version(), 3);
        assert_eq!(ApiVersion::One.renderable_bit(), 4);
        assert_eq!(ApiVersion::Two.renderable_bit(), 64);
    }
}
