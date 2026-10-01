//! Metadata cannot silently change the admitted codec presentation contract.

use super::fixture_builder::*;

#[test]
fn neutral_optional_transforms_preserve_every_decoded_sample() {
    for channels in [1, 2] {
        let base = Document::tone(channels);
        let expected = decode(base.bytes()).2;
        for empty in [false, true] {
            let mut document = base.clone();
            document.track.extend([
                Field::new(
                    0x23314f,
                    if empty {
                        vec![]
                    } else {
                        1_f64.to_be_bytes().to_vec()
                    },
                ),
                Field::new(0x537f, if empty { vec![] } else { vec![0] }),
                Field::new(0x9c, if empty { vec![] } else { vec![1] }),
            ]);
            document
                .audio
                .push(Field::new(0x52f1, if empty { vec![] } else { vec![0] }));
            document
                .audio
                .push(Field::new(0x78b5, 48_000_f64.to_be_bytes()));
            assert_eq!(decode(document.bytes()).2, expected);
        }
    }
}

#[test]
fn unsupported_timestamp_and_emphasis_transforms_are_not_ignored() {
    let base = Document::tone(1);
    for scale in [0_f64, -1.0, 0.5, 2.0, f64::INFINITY, f64::NAN] {
        let mut document = base.clone();
        document
            .track
            .push(Field::new(0x23314f, scale.to_be_bytes()));
        assert!(open(document.bytes(), crate::opus_audio::Limits::default()).is_err());
    }
    for offset in [1_i64, -1, 20_000_000, i64::MIN, i64::MAX] {
        let mut document = base.clone();
        document
            .track
            .push(Field::new(0x537f, offset.to_be_bytes()));
        rejects(document.bytes(), "TrackOffset");
    }
    for emphasis in [1_u64, 2, 3, 4, 16, u64::MAX] {
        let mut document = base.clone();
        document.audio.push(Field::uint(0x52f1, emphasis));
        rejects(document.bytes(), "Emphasis");
    }
    for rate in [0_f64, -1.0, 44_100.0, 96_000.0, f64::INFINITY, f64::NAN] {
        let mut document = base.clone();
        document.audio.push(Field::new(0x78b5, rate.to_be_bytes()));
        rejects(document.bytes(), "output sampling frequency");
    }
}

#[test]
fn duplicate_optional_track_and_audio_values_do_not_override_earlier_values() {
    let base = Document::tone(1);
    for field in [
        Field::uint(0x9c, 1),
        Field::new(0x23314f, 1_f64.to_be_bytes()),
        Field::new(0x537f, [0]),
    ] {
        let mut document = base.clone();
        document.track.extend([field.clone(), field]);
        rejects(document.bytes(), "repeat");
    }
    for field in [
        Field::uint(0x52f1, 0),
        Field::new(0x78b5, 48_000_f64.to_be_bytes()),
    ] {
        let mut document = base.clone();
        document.audio.extend([field.clone(), field]);
        rejects(document.bytes(), "repeat");
    }
}

#[test]
fn disabled_track_lacing_accepts_unlaced_packets_but_rejects_each_lace_flag() {
    let base = Document::tone(1);
    let expected = decode(base.bytes()).2;
    let mut disabled = base.clone();
    disabled.track.push(Field::uint(0x9c, 0));
    assert_eq!(decode(disabled.bytes()).2, expected);
    for flag in [2, 4, 6] {
        // A one-frame lace is deliberately used here. Counting packets alone
        // would miss the contradictory flag because frames would still be one.
        let mut bytes = payload(&base.packets[0], 0, flag | 0x80);
        bytes.insert(4, 0);
        let mut document = disabled.clone();
        document.clusters = vec![cluster(0, &[Field::new(0xa3, bytes)])];
        rejects(document.bytes(), "FlagLacing");
    }
    for value in [2_u64, 255, u64::MAX] {
        let mut document = base.clone();
        document.track.push(Field::uint(0x9c, value));
        rejects(document.bytes(), "FlagLacing");
    }
}

#[test]
fn optional_metadata_scalar_widths_are_bounded_before_demuxing() {
    let base = Document::tone(1);
    for width in [1, 2, 3, 5, 6, 7, 9] {
        for id in [0x23314f, 0x78b5] {
            let mut document = base.clone();
            let fields = if id == 0x23314f {
                &mut document.track
            } else {
                &mut document.audio
            };
            fields.push(Field::new(id, vec![0; width]));
            assert!(open(document.bytes(), crate::opus_audio::Limits::default()).is_err());
        }
    }
    for id in [0x9c, 0x537f, 0x52f1] {
        let mut document = base.clone();
        let fields = if id == 0x52f1 {
            &mut document.audio
        } else {
            &mut document.track
        };
        fields.push(Field::new(id, vec![0; 9]));
        assert!(open(document.bytes(), crate::opus_audio::Limits::default()).is_err());
    }
}
