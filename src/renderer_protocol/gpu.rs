//! Canvas texture metadata only; native resource admission remains with the browser.
use super::{
    ProtocolError,
    wire::{WireReader, WireWriter},
};
use crate::gpu_frame::GpuFrame;
pub use crate::gpu_frame::PresentedGpuCanvas;
use std::collections::HashSet;

fn validate(values: &[PresentedGpuCanvas]) -> Result<(), ProtocolError> {
    if values.len() > crate::limits::MAX_PRESENTED_CANVASES {
        return Err(ProtocolError::InvalidPayload("GPU Canvas count"));
    }
    let mut keys = HashSet::new();
    let mut surfaces = HashSet::new();
    let mut bytes = 0_u64;
    for value in values {
        if !value.url.starts_with("breeze-internal:canvas:")
            || value.url.len() > 128
            || !keys.insert(&value.url)
            || !surfaces.insert(value.frame.surface)
            || !valid_canvas_key(&value.url)
            || !value.frame.validate()
        {
            return Err(ProtocolError::InvalidPayload("GPU Canvas descriptor"));
        }
        bytes += u64::from(value.frame.width) * u64::from(value.frame.height) * 4;
    }
    if bytes > 64 * 1024 * 1024 {
        return Err(ProtocolError::InvalidPayload("GPU Canvas byte budget"));
    }
    Ok(())
}

fn valid_canvas_key(url: &str) -> bool {
    let Some(key) = url.strip_prefix("breeze-internal:canvas:") else {
        return false;
    };
    let Some((node, size)) = key.split_once(':') else {
        return false;
    };
    let Some((width, height)) = size.split_once('x') else {
        return false;
    };
    let decimal = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    decimal(node)
        && node.parse::<u128>().is_ok_and(|value| value != 0)
        && decimal(width)
        && width.parse::<u32>().is_ok_and(|value| value != 0)
        && decimal(height)
        && height.parse::<u32>().is_ok_and(|value| value != 0)
}

pub(super) fn encode(
    writer: &mut WireWriter,
    values: &[PresentedGpuCanvas],
) -> Result<(), ProtocolError> {
    validate(values)?;
    writer.u32(values.len() as u32);
    for value in values {
        writer.string(&value.url)?;
        let frame = value.frame;
        writer.u64(frame.surface);
        writer.u64(frame.handle);
        writer.u64(frame.key);
        writer.u32(frame.width);
        writer.u32(frame.height);
        writer.u32(frame.adapter_low);
        writer.u32(frame.adapter_high as u32);
    }
    Ok(())
}

pub(super) fn decode(
    reader: &mut WireReader<'_>,
) -> Result<Vec<PresentedGpuCanvas>, ProtocolError> {
    let count = reader.u32()? as usize;
    if count > crate::limits::MAX_PRESENTED_CANVASES {
        return Err(ProtocolError::InvalidPayload("GPU Canvas count"));
    }
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        values.push(PresentedGpuCanvas {
            url: reader.string(128)?,
            frame: GpuFrame {
                surface: reader.u64()?,
                handle: reader.u64()?,
                key: reader.u64()?,
                width: reader.u32()?,
                height: reader.u32()?,
                adapter_low: reader.u32()?,
                adapter_high: reader.u32()? as i32,
            },
        });
    }
    validate(&values)?;
    Ok(values)
}

#[cfg(test)]
mod tests;
