//! SVG image decoding has no ambient filesystem authority. All non-data image
//! references must eventually be fulfilled by the browser's resource broker,
//! never by usvg's default local-file resolver.

use crate::engine::image_decode::DecodeLimits;
use crate::limits::MAX_SVG_SOURCE_BYTES;
use image::{ImageEncoder, ImageReader};
use std::borrow::Cow;
use std::io::{Cursor, Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

const MAX_EMBEDDED_IMAGES: usize = 64;
const MAX_NESTED_SVGS: usize = 16;

pub(super) fn payload(source: &[u8]) -> Result<Cow<'_, [u8]>, String> {
    if source.len() > MAX_SVG_SOURCE_BYTES {
        return Err("SVG source exceeds the byte budget".into());
    }
    if !source.starts_with(&[0x1f, 0x8b]) {
        return Ok(Cow::Borrowed(source));
    }
    // usvg's built-in SVGZ inflater has no output budget. Reuse our existing
    // flate2 dependency, but stop before allocating an unbounded expansion.
    let mut inflated = Vec::new();
    flate2::read::GzDecoder::new(source)
        .take((MAX_SVG_SOURCE_BYTES + 1) as u64)
        .read_to_end(&mut inflated)
        .map_err(|error| format!("decode SVGZ: {error}"))?;
    if inflated.len() > MAX_SVG_SOURCE_BYTES {
        return Err("SVGZ expansion exceeds the byte budget".into());
    }
    Ok(Cow::Owned(inflated))
}

struct Budget {
    images: AtomicUsize,
    depth: AtomicUsize,
    bytes: AtomicUsize,
}

struct DepthGuard<'a>(&'a AtomicUsize);
impl Drop for DepthGuard<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

pub(super) fn options(limits: DecodeLimits) -> resvg::usvg::Options<'static> {
    let budget = Arc::new(Budget {
        images: AtomicUsize::new(MAX_EMBEDDED_IMAGES),
        depth: AtomicUsize::new(0),
        bytes: AtomicUsize::new(limits.working_bytes),
    });
    resvg::usvg::Options {
        image_href_resolver: resvg::usvg::ImageHrefResolver {
            resolve_string: Box::new(|_, _| None),
            resolve_data: Box::new(move |mime, data, options| {
                limits.check_source(&data).ok()?;
                budget
                    .images
                    .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |remaining| {
                        remaining.checked_sub(1)
                    })
                    .ok()?;
                let nested = mime == "image/svg+xml"
                    || data.starts_with(&[0x1f, 0x8b])
                    || super::looks_like_svg(&data);
                if nested {
                    budget
                        .depth
                        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |depth| {
                            (depth < MAX_NESTED_SVGS).then_some(depth + 1)
                        })
                        .ok()?;
                    let _depth = DepthGuard(&budget.depth);
                    let source = payload(&data).ok()?;
                    if source.iter().filter(|&&byte| byte == b'<').count()
                        > crate::limits::MAX_DOM_NODES
                    {
                        return None;
                    }
                    let tree = resvg::usvg::Tree::from_data_nested(&source, options).ok()?;
                    let size = tree.size().to_int_size();
                    let bytes = limits.rgba_len(size.width(), size.height()).ok()?;
                    budget
                        .bytes
                        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |remaining| {
                            remaining.checked_sub(bytes)
                        })
                        .ok()?;
                    return Some(resvg::usvg::ImageKind::SVG(tree));
                }
                // Check encoded dimensions before resvg allocates decoded image
                // storage. The aggregate budget also covers sibling data images.
                let (width, height) = ImageReader::new(Cursor::new(data.as_slice()))
                    .with_guessed_format()
                    .ok()?
                    .into_dimensions()
                    .ok()?;
                let bytes = limits.rgba_len(width, height).ok()?;
                budget
                    .bytes
                    .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |remaining| {
                        remaining.checked_sub(bytes)
                    })
                    .ok()?;
                // Normalize through our existing bounded/color-managed decoder.
                // In particular, a GIF's logical dimensions alone cannot bound a
                // hostile frame rectangle passed to a second decoder.
                let raster =
                    crate::engine::image_decode::decode(&data, limits, Default::default()).ok()?;
                let mut png = BoundedPng {
                    bytes: Vec::new(),
                    limit: raster.rgba.len().checked_add(65536)?,
                };
                image::codecs::png::PngEncoder::new(&mut png)
                    .write_image(
                        &raster.rgba,
                        raster.width,
                        raster.height,
                        image::ExtendedColorType::Rgba8,
                    )
                    .ok()?;
                Some(resvg::usvg::ImageKind::PNG(Arc::new(png.bytes)))
            }),
        },
        ..Default::default()
    }
}

struct BoundedPng {
    bytes: Vec<u8>,
    limit: usize,
}

impl Write for BoundedPng {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(std::io::Error::other(
                "SVG normalized image exceeds the byte budget",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use std::io::Write;

    #[test]
    fn svgz_is_inflated_with_an_output_limit_and_invalid_streams_fail() {
        let source = b"<svg xmlns='http://www.w3.org/2000/svg' width='1' height='1'/>";
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gzip.write_all(source).unwrap();
        assert_eq!(payload(&gzip.finish().unwrap()).unwrap().as_ref(), source);
        assert!(payload(&[0x1f, 0x8b, 0, 0]).is_err());
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gzip.write_all(&vec![b' '; MAX_SVG_SOURCE_BYTES + 1])
            .unwrap();
        assert!(payload(&gzip.finish().unwrap()).is_err());
    }

    #[test]
    fn svg_decoder_cannot_open_an_existing_local_image() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "breeze-svg-file-{}-{nonce}.png",
            std::process::id()
        ));
        image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]))
            .save(&path)
            .unwrap();
        let href = path.to_string_lossy().replace('\\', "/");
        let default = resvg::usvg::ImageHrefResolver::default_string_resolver();
        let exists = default(&href, &Default::default()).is_some();
        let svg = format!(
            "<svg xmlns='http://www.w3.org/2000/svg' width='1' height='1'><image href='{href}' width='1' height='1'/></svg>"
        );
        let image = super::super::decode_svg(svg.as_bytes(), "local reference test");
        std::fs::remove_file(path).unwrap();
        assert!(
            exists,
            "control image must be loadable by upstream's default resolver"
        );
        assert_eq!(image.unwrap().bgra.as_ref(), &[0, 0, 0, 0]);
    }

    #[test]
    fn embedded_rasters_share_the_image_budget_but_safe_data_urls_still_render() {
        let mut png = Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(2, 1, image::Rgba([255, 0, 0, 255]))
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let data = Arc::new(png.into_inner());
        let options = options(DecodeLimits {
            working_bytes: 8,
            ..DecodeLimits::CANVAS
        });
        assert!(
            (options.image_href_resolver.resolve_data)("image/png", data.clone(), &options)
                .is_some()
        );
        assert!(
            (options.image_href_resolver.resolve_data)("image/png", data.clone(), &options)
                .is_none()
        );
        let svg = format!(
            "<svg xmlns='http://www.w3.org/2000/svg' width='2' height='1'><image href='data:image/png;base64,{}' width='2' height='1'/></svg>",
            STANDARD.encode(data.as_slice())
        );
        assert_eq!(
            super::super::decode_svg(svg.as_bytes(), "data image")
                .unwrap()
                .bgra
                .as_ref(),
            &[0, 0, 255, 255, 0, 0, 255, 255]
        );
    }

    #[test]
    fn nested_svg_data_urls_are_bounded_without_disabling_vector_images() {
        let mut source = "<svg xmlns='http://www.w3.org/2000/svg' width='1' height='1'><rect width='1' height='1' fill='red'/></svg>".to_string();
        for depth in 0..=MAX_NESTED_SVGS {
            source = format!(
                "<svg xmlns='http://www.w3.org/2000/svg' width='1' height='1'><image href='data:image/svg+xml;base64,{}' width='1' height='1'/></svg>",
                STANDARD.encode(source.as_bytes())
            );
            let image = super::super::decode_svg(source.as_bytes(), "nested vector image").unwrap();
            assert_eq!(
                image.bgra.as_ref(),
                if depth < MAX_NESTED_SVGS {
                    &[0, 0, 255, 255]
                } else {
                    &[0, 0, 0, 0]
                }
            );
        }
    }
}
