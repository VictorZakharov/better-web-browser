//! Test-only extraction from existing synthetic fixtures. No page-facing demux
//! API is added: runtime chunks still require the registered elementary format.
use base64::Engine;
use std::io::Cursor;
use symphonia::core::formats::{FormatOptions, probe::Hint};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

pub(in crate::engine::script) fn fixture(extension: &str) -> Vec<u8> {
    let text = match extension {
        "mp3" => include_str!("../../../../tests/fixtures/media/test-0.4s-tone.mp3.base64"),
        "aac" => include_str!("../../../../tests/fixtures/media/test-0.4s-tone.aac.base64"),
        "flac" => include_str!("../../../../tests/fixtures/media/test-1s-audio.flac.base64"),
        "ogg" => include_str!("../../../../tests/fixtures/media/test-2s-audio.ogg.base64"),
        _ => panic!("fixture not registered"),
    };
    let encoded: String = text
        .chars()
        .filter(|ch| !ch.is_ascii_whitespace())
        .collect();
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .unwrap()
}

pub(in crate::engine::script) fn packets(extension: &str) -> Vec<Vec<u8>> {
    let stream = MediaSourceStream::new(
        Box::new(Cursor::new(fixture(extension))),
        Default::default(),
    );
    let mut hint = Hint::new();
    hint.with_extension(extension);
    let mut reader = symphonia::default::get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .unwrap();
    let mut packets = Vec::new();
    while let Some(packet) = reader.next_packet().unwrap() {
        packets.push(packet.data.into_vec());
    }
    packets
}

pub(in crate::engine::script) fn adts_packets() -> Vec<Vec<u8>> {
    let bytes = fixture("aac");
    let mut packets = Vec::new();
    let mut remaining = bytes.as_slice();
    while !remaining.is_empty() {
        let frame = crate::encoded_audio::adts::frame(remaining).unwrap();
        packets.push(remaining[..frame.bytes].to_vec());
        remaining = &remaining[frame.bytes..];
    }
    packets
}

pub(in crate::engine::script) fn flac_description() -> Vec<u8> {
    let mut bytes = fixture("flac")[..42].to_vec();
    bytes[4] = 128;
    bytes
}

pub(in crate::engine::script) fn javascript_cases() -> String {
    let cases = serde_json::json!([
        {"codec":"mp3", "description":null, "packets":packets("mp3"), "minimumFrames":17640},
        {"codec":"mp4a.69", "description":null, "packets":packets("mp3"), "minimumFrames":17640},
        {"codec":"mp4a.6B", "description":null, "packets":packets("mp3"), "minimumFrames":17640},
        {"codec":"mp4a.40.2", "description":null, "packets":adts_packets(), "minimumFrames":17640},
        {"codec":"mp4a.40.02", "description":[18,8], "packets":packets("aac"), "minimumFrames":17640},
        {"codec":"mp4a.67", "description":[18,8], "packets":packets("aac"), "minimumFrames":17640},
        {"codec":"flac", "description":flac_description(), "packets":packets("flac"), "minimumFrames":44100},
        {"codec":"vorbis", "description":vorbis_description(), "packets":packets("ogg"), "minimumFrames":88200,"primingPackets":1},
    ]);
    format!("const elementaryCases={cases};")
}

pub(in crate::engine::script) fn vorbis_description() -> Vec<u8> {
    let mut reader = ogg::reading::PacketReader::new(Cursor::new(fixture("ogg")));
    let headers: Vec<Vec<u8>> = (0..3)
        .map(|_| reader.read_packet().unwrap().unwrap().data)
        .collect();
    let mut description = vec![2];
    for header in &headers[..2] {
        let mut length = header.len();
        while length >= 255 {
            description.push(255);
            length -= 255;
        }
        description.push(length as u8);
    }
    for header in headers {
        description.extend(header);
    }
    description
}

#[test]
#[ignore = "exports synthetic packet cases for the offline browser comparison"]
fn export_elementary_audio_cases() {
    let path = std::env::var_os("BREEZE_AUDIO_CASES_OUTPUT")
        .expect("set BREEZE_AUDIO_CASES_OUTPUT to a task file on G:");
    let path = std::path::PathBuf::from(path);
    assert!(path.is_absolute());
    let display = path.to_string_lossy();
    assert!(display.starts_with("G:\\") || display.starts_with("G:/"));
    assert_eq!(path.extension().unwrap(), "js");
    let parent = path.parent().unwrap();
    std::fs::create_dir_all(parent).unwrap();
    std::fs::write(path, javascript_cases()).unwrap();
}
