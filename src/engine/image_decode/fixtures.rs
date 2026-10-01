//! Owned 3×2 fixtures; generation/provenance lives in tests/image-fixtures/README.md.

use base64::Engine;

pub(crate) struct Fixture {
    pub name: &'static str,
    pub encoded: &'static str,
    pub reference: &'static str,
    pub tolerance: u8,
}

macro_rules! fixture {
    ($name:literal, $tolerance:literal) => {
        Fixture {
            name: $name,
            encoded: include_str!(concat!("../../../tests/image-fixtures/", $name, ".base64")),
            reference: include_str!(concat!(
                "../../../tests/image-fixtures/",
                $name,
                ".rgba.base64"
            )),
            tolerance: $tolerance,
        }
    };
}

pub(crate) const MODERN: &[Fixture] = &[
    fixture!("rgb-lossless.avif", 0),
    fixture!("yuv420-full.avif", 3),
    fixture!("yuv422-limited.avif", 3),
    fixture!("yuv444-10.avif", 3),
    fixture!("yuv420-12.avif", 3),
    fixture!("gray.avif", 1),
    fixture!("gray10.avif", 1),
    fixture!("rgb-lossless.jxl", 0),
    fixture!("rgba-lossless.jxl", 0),
    fixture!("gray-lossless.jxl", 1),
    fixture!("graya-lossless.jxl", 1),
    fixture!("rgb16-lossless.jxl", 1),
    fixture!("rgba16-lossless.jxl", 1),
    fixture!("rgb-lossy.jxl", 3),
    fixture!("rgba-lossless.avif", 0),
];

pub(crate) fn bytes(text: &str) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(text.split_whitespace().collect::<String>())
        .unwrap()
}

pub(crate) fn find(name: &str) -> Vec<u8> {
    bytes(
        MODERN
            .iter()
            .find(|fixture| fixture.name == name)
            .unwrap()
            .encoded,
    )
}
