use super::*;

#[test]
fn owned_mono_and_stereo_are_real_finite_sample_exact_pcm() {
    for stereo in [false, true] {
        let (channels, frames, pcm) = decode(fixture(stereo));
        assert_eq!(channels, if stereo { 2 } else { 1 });
        assert_eq!(frames, 19_200);
        assert!(pcm.iter().any(|sample| sample.abs() > 0.05));
        if stereo {
            assert!(
                pcm.chunks_exact(2)
                    .any(|pair| (pair[0] - pair[1]).abs() > 0.05)
            );
        }
    }
}

#[test]
fn pre_skip_crosses_packets_and_eos_trim_preserves_exact_presentation_slice() {
    let owned = packets(fixture(false));
    let (_, _, baseline) = decode(fixture(false));
    let changed = remux(owned, 3, 0, 18_901, |index, data| {
        if index == 0 {
            data[10..12].copy_from_slice(&2_501_u16.to_le_bytes());
        }
    });
    let (_, frames, pcm) = decode(changed);
    assert_eq!(frames, 18_901 - 2_501);
    assert_eq!(pcm, baseline[2_501 - 312..18_901 - 312]);
}

#[test]
fn nonzero_origin_and_repacketized_page_groups_do_not_change_pcm() {
    let owned = packets(fixture(false));
    let baseline = decode(fixture(false)).2;
    for (group, origin) in [(1, 71_337), (3, 71_337), (100, 0)] {
        let (channels, frames, pcm) =
            decode(remux(owned.clone(), group, origin, 19_512, |_, _| {}));
        assert_eq!((channels, frames), (1, 19_200));
        assert_eq!(pcm, baseline);
    }
}

#[test]
fn header_gain_is_applied_once_and_input_rate_is_only_metadata() {
    let owned = packets(fixture(false));
    let baseline = decode(fixture(false)).2;
    let changed = remux(owned, 3, 0, 19_512, |index, data| {
        if index == 0 {
            data[12..16].copy_from_slice(&10_000_000_u32.to_le_bytes());
            data[16..18].copy_from_slice(&(-1_536_i16).to_le_bytes());
        }
    });
    let (_, frames, pcm) = decode(changed);
    assert_eq!(frames, 19_200);
    let gain = 10_f32.powf(-6.0 / 20.0);
    for (actual, expected) in pcm.iter().zip(baseline) {
        assert!((actual - expected * gain).abs() < 1.0e-5);
    }
}

#[test]
fn cancellation_and_deadline_fail_terminally_but_reopen_restarts_exactly() {
    let source = fixture(false);
    let baseline = decode(source.clone()).2;
    for cancelled in [false, true] {
        let mut stream = open(source.clone()).unwrap();
        assert!(stream.next_pcm(None, deadline()).unwrap().is_some());
        let flag = AtomicBool::new(cancelled);
        let expiry = if cancelled {
            deadline()
        } else {
            Instant::now()
        };
        assert!(stream.next_pcm(Some(&flag), expiry).is_err());
        flag.store(false, Ordering::Relaxed);
        assert!(stream.next_pcm(None, deadline()).is_err());
        assert_eq!(decode(source.clone()).2, baseline);
    }
}
