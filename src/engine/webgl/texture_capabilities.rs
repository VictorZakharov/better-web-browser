//! Native texture capabilities, separate from the extensions the author enabled.
//! Merely finding an ANGLE capability must never relax WebGL's public validation.
use std::ffi::CStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TextureCapability {
    Float,
    HalfFloat,
    FloatLinear,
    HalfFloatLinear,
    ColorFloat,
    ColorHalfFloat,
    Depth,
    Srgb,
    Anisotropy,
    FloatBlend,
    Norm16,
}

impl TextureCapability {
    pub(super) const ALL: [Self; 11] = [
        Self::Float,
        Self::HalfFloat,
        Self::FloatLinear,
        Self::HalfFloatLinear,
        Self::ColorFloat,
        Self::ColorHalfFloat,
        Self::Depth,
        Self::Srgb,
        Self::Anisotropy,
        Self::FloatBlend,
        Self::Norm16,
    ];

    pub(super) fn native_name(self) -> &'static CStr {
        match self {
            Self::Float => c"GL_OES_texture_float",
            Self::HalfFloat => c"GL_OES_texture_half_float",
            Self::FloatLinear => c"GL_OES_texture_float_linear",
            Self::HalfFloatLinear => c"GL_OES_texture_half_float_linear",
            // ANGLE's GLES2 RGBA32F extension implements the narrower WebGL1
            // contract without exposing RGB32F storage.
            Self::ColorFloat => c"GL_CHROMIUM_color_buffer_float_rgba",
            Self::ColorHalfFloat => c"GL_EXT_color_buffer_half_float",
            Self::Depth => c"GL_ANGLE_depth_texture",
            Self::Srgb => c"GL_EXT_sRGB",
            Self::Anisotropy => c"GL_EXT_texture_filter_anisotropic",
            Self::FloatBlend => c"GL_EXT_float_blend",
            Self::Norm16 => c"GL_EXT_texture_norm16",
        }
    }

    pub(super) fn public_name(self) -> &'static str {
        match self {
            Self::Float => "OES_texture_float",
            Self::HalfFloat => "OES_texture_half_float",
            Self::FloatLinear => "OES_texture_float_linear",
            Self::HalfFloatLinear => "OES_texture_half_float_linear",
            Self::ColorFloat => "WEBGL_color_buffer_float",
            Self::ColorHalfFloat => "EXT_color_buffer_half_float",
            Self::Depth => "WEBGL_depth_texture",
            Self::Srgb => "EXT_sRGB",
            Self::Anisotropy => "EXT_texture_filter_anisotropic",
            Self::FloatBlend => "EXT_float_blend",
            Self::Norm16 => "EXT_texture_norm16",
        }
    }

    pub(super) fn exposed_in(self, api: super::ApiVersion) -> bool {
        if self == Self::Norm16 {
            return api == super::ApiVersion::Two;
        }
        api == super::ApiVersion::One
            || !matches!(
                self,
                Self::Float
                    | Self::HalfFloat
                    | Self::HalfFloatLinear
                    | Self::ColorFloat
                    | Self::Depth
                    | Self::Srgb
            )
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|value| *value == self).unwrap()
    }
}

#[derive(Default)]
pub(super) struct TextureCapabilities {
    available: [bool; 11],
    enabled: [bool; 11],
}

impl super::extensions::Extensions {
    pub(super) fn enable_texture(&mut self, capability: TextureCapability) -> bool {
        use TextureCapability::{ColorFloat, ColorHalfFloat, Float, HalfFloat};
        let base = match capability {
            ColorFloat => Float,
            ColorHalfFloat => HalfFloat,
            _ => capability,
        };
        if !self.request_texture(base) {
            return false;
        }
        // OES float/half-float must implicitly enable color renderability when
        // available. Color requests in turn require the base texture type.
        // A direct request avoids a recursive dependency cycle.
        let color = match base {
            Float => Some(ColorFloat),
            HalfFloat => Some(ColorHalfFloat),
            _ => None,
        };
        if let Some(color) = color
            && self.textures.available(color)
            && !self.request_texture(color)
        {
            return false;
        }
        if base == Float && !self.enable_implicit_float_blend() {
            return false;
        }
        self.textures.enabled(capability)
    }

    pub(super) fn enable_implicit_float_blend(&mut self) -> bool {
        // EXT_float_blend's implicit-enable rule preserves pre-extension HDR
        // content. Unsupported blending does not disable float renderability.
        // https://registry.khronos.org/webgl/extensions/EXT_float_blend/
        !self.textures.available(TextureCapability::FloatBlend)
            || self.request_texture(TextureCapability::FloatBlend)
    }

    fn request_texture(&mut self, capability: TextureCapability) -> bool {
        if self.textures.enabled(capability) {
            return true;
        }
        let enabled = self.enable_simple(
            capability.native_name(),
            self.textures.available(capability),
        );
        self.textures.confirm(capability, enabled);
        self.textures.enabled(capability)
    }
}

impl TextureCapabilities {
    pub(super) fn discover<'a>(names: impl Iterator<Item = &'a str>) -> Self {
        let mut result = Self::default();
        for name in names {
            for capability in TextureCapability::ALL {
                if name.as_bytes() == capability.native_name().to_bytes() {
                    result.available[capability.index()] = true;
                }
            }
        }
        result
    }

    pub(super) fn available(&self, capability: TextureCapability) -> bool {
        self.available[capability.index()]
    }

    pub(super) fn enabled(&self, capability: TextureCapability) -> bool {
        self.enabled[capability.index()]
    }

    pub(super) fn confirm(&mut self, capability: TextureCapability, enabled: bool) {
        // The request API may reject a capability advertised as requestable.
        // Never set a public admission bit until the driver's enabled list agrees.
        self.enabled[capability.index()] = enabled && self.available(capability);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_matches_complete_fixed_native_names_only() {
        let capabilities = TextureCapabilities::discover(
            [
                "GL_OES_texture_float",
                "GL_OES_texture_half_float_linear",
                "GL_OES_texture_half_float_suffix",
                "GL_CHROMIUM_color_buffer_float_rgba",
                "GL_EXT_sRGB",
            ]
            .into_iter(),
        );
        assert!(capabilities.available(TextureCapability::Float));
        assert!(capabilities.available(TextureCapability::HalfFloatLinear));
        assert!(capabilities.available(TextureCapability::Srgb));
        assert!(!capabilities.available(TextureCapability::HalfFloat));
        assert!(capabilities.available(TextureCapability::ColorFloat));
        for capability in TextureCapability::ALL {
            assert!(!capabilities.enabled(capability));
        }
    }

    #[test]
    fn native_confirmation_cannot_create_an_unadvertised_capability() {
        let mut capabilities = TextureCapabilities::discover(["GL_OES_texture_float"].into_iter());
        capabilities.confirm(TextureCapability::Float, true);
        capabilities.confirm(TextureCapability::Depth, true);
        assert!(capabilities.enabled(TextureCapability::Float));
        assert!(!capabilities.enabled(TextureCapability::Depth));
        capabilities.confirm(TextureCapability::Float, false);
        assert!(!capabilities.enabled(TextureCapability::Float));
    }

    #[test]
    fn fresh_capabilities_do_not_inherit_an_old_contexts_admission() {
        let names = ["GL_OES_texture_half_float"];
        let mut old = TextureCapabilities::discover(names.into_iter());
        old.confirm(TextureCapability::HalfFloat, true);
        let restored = TextureCapabilities::discover(names.into_iter());
        assert!(old.enabled(TextureCapability::HalfFloat));
        assert!(restored.available(TextureCapability::HalfFloat));
        assert!(!restored.enabled(TextureCapability::HalfFloat));
    }
}
