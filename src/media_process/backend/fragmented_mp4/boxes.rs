const MAX_DEPTH: usize = 8;
const MAX_TOP_LEVEL_BOXES: usize = 12;

#[derive(Clone, Copy)]
pub(super) struct BoxView<'a> {
    pub(super) kind: [u8; 4],
    pub(super) payload: &'a [u8],
    pub(super) start: usize,
    pub(super) size: usize,
    pub(super) header: usize,
}

pub(super) fn parse(bytes: &[u8], base: usize) -> Result<Vec<BoxView<'_>>, String> {
    let mut output = Vec::new();
    let mut offset = 0_usize;
    while offset < bytes.len() {
        if bytes.len() - offset < 8 {
            return Err("ISO-BMFF box header is truncated".into());
        }
        let declared = u32_at(bytes, offset)? as usize;
        let (header, size) = if declared == 1 {
            (
                16,
                usize::try_from(u64_at(bytes, offset + 8)?)
                    .map_err(|_| "ISO-BMFF box is too large")?,
            )
        } else if declared == 0 {
            (8, bytes.len() - offset)
        } else {
            (8, declared)
        };
        if size < header || size > bytes.len() - offset {
            return Err("ISO-BMFF box size is invalid".into());
        }
        output.push(BoxView {
            kind: bytes[offset + 4..offset + 8].try_into().unwrap(),
            payload: &bytes[offset + header..offset + size],
            start: base + offset,
            size,
            header,
        });
        offset += size;
    }
    Ok(output)
}

pub(super) fn descendant<'a>(
    root: BoxView<'a>,
    path: &[&[u8; 4]],
) -> Result<Option<BoxView<'a>>, String> {
    let Some((first, rest)) = path.split_first() else {
        return Ok(Some(root));
    };
    for child in parse(root.payload, root.start + root.header)? {
        if child.kind == **first {
            return if rest.is_empty() {
                Ok(Some(child))
            } else {
                descendant(child, rest)
            };
        }
    }
    Ok(None)
}

pub(super) fn flags(bytes: &[u8]) -> Result<u32, String> {
    if bytes.len() < 4 {
        return Err("ISO-BMFF full-box header is truncated".into());
    }
    Ok(u32::from_be_bytes([0, bytes[1], bytes[2], bytes[3]]))
}

pub(super) fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, String> {
    bytes
        .get(offset..offset + 4)
        .and_then(|value| value.try_into().ok())
        .map(u32::from_be_bytes)
        .ok_or_else(|| "ISO-BMFF integer is truncated".into())
}

pub(super) fn u64_at(bytes: &[u8], offset: usize) -> Result<u64, String> {
    bytes
        .get(offset..offset + 8)
        .and_then(|value| value.try_into().ok())
        .map(u64::from_be_bytes)
        .ok_or_else(|| "ISO-BMFF integer is truncated".into())
}

pub(super) fn summary(bytes: &[u8]) -> String {
    let mut names = Vec::new();
    let mut samples = 0_u64;
    walk_summary(bytes, 0, true, &mut names, &mut samples);
    format!("boxes={},trun_samples={samples}", names.join("/"))
}

fn walk_summary(bytes: &[u8], depth: usize, top: bool, names: &mut Vec<String>, samples: &mut u64) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = parse(bytes, 0) else {
        return;
    };
    for entry in entries {
        if top && names.len() < MAX_TOP_LEVEL_BOXES {
            names.push(format!(
                "{}:{}",
                std::str::from_utf8(&entry.kind).unwrap_or("????"),
                entry.size
            ));
        }
        if entry.kind == *b"trun" && entry.payload.len() >= 8 {
            *samples = samples.saturating_add(u32_at(entry.payload, 4).unwrap_or(0) as u64);
        } else if matches!(
            &entry.kind,
            b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl" | b"mvex" | b"moof" | b"traf"
        ) {
            walk_summary(entry.payload, depth + 1, false, names, samples);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarizes_top_level_boxes_and_fragment_samples() {
        let trun = box_bytes(b"trun", &[0, 0, 0, 0, 0, 0, 0, 3]);
        let traf = box_bytes(b"traf", &trun);
        let moof = box_bytes(b"moof", &traf);
        let mdat = box_bytes(b"mdat", &[1, 2, 3, 4]);
        assert_eq!(
            summary(&[moof, mdat].concat()),
            "boxes=moof:32/mdat:12,trun_samples=3"
        );
    }

    #[test]
    fn rejects_a_box_that_exceeds_its_source() {
        assert!(parse(&[0, 0, 0, 16, b'm', b'd', b'a', b't'], 0).is_err());
    }

    fn box_bytes(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&((payload.len() + 8) as u32).to_be_bytes());
        bytes.extend_from_slice(kind);
        bytes.extend_from_slice(payload);
        bytes
    }
}
