use super::*;
use base64::Engine as _;
use ogg::writing::{PacketWriteEndInfo, PacketWriter};
use std::time::Duration;

mod admission;
mod presentation;

fn fixture(stereo: bool) -> Arc<[u8]> {
    let source = if stereo {
        include_str!("../../tests/fixtures/media/test-0.4s-opus-stereo.ogg.base64")
    } else {
        include_str!("../../tests/fixtures/media/test-0.4s-opus.ogg.base64")
    };
    base64::engine::general_purpose::STANDARD
        .decode(source.split_whitespace().collect::<String>())
        .unwrap()
        .into()
}

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}

fn open(bytes: Arc<[u8]>) -> Result<Stream, String> {
    Stream::open(bytes, Limits::default(), None, deadline())
}

fn decode(bytes: Arc<[u8]>) -> (u16, u64, Vec<f32>) {
    let mut stream = open(bytes).unwrap();
    let channels = stream.channels();
    let frames = stream.frames();
    let mut samples = Vec::new();
    while let Some(chunk) = stream.next_pcm(None, deadline()).unwrap() {
        assert!(!chunk.is_empty());
        assert!(chunk.iter().all(|sample| sample.is_finite()));
        samples.extend(chunk);
    }
    assert_eq!(samples.len() as u64, frames * u64::from(channels));
    assert!(stream.next_pcm(None, deadline()).unwrap().is_none());
    (channels, frames, samples)
}

fn packets(bytes: Arc<[u8]>) -> Vec<Vec<u8>> {
    let mut reader = PacketReader::new(Cursor::new(bytes));
    let mut packets = Vec::new();
    while let Some(packet) = reader.read_packet().unwrap() {
        packets.push(packet.data);
    }
    packets
}

/// Remux owned packets through the existing Ogg writer, never hand-build CRCs.
fn remux(
    mut packets: Vec<Vec<u8>>,
    group: usize,
    origin: u64,
    end: u64,
    mut change: impl FnMut(usize, &mut Vec<u8>),
) -> Arc<[u8]> {
    let mut writer = PacketWriter::new(Vec::new());
    let count = packets.len();
    let mut raw = 0_u64;
    for (index, data) in packets.iter_mut().enumerate() {
        change(index, data);
        let (ending, granule) = if index < 2 {
            (PacketWriteEndInfo::EndPage, 0)
        } else {
            // Malformed packet tests retain a plausible timeline; production
            // admission, not this writer helper, decides packet validity.
            raw += opus::packet::get_nb_samples(data, SAMPLE_RATE).unwrap_or(960) as u64;
            if index + 1 == count {
                (PacketWriteEndInfo::EndStream, origin + end)
            } else if (index - 1) % group == 0 {
                (PacketWriteEndInfo::EndPage, origin + raw)
            } else {
                (PacketWriteEndInfo::NormalPacket, origin + raw)
            }
        };
        writer
            .write_packet(data.clone().into_boxed_slice(), 7, ending, granule)
            .unwrap();
    }
    writer.into_inner().into()
}
