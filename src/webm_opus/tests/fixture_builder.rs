//! Owned test envelopes, deliberately independent of the production serializer.
//! Compressed packets come from our FFmpeg-generated Ogg tones, not a decoder
//! mock. This builder can express malformed metadata the upstream writer refuses.

use super::*;
use ogg::reading::PacketReader;
use std::io::Cursor;

#[derive(Clone)]
pub(super) struct Field {
    pub(super) id: u32,
    pub(super) data: Vec<u8>,
    pub(super) unknown: bool,
}

impl Field {
    pub(super) fn new(id: u32, data: impl Into<Vec<u8>>) -> Self {
        Self {
            id,
            data: data.into(),
            unknown: false,
        }
    }

    pub(super) fn uint(id: u32, value: u64) -> Self {
        Self::new(id, value.to_be_bytes())
    }

    pub(super) fn master(id: u32, children: &[Self]) -> Self {
        Self::new(
            id,
            children.iter().flat_map(Self::bytes).collect::<Vec<_>>(),
        )
    }

    pub(super) fn bytes(&self) -> Vec<u8> {
        let width = (32 - self.id.leading_zeros()).div_ceil(8) as usize;
        let mut bytes = self.id.to_be_bytes()[4 - width..].to_vec();
        bytes.extend(if self.unknown {
            vec![0xff]
        } else {
            size(self.data.len() as u64)
        });
        bytes.extend_from_slice(&self.data);
        bytes
    }
}

pub(super) fn size(value: u64) -> Vec<u8> {
    let width = (1..=8)
        .find(|width| value < (1_u64 << (width * 7)) - 1)
        .unwrap();
    let encoded = (value | (1 << (width * 7))).to_be_bytes();
    encoded[8 - width..].to_vec()
}

pub(super) fn replace(fields: &mut [Field], id: u32, data: impl Into<Vec<u8>>) {
    fields.iter_mut().find(|field| field.id == id).unwrap().data = data.into();
}

#[derive(Clone)]
pub(super) struct Document {
    pub(super) head: Vec<u8>,
    pub(super) packets: Vec<Vec<u8>>,
    pub(super) header: Vec<Field>,
    pub(super) info: Vec<Field>,
    pub(super) track: Vec<Field>,
    pub(super) audio: Vec<Field>,
    pub(super) clusters: Vec<Field>,
    pub(super) unknown_segment: bool,
    pub(super) extra_segment: Vec<Field>,
}

impl Document {
    pub(super) fn tone(channels: u16) -> Self {
        let encoded = if channels == 1 {
            include_str!("../../../tests/fixtures/media/test-0.4s-opus.ogg.base64")
        } else {
            include_str!("../../../tests/fixtures/media/test-0.4s-opus-stereo.ogg.base64")
        };
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded.split_whitespace().collect::<String>())
            .unwrap();
        let mut reader = PacketReader::new(Cursor::new(bytes));
        let head = reader.read_packet().unwrap().unwrap().data;
        reader.read_packet().unwrap().unwrap(); // Comments have no WebM packet.
        let mut packets = Vec::new();
        let mut end = 0;
        while let Some(packet) = reader.read_packet().unwrap() {
            if packet.last_in_stream() {
                end = packet.absgp_page();
            }
            packets.push(packet.data);
        }
        let pre_skip = u16::from_le_bytes(head[10..12].try_into().unwrap());
        let padding = packets.len() as u64 * 960 - end;
        let mut blocks = Vec::new();
        for (index, packet) in packets.iter().enumerate() {
            let last = index + 1 == packets.len();
            blocks.push(block(
                packet,
                (index * 20) as i16,
                (last && padding > 0).then_some((padding * 1_000_000_000 / 48_000) as i64),
            ));
        }
        Self {
            head,
            packets,
            header: vec![
                Field::uint(0x4286, 1),
                Field::uint(0x42f7, 1),
                Field::uint(0x42f2, 4),
                Field::uint(0x42f3, 8),
                Field::new(0x4282, b"webm"),
                Field::uint(0x4287, 4),
                Field::uint(0x4285, 2),
            ],
            info: vec![
                Field::uint(0x2ad7b1, 1_000_000),
                Field::new(0x4d80, b"Breeze test"),
                Field::new(0x5741, b"Breeze test"),
            ],
            track: vec![
                Field::uint(0xd7, 1),
                Field::uint(0x73c5, 71),
                Field::uint(0x83, 2),
                Field::new(0x86, b"A_OPUS"),
                Field::uint(0x56aa, u64::from(pre_skip) * 1_000_000_000 / 48_000),
                Field::uint(0x56bb, 80_000_000),
            ],
            audio: vec![
                Field::uint(0x9f, u64::from(channels)),
                Field::new(0xb5, 48_000_f64.to_be_bytes()),
            ],
            clusters: vec![cluster(0, &blocks)],
            unknown_segment: false,
            extra_segment: Vec::new(),
        }
    }

    pub(super) fn pre_skip(&mut self, frames: u16) {
        self.head[10..12].copy_from_slice(&frames.to_le_bytes());
        replace(
            &mut self.track,
            0x56aa,
            (u64::from(frames) * 1_000_000_000 / 48_000).to_be_bytes(),
        );
    }

    pub(super) fn bytes(&self) -> Vec<u8> {
        let mut track = self.track.clone();
        track.push(Field::new(0x63a2, self.head.clone()));
        track.push(Field::master(0xe1, &self.audio));
        let mut segment = vec![
            Field::master(0x1549a966, &self.info),
            Field::master(0x1654ae6b, &[Field::master(0xae, &track)]),
        ];
        segment.extend(self.clusters.clone());
        segment.extend(self.extra_segment.clone());
        let mut bytes = Field::master(0x1a45dfa3, &self.header).bytes();
        if self.unknown_segment {
            bytes.extend([0x18, 0x53, 0x80, 0x67, 0xff]);
            bytes.extend(segment.iter().flat_map(Field::bytes));
        } else {
            bytes.extend(Field::master(0x18538067, &segment).bytes());
        }
        bytes
    }
}

pub(super) fn cluster(timestamp: u64, blocks: &[Field]) -> Field {
    let mut children = vec![Field::uint(0xe7, timestamp)];
    children.extend_from_slice(blocks);
    Field::master(0x1f43b675, &children)
}

pub(super) fn payload(packet: &[u8], relative: i16, flags: u8) -> Vec<u8> {
    let mut bytes = vec![0x81];
    bytes.extend(relative.to_be_bytes());
    bytes.push(flags);
    bytes.extend_from_slice(packet);
    bytes
}

pub(super) fn block(packet: &[u8], relative: i16, padding: Option<i64>) -> Field {
    match padding {
        None => Field::new(0xa3, payload(packet, relative, 0x80)),
        Some(padding) => Field::master(
            0xa0,
            &[
                Field::new(0xa1, payload(packet, relative, 0)),
                Field::new(0x75a2, padding.to_be_bytes()),
            ],
        ),
    }
}

pub(super) fn open(bytes: Vec<u8>, limits: crate::opus_audio::Limits) -> Result<Stream, String> {
    Stream::open(
        bytes.into(),
        limits,
        None,
        Instant::now() + Duration::from_secs(5),
    )
}

pub(super) fn decode(bytes: Vec<u8>) -> (u16, u64, Vec<f32>) {
    let mut stream = open(bytes, crate::opus_audio::Limits::default()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut samples = Vec::new();
    while let Some(chunk) = stream.next_pcm(None, deadline).unwrap() {
        assert!(!chunk.is_empty());
        assert!(chunk.len() <= 5_760 * usize::from(stream.channels()));
        assert!(chunk.iter().all(|sample| sample.is_finite()));
        samples.extend(chunk);
    }
    assert_eq!(
        samples.len() as u64,
        stream.frames() * u64::from(stream.channels())
    );
    (stream.channels(), stream.frames(), samples)
}

pub(super) fn rejects(bytes: Vec<u8>, diagnostic: &str) {
    match open(bytes, crate::opus_audio::Limits::default()) {
        Ok(_) => panic!("unexpectedly admitted {diagnostic}"),
        Err(error) => assert!(error.contains(diagnostic), "expected {diagnostic}: {error}"),
    }
}
