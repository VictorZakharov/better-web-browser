//! Bounded metadata iteration after shared envelope preflight. Not a demuxer.

use crate::encoded_audio::{Budget, webm};

pub(super) const SEGMENT: u32 = 0x1853_8067;
pub(super) const CLUSTER: u32 = 0x1f43_b675;
pub(super) const TRACKS: u32 = 0x1654_ae6b;
pub(super) const TRACK: u32 = 0xae;
pub(super) const INFO: u32 = 0x1549_a966;

pub(super) struct Element<'a> {
    pub(super) id: u32,
    pub(super) data: &'a [u8],
    pub(super) unknown_size: bool,
}

pub(super) fn children<'a>(
    mut bytes: &'a [u8],
    budget: &mut Budget<'_>,
    mut visit: impl FnMut(Element<'a>, &mut Budget<'_>) -> Result<(), String>,
) -> Result<(), String> {
    while !bytes.is_empty() {
        // Each finite source element consumes a work unit even when ignored.
        budget.step()?;
        let (id, width) = webm::vint(bytes, true)?;
        let (size, size_width) = webm::vint(&bytes[width..], false)?;
        let id = u32::try_from(id).map_err(|_| "WebM element ID overflow")?;
        let offset = width + size_width;
        let unknown = size == (1_u64 << (7 * size_width)) - 1;
        let len = if unknown {
            match id {
                SEGMENT => bytes.len() - offset,
                CLUSTER => webm::unknown_cluster_length(&bytes[offset..], budget)?,
                _ => return Err("WebM metadata element has unknown size".into()),
            }
        } else {
            usize::try_from(size).map_err(|_| "WebM element length overflow")?
        };
        let end = offset
            .checked_add(len)
            .filter(|end| *end <= bytes.len())
            .ok_or("WebM metadata element is truncated")?;
        visit(
            Element {
                id,
                data: &bytes[offset..end],
                unknown_size: unknown,
            },
            budget,
        )?;
        bytes = &bytes[end..];
    }
    Ok(())
}

pub(super) fn unique<T>(slot: &mut Option<T>, value: T, name: &str) -> Result<(), String> {
    if slot.replace(value).is_some() {
        Err(format!("WebM Opus repeats {name}"))
    } else {
        Ok(())
    }
}

pub(super) fn signed(bytes: &[u8]) -> Result<i64, String> {
    if bytes.is_empty() || bytes.len() > 8 {
        return Err("WebM signed metadata width is invalid".into());
    }
    let mut result = if bytes[0] & 0x80 != 0 { -1_i64 } else { 0 };
    for byte in bytes {
        result = (result << 8) | i64::from(*byte);
    }
    Ok(result)
}

pub(super) fn float(bytes: &[u8]) -> Result<f64, String> {
    match bytes.len() {
        4 => Ok(f64::from(f32::from_be_bytes(bytes.try_into().unwrap()))),
        8 => Ok(f64::from_be_bytes(bytes.try_into().unwrap())),
        _ => Err("WebM floating metadata width is invalid".into()),
    }
}
