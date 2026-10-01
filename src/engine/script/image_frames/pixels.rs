//! CPU frame conversion reuses the BSD-3-Clause/Apache-2.0 yuv dependency.
//! The bridge validates every dimension and plane before entering that library.
use crate::engine::image_decode::DecodeLimits;
use crate::engine::script::{JsNativeError, JsResult, JsValue};
use yuv::{YuvBiPlanarImage, YuvPlanarImage, YuvRange, YuvStandardMatrix};

#[cfg(test)]
mod tests;

pub(super) fn convert(
    width: u32,
    height: u32,
    format: &str,
    planes: &[&[u8]],
    full: bool,
    matrix: &str,
) -> Result<Vec<u8>, String> {
    let length = DecodeLimits::CANVAS.rgba_len(width, height)?;
    let range = if full {
        YuvRange::Full
    } else {
        YuvRange::Limited
    };
    let matrix = match matrix {
        "bt709" => YuvStandardMatrix::Bt709,
        "bt470bg" | "smpte170m" => YuvStandardMatrix::Bt601,
        _ => return Err("unsupported frame YUV matrix".into()),
    };
    let (chroma_width, chroma_height, count) = match format {
        "I420" => (width.div_ceil(2), height.div_ceil(2), 3),
        "I420A" => (width.div_ceil(2), height.div_ceil(2), 4),
        "NV12" => (width.div_ceil(2), height.div_ceil(2), 2),
        "I422" => (width.div_ceil(2), height, 3),
        "I422A" => (width.div_ceil(2), height, 4),
        "I444" => (width, height, 3),
        "I444A" => (width, height, 4),
        _ => return Err("unsupported frame sample format".into()),
    };
    if planes.len() != count {
        return Err("incorrect number of frame planes".into());
    }
    let luma = width as usize * height as usize;
    let chroma = chroma_width as usize * chroma_height as usize;
    for (index, plane) in planes.iter().enumerate() {
        let expected = if index == 0 || index == 3 {
            luma
        } else if format == "NV12" {
            chroma * 2
        } else {
            chroma
        };
        if plane.len() != expected {
            return Err("frame plane length does not match its dimensions".into());
        }
    }
    let mut output = vec![0; length];
    if format == "NV12" {
        yuv::yuv_nv12_to_rgba(
            &YuvBiPlanarImage {
                y_plane: planes[0],
                y_stride: width,
                uv_plane: planes[1],
                uv_stride: chroma_width * 2,
                width,
                height,
            },
            &mut output,
            width * 4,
            range,
            matrix,
            yuv::YuvConversionMode::Balanced,
        )
        .map_err(|e| e.to_string())?;
    } else {
        let image = YuvPlanarImage {
            y_plane: planes[0],
            y_stride: width,
            u_plane: planes[1],
            u_stride: chroma_width,
            v_plane: planes[2],
            v_stride: chroma_width,
            width,
            height,
        };
        let conversion = match format {
            "I420" | "I420A" => yuv::yuv420_to_rgba,
            "I422" | "I422A" => yuv::yuv422_to_rgba,
            _ => yuv::yuv444_to_rgba,
        };
        conversion(&image, &mut output, width * 4, range, matrix).map_err(|e| e.to_string())?;
        if count == 4 {
            for (pixel, alpha) in output.chunks_exact_mut(4).zip(planes[3]) {
                pixel[3] = *alpha;
            }
        }
    }
    Ok(output)
}

pub(super) fn dispatch(args: &[JsValue]) -> JsResult<JsValue> {
    let width = super::host::integer(args, 1)?;
    let height = super::host::integer(args, 2)?;
    let format = args.get(3).map(JsValue::string_value).unwrap_or_default();
    let Some(JsValue::Array(planes)) = args.get(4) else {
        return Err(JsNativeError::typ()
            .with_message("frame planes must be byte arrays")
            .into());
    };
    let planes = planes
        .iter()
        .map(|value| {
            value
                .as_bytes()
                .ok_or_else(|| JsNativeError::typ().with_message("frame plane is not a byte array"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let full = matches!(args.get(5), Some(JsValue::Boolean(true)));
    let matrix = args.get(6).map(JsValue::string_value).unwrap_or_default();
    convert(width, height, &format, &planes, full, &matrix)
        .map(JsValue::Bytes)
        .map_err(|message| JsNativeError::range().with_message(message).into())
}
