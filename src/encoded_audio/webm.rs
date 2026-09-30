//! Complete bounded EBML envelope policy, not a Matroska demuxer. Symphonia
//! interprets timing, block lacing and packets after this allocation preflight.

use super::Budget;

const HEADER: u32 = 0x1a45_dfa3;
const SEGMENT: u32 = 0x1853_8067;
const CLUSTER: u32 = 0x1f43_b675;
const TRACKS: u32 = 0x1654_ae6b;
const TRACK: u32 = 0xae;
const MAX_LEAF: usize = 256 * 1024;

pub(super) fn validate(bytes: &[u8], budget: &mut Budget<'_>) -> Result<(), String> {
    if !bytes.starts_with(&HEADER.to_be_bytes()) {
        return Err("WebM EBML header is missing".into());
    }
    let mut policy = Policy::default();
    walk(bytes, 0, 0, budget, &mut policy)?;
    if policy.headers != 1
        || !policy.webm
        || policy.segments != 1
        || policy.tracks != 1
        || policy.blocks == 0
    {
        return Err("WebM requires one complete document with one Vorbis audio track".into());
    }
    Ok(())
}

#[derive(Default)]
struct Policy {
    headers: usize,
    webm: bool,
    segments: usize,
    tracks: usize,
    blocks: usize,
}

fn walk(
    mut bytes: &[u8],
    parent: u32,
    depth: usize,
    budget: &mut Budget<'_>,
    policy: &mut Policy,
) -> Result<(), String> {
    if depth > 12 {
        return Err("WebM metadata nesting exceeds admission limit".into());
    }
    let mut audio_type = false;
    let mut vorbis = false;
    let mut private = false;
    let mut timestamp = false;
    while !bytes.is_empty() {
        budget.step()?;
        let (id, id_bytes) = vint(bytes, true)?;
        let id = u32::try_from(id).map_err(|_| "WebM element ID is too wide")?;
        let (size, size_bytes) = vint(&bytes[id_bytes..], false)?;
        let header_bytes = id_bytes + size_bytes;
        let unknown = size == (1_u64 << (7 * size_bytes)) - 1;
        if unknown && id != SEGMENT && id != CLUSTER {
            return Err("WebM unknown size is permitted only for Segment and Cluster".into());
        }
        let payload_bytes = if unknown && id == CLUSTER {
            unknown_cluster_length(&bytes[header_bytes..], budget)?
        } else if unknown {
            bytes.len() - header_bytes
        } else {
            usize::try_from(size).map_err(|_| "WebM element length exceeds address space")?
        };
        let end = header_bytes
            .checked_add(payload_bytes)
            .filter(|end| *end <= bytes.len())
            .ok_or("WebM element payload is truncated")?;
        let payload = &bytes[header_bytes..end];
        if !master(id) && id != 0xec && payload.len() > MAX_LEAF {
            return Err("WebM metadata or packet exceeds admission allocation limit".into());
        }
        match id {
            HEADER => {
                if parent != 0 || policy.headers != 0 || policy.segments != 0 || unknown {
                    return Err("WebM EBML header is misplaced or repeated".into());
                }
                policy.headers += 1;
            }
            SEGMENT => {
                if parent != 0 || policy.headers != 1 || policy.segments != 0 {
                    return Err("WebM Segment is misplaced or repeated".into());
                }
                policy.segments += 1;
            }
            CLUSTER if parent != SEGMENT => return Err("WebM Cluster is outside Segment".into()),
            0xe7 if parent == CLUSTER => {
                if timestamp {
                    return Err("WebM Cluster repeats its Timestamp".into());
                }
                uint(payload)?;
                timestamp = true;
            }
            0xa0 if parent != CLUSTER || !timestamp => {
                return Err("WebM BlockGroup is outside a timestamped Cluster".into());
            }
            0x4282 if parent == HEADER => {
                if policy.webm || payload != b"webm" {
                    return Err("EBML DocType is not WebM".into());
                }
                policy.webm = true;
            }
            0x42f7 if parent == HEADER => require_uint(payload, 1, "EBML read version")?,
            0x42f2 if parent == HEADER => require_uint(payload, 4, "EBML maximum ID width")?,
            0x42f3 if parent == HEADER => {
                if !(1..=8).contains(&uint(payload)?) {
                    return Err("EBML maximum size width is unsupported".into());
                }
            }
            0x4285 if parent == HEADER => {
                if !(1..=2).contains(&uint(payload)?) {
                    return Err("WebM read version is unsupported".into());
                }
            }
            TRACK => {
                if parent != TRACKS || policy.tracks != 0 {
                    return Err("WebM requires one audio track".into());
                }
                policy.tracks += 1;
            }
            0x83 if parent == TRACK => {
                if audio_type || uint(payload)? != 2 {
                    return Err("WebM video or non-audio track is unsupported".into());
                }
                audio_type = true;
            }
            0x86 if parent == TRACK => {
                if vorbis || payload != b"A_VORBIS" {
                    return Err("WebM audio codec is not Vorbis".into());
                }
                vorbis = true;
            }
            0x63a2 if parent == TRACK => {
                if private {
                    return Err("WebM repeats CodecPrivate".into());
                }
                crate::ogg_vorbis_headers::preflight_xiph_laced(payload, 8)?;
                private = true;
            }
            0xe0 | 0x6d80 | 0xe2 => {
                return Err(
                    "WebM video and encoded/encrypted/composite tracks are unsupported".into(),
                );
            }
            0xa3 | 0xa1 => {
                if (id == 0xa3 && (parent != CLUSTER || !timestamp))
                    || (id == 0xa1 && parent != 0xa0)
                {
                    return Err(
                        "WebM block is outside its timestamped Cluster or BlockGroup".into(),
                    );
                }
                if payload.is_empty() {
                    return Err("WebM block is empty".into());
                }
                policy.blocks += 1;
            }
            _ if parent == 0 && id != HEADER && id != SEGMENT && id != 0xec => {
                return Err("WebM has trailing or unsupported top-level elements".into());
            }
            _ => {}
        }
        if master(id) {
            walk(payload, id, depth + 1, budget, policy)?;
        }
        bytes = &bytes[end..];
    }
    if parent == TRACK && (!audio_type || !vorbis || !private) {
        return Err("WebM Vorbis track headers are incomplete".into());
    }
    if parent == CLUSTER && !timestamp {
        return Err("WebM Cluster has no Timestamp".into());
    }
    Ok(())
}

fn unknown_cluster_length(bytes: &[u8], budget: &mut Budget<'_>) -> Result<usize, String> {
    // An unknown-sized Cluster ends at the next Segment-level element, not
    // necessarily at EOF. Do not recursively nest following live clusters.
    let mut offset = 0_usize;
    while offset < bytes.len() {
        budget.step()?;
        let (id, width) = vint(&bytes[offset..], true)?;
        if matches!(
            id,
            0x114d_9b74
                | 0x1549_a966
                | 0x1654_ae6b
                | 0x1f43_b675
                | 0x1c53_bb6b
                | 0x1941_a469
                | 0x1043_a770
                | 0x1254_c367
        ) {
            return Ok(offset);
        }
        let (size, size_width) = vint(&bytes[offset + width..], false)?;
        if size == (1_u64 << (7 * size_width)) - 1 {
            return Err("WebM Cluster child requires a bounded finite size".into());
        }
        offset = offset
            .checked_add(width + size_width)
            .and_then(|start| {
                usize::try_from(size)
                    .ok()
                    .and_then(|size| start.checked_add(size))
            })
            .filter(|end| *end <= bytes.len())
            .ok_or("WebM Cluster child is truncated")?;
    }
    Ok(offset)
}

fn vint(bytes: &[u8], id: bool) -> Result<(u64, usize), String> {
    let first = *bytes.first().ok_or("WebM variable integer is truncated")?;
    let width = first.leading_zeros() as usize + 1;
    if width > if id { 4 } else { 8 } {
        return Err("WebM variable integer width is invalid".into());
    }
    let bytes = bytes
        .get(..width)
        .ok_or("WebM variable integer is truncated")?;
    let mut value = u64::from(if id {
        first
    } else {
        first & ((0xff_u16 >> width) as u8)
    });
    for byte in &bytes[1..] {
        value = (value << 8) | u64::from(*byte);
    }
    Ok((value, width))
}

fn uint(bytes: &[u8]) -> Result<u64, String> {
    if bytes.is_empty() || bytes.len() > 8 {
        return Err("WebM unsigned metadata width is invalid".into());
    }
    Ok(bytes
        .iter()
        .fold(0, |value, byte| (value << 8) | u64::from(*byte)))
}

fn require_uint(bytes: &[u8], expected: u64, name: &str) -> Result<(), String> {
    if uint(bytes)? != expected {
        return Err(format!("{name} is unsupported"));
    }
    Ok(())
}

fn master(id: u32) -> bool {
    // Traverse supported EBML master envelopes even when their metadata is
    // irrelevant to audio, so nested claimed binary sizes cannot bypass bounds.
    matches!(
        id,
        HEADER
            | SEGMENT
            | CLUSTER
            | TRACKS
            | TRACK
            | 0xe1
            | 0xa0
            | 0x1549_a966
            | 0x114d_9b74
            | 0x4dbb
            | 0x1c53_bb6b
            | 0xbb
            | 0xb7
            | 0x1254_c367
            | 0x7373
            | 0x63c0
            | 0x67c8
            | 0x1043_a770
            | 0x45b9
            | 0xb6
            | 0x80
            | 0x1941_a469
            | 0x61a7
            | 0x4281
            | 0x6944
            | 0x6911
            | 0x8f
            | 0x4520
            | 0x75a1
            | 0xa6
            | 0xc8
            | 0x8e
            | 0xe8
            | 0x5854
            | 0xdb
            | 0x6924
            | 0x41e4
            | 0x6624
    )
}

#[cfg(test)]
#[path = "webm_tests.rs"]
mod tests;
