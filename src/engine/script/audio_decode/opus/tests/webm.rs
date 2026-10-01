//! Real complete-file WebM enters the existing bounded document-job machinery.

use super::*;
use crate::engine::script::audio_decode::{AudioDecodes, Poll};
use crate::webm_opus::mux::Mux;
use ogg::PacketReader;
use std::io::Cursor;

fn remux(channels: usize, repeat: Option<usize>) -> (Vec<u8>, u64) {
    let bytes = fixtures()
        .into_iter()
        .find(|(_, count)| *count == channels)
        .unwrap()
        .0;
    let mut reader = PacketReader::new(Cursor::new(bytes));
    let head = reader.read_packet().unwrap().unwrap().data;
    reader.read_packet().unwrap().unwrap();
    let mut packets = Vec::new();
    let mut end = 0;
    while let Some(packet) = reader.read_packet().unwrap() {
        if packet.last_in_stream() {
            end = packet.absgp_page();
        }
        packets.push(packet.data);
    }
    if let Some(count) = repeat {
        packets = vec![packets[0].clone(); count];
        end = count as u64 * 960;
    }
    let pre_skip = u64::from(u16::from_le_bytes(head[10..12].try_into().unwrap()));
    let mut mux = Mux::new(&head, 1).unwrap();
    for (index, packet) in packets.iter().enumerate() {
        mux.packet(packet, (index + 1 == packets.len()).then_some(end))
            .unwrap();
    }
    (mux.drain().unwrap(), end - pre_skip)
}

fn collect(jobs: &mut AudioDecodes, id: u32) -> (Vec<Vec<f32>>, f64) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut collected = Vec::<Vec<f32>>::new();
    loop {
        match jobs.poll(id) {
            Poll::Pending => {
                assert!(Instant::now() < deadline, "WebM document job timed out");
                std::thread::sleep(Duration::from_millis(1));
            }
            Poll::Data {
                channels,
                frames,
                sample_rate,
                channel,
                offset,
                bytes,
                done,
            } => {
                if collected.is_empty() {
                    collected.resize_with(channels, Vec::new);
                }
                assert_eq!(channels, collected.len());
                assert!(channel < channels);
                assert_eq!(offset, collected[channel].len());
                assert!(bytes.len() <= 16_384 * 4);
                assert!(bytes.len().is_multiple_of(4));
                collected[channel].extend(bytes.chunks_exact(4).map(|sample| {
                    let value = f32::from_le_bytes(sample.try_into().unwrap());
                    assert!(value.is_finite());
                    value
                }));
                if done {
                    assert!(collected.iter().all(|samples| samples.len() == frames));
                    assert!(
                        matches!(jobs.poll(id), Poll::Error(_)),
                        "completed job must retire"
                    );
                    return (collected, sample_rate);
                }
            }
            Poll::Error(error) => panic!("WebM job error: {error}"),
        }
    }
}

#[test]
fn webm_jobs_return_actual_planar_pcm_with_context_rate_conversion_and_owned_offsets() {
    let cancelled = AtomicBool::new(false);
    let mut jobs = AudioDecodes::default();
    let mut expected = Vec::new();
    for (channels, rate) in [(1, 96_000.0), (2, 44_100.0)] {
        let (bytes, frames) = remux(channels, None);
        assert_eq!(frames, 19_200);
        let decoded = decode(&bytes, rate, &cancelled).unwrap();
        let id = jobs.start(bytes, rate).unwrap();
        expected.push((id, decoded.channels, rate));
    }
    assert!(
        jobs.start(vec![], 48_000.0).is_err(),
        "both document slots are owned"
    );
    for (id, pcm, rate) in expected {
        let (actual, actual_rate) = collect(&mut jobs, id);
        assert_eq!(actual_rate, rate);
        assert_eq!(actual, pcm);
    }
}

#[test]
fn malformed_webm_job_error_retires_its_slot_without_damaging_a_valid_sibling() {
    let (bytes, _) = remux(2, None);
    let expected = decode(&bytes, 48_000.0, &AtomicBool::new(false))
        .unwrap()
        .channels;
    let mut jobs = AudioDecodes::default();
    let invalid = jobs
        .start(bytes[..bytes.len() - 1].to_vec(), 48_000.0)
        .unwrap();
    let valid = jobs.start(bytes.clone(), 48_000.0).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match jobs.poll(invalid) {
            Poll::Pending => {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(1));
            }
            Poll::Error(error) => {
                assert!(!error.is_empty());
                break;
            }
            Poll::Data { .. } => panic!("truncated WebM returned successful partial PCM"),
        }
    }
    assert!(matches!(jobs.poll(invalid), Poll::Error(_)));
    let replacement = jobs
        .start(bytes, 48_000.0)
        .expect("failed slot is reusable");
    assert_eq!(collect(&mut jobs, valid).0, expected);
    assert_eq!(collect(&mut jobs, replacement).0, expected);
}

#[test]
fn webm_cancelled_job_has_no_late_result_and_does_not_reuse_job_identity() {
    let (bytes, _) = remux(2, None);
    let mut jobs = AudioDecodes::default();
    for _ in 0..8 {
        let id = jobs.start(bytes.clone(), 48_000.0).unwrap();
        jobs.cancel(id);
        assert!(matches!(jobs.poll(id), Poll::Error(_)));
        let next = jobs.start(bytes.clone(), 48_000.0).unwrap();
        assert!(next > id);
        assert_eq!(collect(&mut jobs, next).0.len(), 2);
        assert!(matches!(jobs.poll(id), Poll::Error(_)));
    }
}

#[test]
fn streaming_webm_can_exceed_audio_buffer_budget_without_whole_file_pcm_storage() {
    let (bytes, frames) = remux(1, Some(5_000));
    assert!(bytes.len() < 8 * 1024 * 1024);
    assert!(frames * 4 > 16 * 1024 * 1024);
    assert!(decode(&bytes, 48_000.0, &AtomicBool::new(false)).is_err());
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut stream = Stream::open(bytes.into(), Limits::default(), None, deadline).unwrap();
    assert_eq!(stream.frames(), frames);
    let mut received = 0_u64;
    while let Some(chunk) = stream.next_pcm(None, deadline).unwrap() {
        assert!(chunk.len() <= 5_760);
        received += chunk.len() as u64;
    }
    assert_eq!(received, frames);
}

#[test]
fn encoded_header_and_resampling_limits_are_checked_before_successful_pcm() {
    let (bytes, _) = remux(2, None);
    for rate in [f64::NAN, f64::INFINITY, 0.0, -1.0] {
        assert!(AudioDecodes::default().start(bytes.clone(), rate).is_err());
    }
    assert!(
        decode(&bytes, 48_000.0, &AtomicBool::new(true))
            .err()
            .unwrap()
            .contains("cancel")
    );
    let mut chained = bytes.clone();
    chained.extend_from_slice(&bytes);
    assert!(decode(&chained, 48_000.0, &AtomicBool::new(false)).is_err());
    let mut huge = bytes;
    huge.resize(MAX_ENCODED_BYTES + 1, 0);
    assert!(decode(&huge, 48_000.0, &AtomicBool::new(false)).is_err());
}
