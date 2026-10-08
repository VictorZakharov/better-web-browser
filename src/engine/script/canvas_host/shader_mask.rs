//! Shared owned Canvas shader output, coverage and source-over composition.
use resvg::tiny_skia::{Paint, Pixmap, Rect, Shader, Transform};

pub(super) fn render(
    width: u32,
    height: u32,
    shader: Shader<'_>,
    destination: &[u8],
    mask: Option<&[u8]>,
    opacity: f64,
    clip: Option<&super::raster_clip::Clip<'_>>,
) -> Option<Vec<u8>> {
    let pixels = (width as usize).checked_mul(height as usize)?;
    if pixels == 0
        || pixels > super::MAX_CANVAS_PIXELS
        || width > 16384
        || height > 16384
        || destination.len() != pixels.checked_mul(4)?
        || mask.is_some_and(|mask| mask.len() != pixels)
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
    let mut output = destination.to_vec();
    for (index, (pixel, source)) in output.chunks_exact_mut(4).zip(source.pixels()).enumerate() {
        let coverage = mask.map_or(255, |mask| mask[index]);
        if coverage == 0 || clip.is_some_and(|clip| !clip.allows(index)) {
            continue;
        }
        let color = source.demultiply();
        super::solid_mask::source_over(
            pixel,
            [color.red(), color.green(), color.blue(), color.alpha()].map(f64::from),
            opacity * (f64::from(coverage) / 255.0),
        );
    }
    Some(output)
}
