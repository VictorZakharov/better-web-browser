use super::*;
use base64::Engine as _;
use ogg::writing::{PacketWriteEndInfo, PacketWriter};

fn tone() -> Vec<u8> {
    let source = include_str!("../../tests/fixtures/media/test-0.4s-tone.oga.base64");
    base64::engine::general_purpose::STANDARD
        .decode(source.split_whitespace().collect::<String>())
        .unwrap()
}

fn admit(bytes: &[u8]) -> Result<(), String> {
    crate::encoded_audio::validate(bytes, crate::encoded_audio::Kind::FlacOgg)
}

fn remux(mut change: impl FnMut(usize, &mut Vec<u8>, &mut u64)) -> Vec<u8> {
    let good = tone();
    let mut reader = PacketReader::new(Cursor::new(good));
    let mut writer = PacketWriter::new(Vec::new());
    let mut index = 0;
    while let Some(packet) = reader.read_packet().unwrap() {
        let mut granule = packet.absgp_page();
        let end = if packet.last_in_stream() {
            PacketWriteEndInfo::EndStream
        } else if index < 2 {
            PacketWriteEndInfo::EndPage
        } else {
            PacketWriteEndInfo::NormalPacket
        };
        let mut data = packet.data;
        change(index, &mut data, &mut granule);
        writer
            .write_packet(data.into_boxed_slice(), 7, end, granule)
            .unwrap();
        index += 1;
    }
    writer.into_inner()
}

#[test]
fn owned_flac_ogg_has_valid_crc_mapping_metadata_and_eos() {
    assert!(sniff(&tone()));
    assert!(admit(&tone()).is_ok());
    assert!(admit(&remux(|_, _, _| {})).is_ok());
}

#[test]
fn rejects_every_truncation_crc_corruption_and_trailing_garbage() {
    let good = tone();
    for end in 0..good.len() {
        assert!(
            admit(&good[..end]).is_err(),
            "accepted truncated byte {end}"
        );
    }
    let mut corrupt = good.clone();
    *corrupt.last_mut().unwrap() ^= 1;
    assert!(admit(&corrupt).is_err());
    let mut trailing = good;
    trailing.extend_from_slice(b"junk");
    assert!(admit(&trailing).is_err());
}

#[test]
fn rejects_reserved_mapping_versions_metadata_types_and_counts() {
    for position in [5, 6, 7, 13, 16] {
        assert!(
            admit(&remux(|index, data, _| {
                if index == 0 {
                    data[position] ^= 1;
                }
            }))
            .is_err(),
            "position {position}"
        );
    }
    assert!(
        admit(&remux(|index, data, _| {
            if index == 1 {
                data[0] = 0xff;
            }
        }))
        .is_err()
    );
}

#[test]
fn eos_must_match_known_streaminfo_but_zero_total_remains_unknown() {
    assert!(
        admit(&remux(|index, data, _| {
            if index == 0 {
                data[31..35].copy_from_slice(&17_640_u32.to_be_bytes());
            }
        }))
        .is_ok()
    );
    assert!(
        admit(&remux(|index, data, _| {
            if index == 0 {
                data[31..35].copy_from_slice(&17_641_u32.to_be_bytes());
            }
        }))
        .is_err()
    );
    assert!(
        admit(&remux(|_, _, granule| {
            if *granule != 0 {
                *granule = u64::MAX;
            }
        }))
        .is_err()
    );
}

#[test]
fn rejects_chained_streams_and_oversized_packets_before_reassembly() {
    let mut chained = tone();
    chained.extend(tone());
    assert!(admit(&chained).is_err());
    let huge = remux(|index, data, _| {
        if index == 1 {
            data.resize(MAX_PACKET + 1, 0);
        }
    });
    assert!(admit(&huge).is_err());
}
