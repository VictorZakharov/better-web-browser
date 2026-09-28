use super::*;

#[test]
fn only_a_missing_default_endpoint_selects_silent_output() {
    assert!(missing_default_audio_endpoint(windows::core::HRESULT(
        AUDIO_ENDPOINT_NOT_FOUND
    )));
    assert!(!missing_default_audio_endpoint(windows::core::HRESULT(
        0x8007_0057_u32 as i32
    )));
}

#[test]
fn live_graph_rejects_a_missing_audio_endpoint() {
    assert_eq!(
        AudioOutput::from_required_device(Err(DeviceOutputError::EndpointUnavailable))
            .err()
            .as_deref(),
        Some("default audio output endpoint is unavailable")
    );
}
