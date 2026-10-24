//! Native BC families are negotiated separately from author-visible admission.
use super::extensions::Extensions;
use std::ffi::CStr;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Family {
    S3tc,
    Srgb,
    Rgtc,
}
impl Family {
    pub(super) const ALL: [Self; 3] = [Self::S3tc, Self::Srgb, Self::Rgtc];
    fn index(self) -> usize {
        self as usize
    }
    pub(super) fn public_name(self) -> &'static str {
        match self {
            Self::S3tc => "WEBGL_compressed_texture_s3tc",
            Self::Srgb => "WEBGL_compressed_texture_s3tc_srgb",
            Self::Rgtc => "EXT_texture_compression_rgtc",
        }
    }
    fn native_name(self) -> &'static CStr {
        match self {
            Self::S3tc => c"GL_EXT_texture_compression_s3tc",
            Self::Srgb => c"GL_EXT_texture_compression_s3tc_srgb",
            Self::Rgtc => c"GL_EXT_texture_compression_rgtc",
        }
    }
    pub(super) fn formats(self) -> [u32; 4] {
        match self {
            Self::S3tc => [0x83f0, 0x83f1, 0x83f2, 0x83f3],
            Self::Srgb => [0x8c4c, 0x8c4d, 0x8c4e, 0x8c4f],
            Self::Rgtc => [0x8dbb, 0x8dbc, 0x8dbd, 0x8dbe],
        }
    }
}
pub(super) struct Capabilities {
    available: [bool; 3],
    enabled: [bool; 3],
    split_s3tc: bool,
}
impl Capabilities {
    pub(super) fn discover<'a>(names: impl Iterator<Item = &'a str>) -> Self {
        let names = names.collect::<std::collections::HashSet<_>>();
        let split_s3tc = !names.contains("GL_EXT_texture_compression_s3tc")
            && S3TC_PARTS
                .iter()
                .all(|name| names.contains(name.to_str().unwrap()));
        Self {
            available: Family::ALL.map(|family| {
                names.contains(family.native_name().to_str().unwrap())
                    || family == Family::S3tc && split_s3tc
            }),
            enabled: [false; 3],
            split_s3tc,
        }
    }
    pub(super) fn available(&self, family: Family) -> bool {
        self.available[family.index()]
    }
    pub(super) fn enabled(&self, family: Family) -> bool {
        self.enabled[family.index()]
    }
    pub(super) fn formats(&self) -> Vec<u32> {
        Family::ALL
            .into_iter()
            .filter(|&family| self.enabled(family))
            .flat_map(Family::formats)
            .collect()
    }
}
impl Extensions {
    pub(super) fn enable_compressed(&mut self, family: Family) -> bool {
        if self.compressed.enabled(family) {
            return true;
        }
        // ANGLE exposes linear BC1/2/3 as three native extensions on D3D11.
        // The browser extension is admitted only if all three are real.
        let enabled = if family == Family::S3tc && self.compressed.split_s3tc {
            S3TC_PARTS
                .into_iter()
                .all(|name| self.enable_simple(name, true))
        } else {
            self.enable_simple(family.native_name(), self.compressed.available(family))
        };
        self.compressed.enabled[family.index()] = enabled;
        enabled
    }
}
const S3TC_PARTS: [&CStr; 3] = [
    c"GL_EXT_texture_compression_dxt1",
    c"GL_ANGLE_texture_compression_dxt3",
    c"GL_ANGLE_texture_compression_dxt5",
];
