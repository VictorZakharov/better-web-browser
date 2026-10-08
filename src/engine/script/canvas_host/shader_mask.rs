//! Shared owned Canvas shader output, coverage and source-over composition.
use resvg::tiny_skia::{Paint, Pixmap, Rect, Shader, Transform};
use std::borrow::Cow;
use std::sync::Arc;

pub(super) enum Samples<'a> {
    Full,
    Bytes(&'a [u8]),
    Compact(Arc<super::coverage_storage::Coverage>),
}

impl<'a> Samples<'a> {
    pub(super) fn from_argument(value: Option<&'a super::JsValue>) -> Option<Self> {
        match value? {
            super::JsValue::Null => Some(Self::Full),
            super::JsValue::Bytes(bytes) => Some(Self::Bytes(bytes)),
            _ => None,
        }
    }

    fn matches(&self, pixels: usize) -> bool {
        match self {
            Self::Full => true,
            Self::Bytes(bytes) => bytes.len() == pixels,
            Self::Compact(coverage) => coverage.len() == pixels,
        }
    }

    fn visit(&self, pixels: usize, mut paint: impl FnMut(usize, u8)) {
        match self {
            Self::Full => (0..pixels).for_each(|index| paint(index, 255)),
            Self::Bytes(bytes) => bytes.iter().enumerate().for_each(|(i, &v)| paint(i, v)),
            Self::Compact(coverage) => coverage.runs(|start, bytes| {
                for (offset, &value) in bytes.iter().enumerate() {
                    paint(start + offset, value);
                }
            }),
        }
    }
}

pub(super) fn render(
    width: u32,
    height: u32,
    shader: Shader<'_>,
    destination: Cow<'_, [u8]>,
    mask: Samples<'_>,
    opacity: f64,
    clip: Option<&super::raster_clip::Clip<'_>>,
) -> Option<Vec<u8>> {
    let pixels = (width as usize).checked_mul(height as usize)?;
    if pixels == 0
        || pixels > super::MAX_CANVAS_PIXELS
        || width > 16384
        || height > 16384
        || destination.len() != pixels.checked_mul(4)?
        || !mask.matches(pixels)
        || !opacity.is_finite()
        || !(0.0..=1.0).contains(&opacity)
    {
        return None;
    }
    let mut source = Pixmap::new(width, height)?;
    let paint = Paint {
        shader,
        anti_alias: false,
        force_hq_pipeline: true,
        ..Paint::default()
    };
    source.fill_rect(
        Rect::from_xywh(0.0, 0.0, width as f32, height as f32)?,
        &paint,
        Transform::identity(),
        None,
    );
    let mut output = destination.into_owned();
    mask.visit(pixels, |index, coverage| {
        if coverage == 0 || clip.is_some_and(|clip| !clip.allows(index)) {
            return;
        }
        let pixel = &mut output[index * 4..index * 4 + 4];
        let color = source.pixels()[index].demultiply();
        super::solid_mask::source_over(
            pixel,
            [color.red(), color.green(), color.blue(), color.alpha()].map(f64::from),
            opacity * (f64::from(coverage) / 255.0),
        );
    });
    Some(output)
}
