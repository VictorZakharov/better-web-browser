//! Bounded physical Ogg policy, not a replacement for the upstream demuxer.
//! Check complete framing and allocation bounds before PacketReader reassembly.

use ogg::reading::PageParser;

pub(crate) struct Page {
    pub(crate) granule: u64,
    pub(crate) segments: usize,
    pub(crate) completed_packets: usize,
    pub(crate) continued: bool,
    pub(crate) eos: bool,
}

pub(crate) struct Envelope {
    pub(crate) serial: u32,
    pub(crate) pages: Vec<Page>,
}

pub(crate) fn inspect(
    bytes: &[u8],
    packet_limit: impl Fn(usize) -> usize,
    mut step: impl FnMut() -> Result<(), String>,
) -> Result<Envelope, String> {
    let mut remaining = bytes;
    let mut serial = None;
    let mut sequence = 0_u32;
    let mut continued = false;
    let mut packet_bytes = 0_usize;
    let mut packet_index = 0_usize;
    let mut ended = false;
    let mut pages = Vec::new();
    while !remaining.is_empty() {
        step()?;
        let header: [u8; 27] = remaining
            .get(..27)
            .and_then(|head| head.try_into().ok())
            .ok_or("Ogg page header is truncated")?;
        let current_serial = u32::from_le_bytes(header[14..18].try_into().unwrap());
        let current_sequence = u32::from_le_bytes(header[18..22].try_into().unwrap());
        let flags = header[5];
        if header[..4] != *b"OggS"
            || flags & !7 != 0
            || ended
            || current_sequence != sequence
            || serial.is_some_and(|value| value != current_serial)
            || (flags & 1 != 0) != continued
            || (flags & 2 != 0) != serial.is_none()
        {
            return Err("Ogg requires consecutive pages of one complete logical stream".into());
        }
        serial = Some(current_serial);
        sequence = sequence
            .checked_add(1)
            .ok_or("Ogg page sequence overflows admission limit")?;
        let (mut parser, segments) =
            PageParser::new(header).map_err(|error| format!("read Ogg page: {error}"))?;
        let laces = remaining
            .get(27..27 + segments)
            .ok_or("Ogg page segment table is truncated")?;
        let mut completed_packets = 0;
        for lace in laces {
            packet_bytes = packet_bytes
                .checked_add(usize::from(*lace))
                .ok_or("Ogg packet size overflows admission limit")?;
            if packet_bytes > packet_limit(packet_index) {
                return Err("Ogg packet exceeds admission allocation limit".into());
            }
            continued = *lace == 255;
            if !continued {
                packet_bytes = 0;
                packet_index = packet_index
                    .checked_add(1)
                    .ok_or("Ogg packet count overflows admission limit")?;
                completed_packets += 1;
            }
        }
        let body_bytes = parser.parse_segments(laces.to_vec());
        let end = 27 + segments + body_bytes;
        let body = remaining
            .get(27 + segments..end)
            .ok_or("Ogg page body is truncated")?;
        parser
            .parse_packet_data(body.to_vec())
            .map_err(|error| format!("verify Ogg page: {error}"))?;
        ended = flags & 4 != 0;
        if ended && continued {
            return Err("Ogg EOS leaves an incomplete packet".into());
        }
        pages.push(Page {
            granule: u64::from_le_bytes(header[6..14].try_into().unwrap()),
            segments,
            completed_packets,
            continued,
            eos: ended,
        });
        remaining = &remaining[end..];
    }
    if !ended || pages.len() < 3 {
        return Err("Ogg source has no complete EOS audio page".into());
    }
    Ok(Envelope {
        serial: serial.ok_or("Ogg source has no pages")?,
        pages,
    })
}
