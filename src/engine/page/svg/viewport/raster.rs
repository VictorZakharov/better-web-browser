use super::*;
use crate::engine::image_decode::DecodeLimits;
use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{Size, Tree};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Request {
    width: f32,
    height: f32,
    pixels: (u32, u32),
}

impl Request {
    pub fn new(width: f32, height: f32, density: f32) -> Result<Self, String> {
        if ![width, height, density]
            .into_iter()
            .all(|value| value.is_finite() && value > 0.)
        {
            return Err("inline SVG viewport or density is not positive and finite".into());
        }
        let pixel_width = (f64::from(width) * f64::from(density)).ceil();
        let pixel_height = (f64::from(height) * f64::from(density)).ceil();
        let limit = f64::from(DecodeLimits::PAGE.dimension);
        if pixel_width > limit || pixel_height > limit {
            return Err("inline SVG viewport exceeds the device-raster dimension budget".into());
        }
        let pixels = (pixel_width as u32, pixel_height as u32);
        DecodeLimits::PAGE.rgba_len(pixels.0, pixels.1)?;
        Ok(Self {
            width,
            height,
            pixels,
        })
    }
}

pub(super) fn tree(
    source: &str,
    fonts: &[crate::engine::font::WebFont],
    viewport: Option<Size>,
) -> Result<Tree, String> {
    let document = super::super::admission::document(source.as_bytes())?;
    let embedded = super::super::embedded_fonts::collect(&document);
    let fonts = fonts.iter().chain(&embedded).cloned().collect::<Vec<_>>();
    let mut options = super::super::decoder::options(DecodeLimits::PAGE);
    super::super::fonts::configure(&mut options, &fonts);
    match viewport {
        Some(size) => Tree::from_xmltree_with_viewport(&document, &options, size),
        None => Tree::from_xmltree(&document, &options),
    }
    .map_err(|error| format!("parse inline SVG: {error}"))
}

pub(super) fn render(
    source: &str,
    fonts: &[crate::engine::font::WebFont],
    request: Request,
) -> Result<DecodedImage, String> {
    let size = Size::from_wh(request.width, request.height).ok_or("invalid inline SVG viewport")?;
    let tree = tree(source, fonts, Some(size))?;
    let (width, height) = request.pixels;
    let mut pixmap = Pixmap::new(width, height).ok_or("allocate inline SVG viewport pixels")?;
    // The vector coordinate system remains CSS-sized. Pixel rounding is local
    // to its raster and is not reflected in DOM geometry or intrinsic sizing.
    resvg::render(
        &tree,
        Transform::from_scale(width as f32 / request.width, height as f32 / request.height),
        &mut pixmap.as_mut(),
    );
    let mut bgra = pixmap.take();
    for pixel in bgra.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Ok(DecodedImage {
        width,
        height,
        bgra: bgra.into(),
    })
}
