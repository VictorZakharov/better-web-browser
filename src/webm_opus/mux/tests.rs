use super::*;

fn recording() -> (Mux, Vec<u8>) {
    let mut head = b"OpusHead\x01\x01\x00\x00".to_vec();
    head.extend(48_000_u32.to_le_bytes());
    head.extend([0, 0, 0]);
    let mux = Mux::new(&head, 1).unwrap();
    let mut encoder =
        opus::Encoder::new(48_000, opus::Channels::Mono, opus::Application::Audio).unwrap();
    let mut packet = vec![0; 4_000];
    let len = encoder.encode(&[0; 960], &mut packet).unwrap();
    packet.truncate(len);
    (mux, packet)
}

#[test]
fn cumulative_limit_counts_headers_previously_drained_bytes_and_current_cluster() {
    let (mut mux, packet) = recording();
    let header_bytes = mux.bytes().len();
    mux.total_bytes = crate::limits::MAX_MEDIA_ENCODED_QUEUE_BYTES - header_bytes;
    assert_eq!(mux.drain().unwrap().len(), header_bytes);
    assert_eq!(
        mux.total_bytes,
        crate::limits::MAX_MEDIA_ENCODED_QUEUE_BYTES
    );
    assert!(mux.drain().unwrap().is_empty());
    mux.packet(&packet, None).unwrap();
    let pending = mux.bytes().to_vec();
    assert!(!pending.is_empty());
    assert!(mux.drain().unwrap_err().contains("cumulative"));
    assert_eq!(
        mux.bytes(),
        pending,
        "failed admission must not silently remove encoded bytes"
    );
    assert!(mux.failed);
    assert!(mux.packet(&packet, Some(1_920)).is_err());
    assert!(
        mux.drain().is_err(),
        "retry must not reset the cumulative budget"
    );
}

#[test]
fn byte_counter_overflow_is_terminal_before_writer_buffer_is_drained() {
    let (mut mux, packet) = recording();
    mux.total_bytes = usize::MAX;
    let pending = mux.bytes().to_vec();
    assert!(mux.drain().is_err());
    assert_eq!(mux.bytes(), pending);
    assert_eq!(mux.total_bytes, usize::MAX);
    assert!(mux.failed);
    assert!(mux.packet(&packet, None).is_err());
}

#[test]
fn final_cluster_remains_drainable_once_and_cannot_be_reopened() {
    let (mut mux, packet) = recording();
    let header = mux.drain().unwrap();
    mux.packet(&packet, Some(937)).unwrap();
    let terminal = mux.drain().unwrap();
    assert!(mux.finished);
    assert!(!terminal.is_empty());
    assert_eq!(mux.total_bytes, header.len() + terminal.len());
    assert!(mux.drain().unwrap().is_empty());
    let mut stream = crate::webm_opus::Stream::open(
        [header, terminal].concat().into(),
        crate::opus_audio::Limits::default(),
        None,
        std::time::Instant::now() + std::time::Duration::from_secs(5),
    )
    .unwrap();
    assert_eq!(stream.frames(), 937);
    assert_eq!(
        stream
            .next_pcm(
                None,
                std::time::Instant::now() + std::time::Duration::from_secs(5)
            )
            .unwrap()
            .unwrap()
            .len(),
        937
    );
    assert!(mux.packet(&packet, Some(960)).is_err());
}

#[test]
fn duration_limit_is_checked_before_serializing_a_cluster_or_advancing_time() {
    let (mut mux, packet) = recording();
    mux.raw_frames = 48_000 * 3_600;
    let pending = mux.bytes().to_vec();
    assert!(mux.packet(&packet, None).unwrap_err().contains("duration"));
    assert_eq!(mux.bytes(), pending);
    assert_eq!(mux.raw_frames, 48_000 * 3_600);
    assert!(mux.failed);
}
