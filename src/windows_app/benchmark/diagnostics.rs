//! Validation for opt-in selector diagnostics collected by the isolated renderer.

use better_web_browser::limits::{
    MAX_PAGE_DIAGNOSTIC_SELECTOR_BYTES, MAX_PAGE_DIAGNOSTIC_SELECTORS,
};
use better_web_browser::renderer_protocol::MediaRuntimeReport;

use crate::windows_app::json_string;

pub(super) fn validate_selector_count(selectors: &[String]) -> Result<(), String> {
    if selectors.len() > MAX_PAGE_DIAGNOSTIC_SELECTORS {
        return Err(format!(
            "at most {MAX_PAGE_DIAGNOSTIC_SELECTORS} --diagnostic-selector options are allowed"
        ));
    }
    if selectors
        .iter()
        .any(|selector| selector.len() > MAX_PAGE_DIAGNOSTIC_SELECTOR_BYTES)
    {
        return Err(format!(
            "--diagnostic-selector values cannot exceed {MAX_PAGE_DIAGNOSTIC_SELECTOR_BYTES} UTF-8 bytes"
        ));
    }
    Ok(())
}

pub(super) fn media_runtime_json(media: Option<&MediaRuntimeReport>) -> String {
    let Some(media) = media else {
        return "null".into();
    };
    format!(
        concat!(
            "{{\"active\":{},\"playing\":{},\"ended\":{},",
            "\"current_time_seconds\":{:.3},\"duration_seconds\":{:.3},",
            "\"backend\":{},\"mime_type\":{},\"video_codec\":{},\"audio_codec\":{},",
            "\"encoded_queue_bytes\":{},\"encoded_queue_limit_bytes\":{},",
            "\"decoded_frame_queue_depth\":{},\"decoded_frame_queue_limit\":{},",
            "\"frames_presented\":{},\"dropped_frames\":{},",
            "\"width\":{},\"height\":{},\"failure\":{}}}"
        ),
        media.active,
        media.playing,
        media.ended,
        media.current_time_100ns as f64 / 10_000_000.0,
        media.duration_100ns as f64 / 10_000_000.0,
        json_string(&media.backend),
        json_string(&media.mime_type),
        json_string(&media.video_codec),
        json_string(&media.audio_codec),
        media.encoded_queue_bytes,
        media.encoded_queue_limit_bytes,
        media.decoded_frame_queue_depth,
        media.decoded_frame_queue_limit,
        media.frames_presented,
        media.dropped_frames,
        media.width,
        media.height,
        media
            .failure
            .as_deref()
            .map(json_string)
            .unwrap_or_else(|| "null".into()),
    )
}
