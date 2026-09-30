use super::*;

#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    input_frames: u64,
    pending: Vec<i16>,
    packets: usize,
    encoded_frames: u64,
    encoded_bytes: usize,
}

fn snapshot(session: &Session) -> Snapshot {
    let encoder = session.encoder.as_ref().unwrap();
    Snapshot {
        input_frames: encoder.input_frames,
        pending: encoder.pending.clone(),
        packets: encoder.packets,
        encoded_frames: encoder.encoded_frames,
        encoded_bytes: encoder.encoded_bytes,
    }
}

fn exercise(bitrate: u32, expected_error: &str) {
    let mut session = Session::new(7, bitrate, true);
    let mut output = session.append(48_000, 1, &pcm(48_000, 1, 960, 0)).unwrap();
    let mut accepted_frames = 960_u64;
    let mut exhausted = false;
    for _ in 1..=MAX_PACKETS {
        let before = snapshot(&session);
        let data = pcm(48_000, 1, 960, accepted_frames as usize);
        match session.append(48_000, 1, &data) {
            Ok(bytes) => {
                output.extend(bytes);
                accepted_frames += 960;
                assert!(session.encoder.as_ref().unwrap().writer.inner().is_empty());
            }
            Err(error) => {
                assert!(error.contains(expected_error), "{error}");
                assert_eq!(snapshot(&session), before);
                assert!(session.blocked);
                assert!(!session.failed);
                assert!(session.append(48_000, 1, &data).is_err());
                output.extend(
                    session
                        .finish()
                        .expect("reserved EOS must seal the admitted PCM"),
                );
                exhausted = true;
                break;
            }
        }
    }
    assert!(
        exhausted,
        "recording must reach the real configured boundary"
    );
    assert!(output.len() <= crate::limits::MAX_MEDIA_ENCODED_QUEUE_BYTES);
    let mut stream = Stream::open(
        Arc::from(output),
        Limits::default(),
        None,
        Instant::now() + Duration::from_secs(5),
    )
    .unwrap();
    assert_eq!(stream.channels(), 1);
    assert_eq!(stream.frames(), accepted_frames);
    let mut presented = 0_u64;
    while let Some(chunk) = stream
        .next_pcm(None, Instant::now() + Duration::from_secs(5))
        .unwrap()
    {
        assert!(!chunk.is_empty());
        assert!(chunk.iter().all(|sample| sample.is_finite()));
        presented += chunk.len() as u64;
    }
    assert_eq!(presented, accepted_frames);
}

#[test]
fn actual_low_bitrate_packets_reach_the_packet_limit_and_seal_valid_eos() {
    exercise(64_000, "packet or raw duration limit");
}

#[test]
fn actual_high_bitrate_packets_reach_the_complete_file_byte_limit_and_seal_valid_eos() {
    exercise(500_000, "8 MiB complete-file limit");
}
