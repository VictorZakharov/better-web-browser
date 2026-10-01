//! AV1 sample conversion, with explicit handling of CICP matrix/range and alpha.

#[path = "planes.rs"]
mod planes;
#[cfg(test)]
mod tests;

use super::super::{DecodeLimits, DecodeResult, RasterImage, color};
use super::{decoder::Picture, metadata::Cicp};
use planes::Planes;
use rav1d::include::dav1d::headers::*;
use yuv::{YuvPlanarImage, YuvRange, YuvStandardMatrix};

fn encoding(picture: &Picture, override_: Option<Cicp>) -> DecodeResult<Cicp> {
    // SAFETY: the picture owns its sequence header until its Drop runs.
    let header = unsafe {
        picture
            .0
            .seq_hdr
            .ok_or("AV1 picture has no sequence header")?
            .as_ref()
    };
    Ok(override_.unwrap_or(Cicp {
        primaries: header.pri as u16,
        transfer: header.trc as u16,
        matrix: header.mtrx as u16,
        full_range: header.color_range != 0,
    }))
}

pub(super) fn rgba(
    picture: &Picture,
    limits: DecodeLimits,
    override_: Option<Cicp>,
) -> DecodeResult<RasterImage> {
    let width = u32::try_from(picture.0.p.w).map_err(|_| "invalid AV1 width")?;
    let height = u32::try_from(picture.0.p.h).map_err(|_| "invalid AV1 height")?;
    let mut output = vec![0; limits.rgba_len(width, height)?];
    let cicp = encoding(picture, override_)?;
    match picture.0.p.bpc {
        8 => {
            let planes = Planes::<u8>::copy(picture, limits)?;
            if picture.0.p.layout == DAV1D_PIXEL_LAYOUT_I400 {
                monochrome(&planes.y, &mut output, 8, cicp.full_range);
            } else if cicp.matrix == 0 {
                identity(&planes, &mut output, 8, cicp.full_range)?;
            } else {
                convert8(&planes, picture.0.p.layout, &mut output, cicp)?;
            }
        }
        depth @ (10 | 12) => {
            let planes = Planes::<u16>::copy(picture, limits)?;
            if picture.0.p.layout == DAV1D_PIXEL_LAYOUT_I400 {
                monochrome(&planes.y, &mut output, depth as u32, cicp.full_range);
            } else if cicp.matrix == 0 {
                identity(&planes, &mut output, depth as u32, cicp.full_range)?;
            } else {
                convert16(&planes, picture.0.p.layout, depth, &mut output, cicp)?;
            }
        }
        _ => return Err("unsupported AV1 image bit depth".into()),
    }
    RasterImage::new(width, height, output, limits)
}

fn matrix(value: u16) -> DecodeResult<YuvStandardMatrix> {
    Ok(match value {
        1 => YuvStandardMatrix::Bt709,
        // Unspecified matrix is interpreted as BT.601, matching AVIF decoders.
        2 | 5 | 6 => YuvStandardMatrix::Bt601,
        4 => YuvStandardMatrix::Fcc,
        7 => YuvStandardMatrix::Smpte240,
        9 => YuvStandardMatrix::Bt2020,
        _ => return Err(format!("unsupported AVIF CICP matrix {value}")),
    })
}

fn planar<T: Copy + std::fmt::Debug>(planes: &Planes<T>) -> YuvPlanarImage<'_, T> {
    YuvPlanarImage {
        y_plane: &planes.y,
        y_stride: planes.width,
        u_plane: &planes.u,
        u_stride: planes.chroma_width,
        v_plane: &planes.v,
        v_stride: planes.chroma_width,
        width: planes.width,
        height: planes.height,
    }
}

fn range(cicp: Cicp) -> YuvRange {
    if cicp.full_range {
        YuvRange::Full
    } else {
        YuvRange::Limited
    }
}

fn convert8(planes: &Planes<u8>, layout: u32, rgba: &mut [u8], cicp: Cicp) -> DecodeResult<()> {
    let convert = match layout {
        DAV1D_PIXEL_LAYOUT_I420 => yuv::yuv420_to_rgba,
        DAV1D_PIXEL_LAYOUT_I422 => yuv::yuv422_to_rgba,
        DAV1D_PIXEL_LAYOUT_I444 => yuv::yuv444_to_rgba,
        _ => return Err("unsupported AV1 chroma layout".into()),
    };
    convert(
        &planar(planes),
        rgba,
        planes.width * 4,
        range(cicp),
        matrix(cicp.matrix)?,
    )
    .map_err(|error| format!("convert AVIF YUV: {error}"))
}

fn convert16(
    planes: &Planes<u16>,
    layout: u32,
    depth: i32,
    rgba: &mut [u8],
    cicp: Cicp,
) -> DecodeResult<()> {
    let convert = match (layout, depth) {
        (DAV1D_PIXEL_LAYOUT_I420, 10) => yuv::i010_to_rgba10,
        (DAV1D_PIXEL_LAYOUT_I422, 10) => yuv::i210_to_rgba10,
        (DAV1D_PIXEL_LAYOUT_I444, 10) => yuv::i410_to_rgba10,
        (DAV1D_PIXEL_LAYOUT_I420, 12) => yuv::i012_to_rgba12,
        (DAV1D_PIXEL_LAYOUT_I422, 12) => yuv::i212_to_rgba12,
        (DAV1D_PIXEL_LAYOUT_I444, 12) => yuv::i412_to_rgba12,
        _ => return Err("unsupported AV1 high-bit-depth layout".into()),
    };
    // yuv 0.8.19's scalar 16-to-8 path clamps to the *source* bit depth
    // after scaling to 8 bits, allowing saturated channels to wrap on cast.
    // Reuse its correctly saturated 16-to-16 transform, then narrow samples.
    // Two-row scratch supports 4:2:0 without another full-frame allocation.
    let rows = if layout == DAV1D_PIXEL_LAYOUT_I420 {
        2
    } else {
        1
    };
    let mut scratch = vec![0u16; planes.width as usize * 4 * rows];
    for top in (0..planes.height as usize).step_by(rows) {
        let count = rows.min(planes.height as usize - top);
        let chroma_top = if rows == 2 { top / 2 } else { top };
        let chroma_rows = if rows == 2 { 1 } else { count };
        let y_start = top * planes.width as usize;
        let uv_start = chroma_top * planes.chroma_width as usize;
        let image = YuvPlanarImage {
            y_plane: &planes.y[y_start..y_start + count * planes.width as usize],
            y_stride: planes.width,
            u_plane: &planes.u[uv_start..uv_start + chroma_rows * planes.chroma_width as usize],
            v_plane: &planes.v[uv_start..uv_start + chroma_rows * planes.chroma_width as usize],
            u_stride: planes.chroma_width,
            v_stride: planes.chroma_width,
            width: planes.width,
            height: count as u32,
        };
        let samples = count * planes.width as usize * 4;
        convert(
            &image,
            &mut scratch[..samples],
            planes.width * 4,
            range(cicp),
            matrix(cicp.matrix)?,
        )
        .map_err(|error| format!("convert high-bit-depth AVIF YUV: {error}"))?;
        let destination = &mut rgba[y_start * 4..y_start * 4 + samples];
        for (&value, output) in scratch[..samples].iter().zip(destination) {
            *output = sample(u32::from(value), depth as u32, true);
        }
    }
    Ok(())
}

fn sample(value: u32, depth: u32, full: bool) -> u8 {
    let (black, white) = if full {
        (0, (1 << depth) - 1)
    } else {
        (16 << (depth - 8), 235 << (depth - 8))
    };
    ((value.saturating_sub(black).min(white - black) * 255 + (white - black) / 2) / (white - black))
        as u8
}

fn monochrome<T: Copy + Into<u32>>(y: &[T], output: &mut [u8], depth: u32, full: bool) {
    for (&value, pixel) in y.iter().zip(output.chunks_exact_mut(4)) {
        let value = sample(value.into(), depth, full);
        pixel.copy_from_slice(&[value, value, value, 255]);
    }
}

fn identity<T: Copy + Into<u32>>(
    planes: &Planes<T>,
    output: &mut [u8],
    depth: u32,
    full: bool,
) -> DecodeResult<()> {
    if planes.y.len() != planes.u.len() || planes.y.len() != planes.v.len() {
        return Err("AVIF identity matrix requires full-resolution RGB planes".into());
    }
    for (((&green, &blue), &red), pixel) in planes
        .y
        .iter()
        .zip(&planes.u)
        .zip(&planes.v)
        .zip(output.chunks_exact_mut(4))
    {
        pixel.copy_from_slice(&[
            sample(red.into(), depth, full),
            sample(green.into(), depth, full),
            sample(blue.into(), depth, full),
            255,
        ]);
    }
    Ok(())
}

pub(super) fn apply_alpha(
    image: &mut RasterImage,
    picture: &Picture,
    limits: DecodeLimits,
) -> DecodeResult<()> {
    if (picture.0.p.w, picture.0.p.h) != (image.width as i32, image.height as i32) {
        return Err("AVIF color and alpha dimensions differ".into());
    }
    if picture.0.p.layout != DAV1D_PIXEL_LAYOUT_I400 {
        return Err("AVIF alpha item is not monochrome".into());
    }
    // AVIF §4: auxiliary alpha is full-range irrespective of color metadata.
    match picture.0.p.bpc {
        8 => {
            let planes = Planes::<u8>::copy(picture, limits)?;
            for (&alpha, pixel) in planes.y.iter().zip(image.rgba.chunks_exact_mut(4)) {
                pixel[3] = alpha;
            }
        }
        depth @ (10 | 12) => {
            let planes = Planes::<u16>::copy(picture, limits)?;
            for (&alpha, pixel) in planes.y.iter().zip(image.rgba.chunks_exact_mut(4)) {
                pixel[3] = sample(u32::from(alpha), depth as u32, true);
            }
        }
        _ => return Err("unsupported AVIF alpha bit depth".into()),
    }
    Ok(())
}

pub(super) fn apply_color_encoding(
    image: &mut RasterImage,
    picture: &Picture,
    override_: Option<Cicp>,
) -> DecodeResult<()> {
    let cicp = encoding(picture, override_)?;
    if matches!(cicp.primaries, 1 | 2) && matches!(cicp.transfer, 2 | 13) {
        return Ok(());
    }
    let primaries = u8::try_from(if cicp.primaries == 2 {
        1
    } else {
        cicp.primaries
    })
    .ok()
    .and_then(|value| value.try_into().ok())
    .ok_or("unsupported AVIF color primaries")?;
    let transfer = u8::try_from(if cicp.transfer == 2 {
        13
    } else {
        cicp.transfer
    })
    .ok()
    .and_then(|value| value.try_into().ok())
    .ok_or("unsupported AVIF transfer function")?;
    let profile = moxcms::ColorProfile::new_from_cicp(moxcms::CicpProfile {
        color_primaries: primaries,
        transfer_characteristics: transfer,
        // The YUV-to-RGB transform has already consumed matrix/range.
        matrix_coefficients: moxcms::MatrixCoefficients::Identity,
        full_range: true,
    });
    color::apply_profile(image, &profile)
}
