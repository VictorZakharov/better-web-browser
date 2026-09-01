use super::boxes::{self, BoxView};

#[derive(Clone, Copy, Default)]
pub(super) struct SampleDefaults {
    pub(super) duration: u32,
    pub(super) size: u32,
    pub(super) flags: u32,
}

#[derive(Clone)]
pub(super) struct Metadata {
    pub(super) track_id: u32,
    pub(super) timescale: u32,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) nal_length_size: usize,
    pub(super) sequence_header: Vec<u8>,
    pub(super) defaults: SampleDefaults,
}

struct AvcDescription {
    width: u32,
    height: u32,
    nal_length_size: usize,
    sequence_header: Vec<u8>,
}

pub(super) fn parse(moov: BoxView<'_>) -> Result<Metadata, String> {
    for trak in boxes::parse(moov.payload, moov.start + moov.header)?
        .into_iter()
        .filter(|entry| entry.kind == *b"trak")
    {
        let Some(stsd) = boxes::descendant(trak, &[b"mdia", b"minf", b"stbl", b"stsd"])? else {
            continue;
        };
        let Some(description) = parse_avc(stsd)? else {
            continue;
        };
        let tkhd = boxes::descendant(trak, &[b"tkhd"])?
            .ok_or_else(|| "H.264 track has no track header".to_string())?;
        let track_id = parse_track_id(tkhd.payload)?;
        let mdhd = boxes::descendant(trak, &[b"mdia", b"mdhd"])?
            .ok_or_else(|| "H.264 track has no media header".to_string())?;
        let timescale = parse_timescale(mdhd.payload)?;
        return Ok(Metadata {
            track_id,
            timescale,
            width: description.width,
            height: description.height,
            nal_length_size: description.nal_length_size,
            sequence_header: description.sequence_header,
            defaults: parse_trex(moov, track_id)?.unwrap_or_default(),
        });
    }
    Err("fragmented MP4 has no AVC video sample description".into())
}

fn parse_avc(stsd: BoxView<'_>) -> Result<Option<AvcDescription>, String> {
    if stsd.payload.len() < 8 {
        return Err("fragmented MP4 sample description is truncated".into());
    }
    for entry in boxes::parse(&stsd.payload[8..], stsd.start + stsd.header + 8)? {
        if !matches!(&entry.kind, b"avc1" | b"avc3") {
            continue;
        }
        if entry.payload.len() < 78 {
            return Err("AVC visual sample entry is truncated".into());
        }
        let width = u16::from_be_bytes(entry.payload[24..26].try_into().unwrap()) as u32;
        let height = u16::from_be_bytes(entry.payload[26..28].try_into().unwrap()) as u32;
        let avcc = boxes::parse(&entry.payload[78..], entry.start + entry.header + 78)?
            .into_iter()
            .find(|child| child.kind == *b"avcC")
            .ok_or_else(|| "AVC sample entry has no configuration box".to_string())?;
        let (length_size, header) = parse_avcc(avcc.payload)?;
        return Ok(Some(AvcDescription {
            width,
            height,
            nal_length_size: length_size,
            sequence_header: header,
        }));
    }
    Ok(None)
}

fn parse_avcc(payload: &[u8]) -> Result<(usize, Vec<u8>), String> {
    if payload.len() < 7 || payload[0] != 1 {
        return Err("AVC configuration is truncated or unsupported".into());
    }
    let length_size = usize::from((payload[4] & 3) + 1);
    let mut offset = 6_usize;
    let sps_count = payload[5] & 0x1f;
    let mut header = Vec::new();
    for _ in 0..sps_count {
        append_parameter_set(payload, &mut offset, &mut header)?;
    }
    if offset >= payload.len() {
        return Err("AVC configuration has no PPS count".into());
    }
    let pps_count = payload[offset];
    offset += 1;
    for _ in 0..pps_count {
        append_parameter_set(payload, &mut offset, &mut header)?;
    }
    if header.is_empty() || sps_count == 0 || pps_count == 0 {
        return Err("AVC configuration has no SPS/PPS".into());
    }
    Ok((length_size, header))
}

fn append_parameter_set(
    payload: &[u8],
    offset: &mut usize,
    output: &mut Vec<u8>,
) -> Result<(), String> {
    if payload.len().saturating_sub(*offset) < 2 {
        return Err("AVC parameter-set length is truncated".into());
    }
    let length = u16::from_be_bytes(payload[*offset..*offset + 2].try_into().unwrap()) as usize;
    *offset += 2;
    if length == 0 || length > payload.len().saturating_sub(*offset) {
        return Err("AVC parameter set is truncated".into());
    }
    output.extend_from_slice(&[0, 0, 0, 1]);
    output.extend_from_slice(&payload[*offset..*offset + length]);
    *offset += length;
    Ok(())
}

fn parse_trex(moov: BoxView<'_>, track_id: u32) -> Result<Option<SampleDefaults>, String> {
    let Some(mvex) = boxes::descendant(moov, &[b"mvex"])? else {
        return Ok(None);
    };
    for trex in boxes::parse(mvex.payload, mvex.start + mvex.header)?
        .into_iter()
        .filter(|entry| entry.kind == *b"trex")
    {
        if trex.payload.len() < 24 || boxes::u32_at(trex.payload, 4)? != track_id {
            continue;
        }
        return Ok(Some(SampleDefaults {
            duration: boxes::u32_at(trex.payload, 12)?,
            size: boxes::u32_at(trex.payload, 16)?,
            flags: boxes::u32_at(trex.payload, 20)?,
        }));
    }
    Ok(None)
}

fn parse_track_id(payload: &[u8]) -> Result<u32, String> {
    if payload.is_empty() {
        return Err("track header is truncated".into());
    }
    boxes::u32_at(payload, if payload[0] == 1 { 20 } else { 12 })
}

fn parse_timescale(payload: &[u8]) -> Result<u32, String> {
    if payload.is_empty() {
        return Err("media header is truncated".into());
    }
    let value = boxes::u32_at(payload, if payload[0] == 1 { 20 } else { 12 })?;
    if value == 0 {
        Err("media timescale is zero".into())
    } else {
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avcc_becomes_annex_b_sequence_header() {
        let payload = [1, 100, 0, 31, 0xff, 0xe1, 0, 2, 0x67, 1, 1, 0, 2, 0x68, 2];
        let (length, header) = parse_avcc(&payload).unwrap();
        assert_eq!(length, 4);
        assert_eq!(header, [0, 0, 0, 1, 0x67, 1, 0, 0, 0, 1, 0x68, 2]);
    }
}
