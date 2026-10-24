//! Try full native/context/surface admission before accepting a backend.
//! Hardware is preferred; WARP is a real renderer, never ANGLE's NULL backend.
use super::adapter_selection::AdapterId;
use super::creation_options::Options;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Backend {
    Hardware(Option<AdapterId>),
    Software,
}

impl Backend {
    pub(super) fn attributes(self) -> Vec<i32> {
        // EGL_ANGLE_platform_angle_d3d and its independently advertised LUID
        // extension. Adapter IDs are trusted OS results, not author strings.
        // https://github.com/google/angle/blob/main/extensions/EGL_ANGLE_platform_angle_d3d_luid.txt
        let mut attributes = vec![
            0x3203,
            0x3208,
            0x3209,
            match self {
                Self::Hardware(_) => 0x320a,
                Self::Software => 0x320b,
            },
            // EGL_PLATFORM_ANGLE_DEBUG_LAYERS_ENABLED_ANGLE: do not let
            // installed developer SDK layers change the production working set
            // or latency. This disables optional backend debugging, NOT GLES/
            // WebGL validation, robustness, shader rules, or initialization.
            // https://github.com/google/angle/blob/main/extensions/EGL_ANGLE_platform_angle.txt
            0x3451,
            mozangle::egl::ffi::FALSE as i32,
        ];
        if let Self::Hardware(Some(id)) = self {
            attributes.extend([0x34a0, id.high, 0x34a1, id.low as i32]);
        }
        attributes.push(mozangle::egl::ffi::NONE as i32);
        attributes
    }
}

pub(super) fn admit<T>(
    options: Options,
    software_only: bool,
    preferred: Option<AdapterId>,
    mut create: impl FnMut(Backend) -> Result<T, String>,
) -> Result<T, String> {
    let mut errors = Vec::new();
    if !software_only {
        if let Some(adapter) = preferred {
            match create(Backend::Hardware(Some(adapter))) {
                Ok(context) => return Ok(context),
                Err(error) => errors.push(format!("preferred hardware: {error}")),
            }
        }
        match create(Backend::Hardware(None)) {
            Ok(context) => return Ok(context),
            Err(error) => errors.push(format!("default hardware: {error}")),
        }
    }
    if options.fail_if_major_performance_caveat {
        errors.push("software rendering rejected by failIfMajorPerformanceCaveat".into());
        return Err(errors.join("; "));
    }
    match create(Backend::Software) {
        Ok(context) => Ok(context),
        Err(error) => {
            errors.push(format!("software fallback: {error}"));
            Err(errors.join("; "))
        }
    }
}

#[cfg(test)]
mod tests;
