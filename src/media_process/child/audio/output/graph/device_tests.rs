use super::*;

#[test]
#[ignore = "requires the Windows user's audio endpoint; output volume is zero"]
fn real_graph_voice_primes_buffers_and_reports_consumption_and_starvation() {
    let device = match XAudioOutput::new(48_000, 2) {
        Ok(device) => device,
        Err(DeviceOutputError::EndpointUnavailable) => panic!("audio endpoint unavailable"),
        Err(DeviceOutputError::Fatal(error)) => panic!("{error}"),
    };
    unsafe { device.source.SetVolume(0.0, XAUDIO2_COMMIT_NOW) }.unwrap();
    let mut output = AudioOutput::Device(device);
    for buffers in 1..=3 {
        assert!(output.queue_graph_pcm(vec![0; 896 * 4], 2).unwrap());
        let health = output.graph_health().unwrap();
        assert_eq!(health.submitted_frames, buffers * 896);
        assert_eq!(health.playing, buffers >= 3);
        assert_eq!(health.underruns, 0);
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    while output.graph_health().unwrap().played_frames < 2688 {
        assert!(
            Instant::now() < deadline,
            "device did not consume the primed PCM"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(output.queue_graph_pcm(vec![0; 896 * 4], 2).unwrap());
    let recovered = output.graph_health().unwrap();
    assert_eq!(recovered.submitted_frames, 3584);
    assert_eq!(recovered.underruns, 1);
    assert!(recovered.playing);
}
