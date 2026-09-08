//! Bounded page metadata and opt-in selector diagnostics for hidden captures.

use better_web_browser::limits::{
    MAX_PAGE_DIAGNOSTIC_SELECTOR_BYTES, MAX_PAGE_DIAGNOSTIC_SELECTORS,
};
use better_web_browser::renderer_protocol::MediaRuntimeReport;

use crate::windows_app::json_string;

const MAX_DIAGNOSTIC_TITLE_BYTES: usize = 512;

#[derive(Default)]
pub(in crate::windows_app) struct PageTitles {
    document_title: Option<String>,
    document_title_truncated: bool,
}

impl PageTitles {
    /// Keep the accepted renderer's DOM-derived title separate from browser chrome so captures
    /// can distinguish a page that never changed its title from a tab update/painting failure.
    pub(in crate::windows_app) fn record_document_title(&mut self, title: &str) {
        let (title, truncated) = bounded_title(title);
        self.document_title = Some(title.to_string());
        self.document_title_truncated = truncated;
    }

    pub(super) fn json(&self, browser_title: &str) -> serde_json::Value {
        let (browser_title, browser_title_truncated) = bounded_title(browser_title);
        serde_json::json!({
            "document_title": self.document_title,
            "document_title_truncated": self.document_title_truncated,
            "browser_title": browser_title,
            "browser_title_truncated": browser_title_truncated,
        })
    }
}

fn bounded_title(title: &str) -> (&str, bool) {
    better_web_browser::limits::bounded_utf8_prefix(title, MAX_DIAGNOSTIC_TITLE_BYTES)
}

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
            "\"frames_submitted\":{},\"dropped_frames\":{},",
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
        media.frames_submitted,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_keeps_renderer_title_independent_from_browser_tab_title() {
        let mut titles = PageTitles::default();
        assert_eq!(
            titles.json("Loading")["document_title"],
            serde_json::Value::Null
        );

        titles.record_document_title("First document");
        titles.record_document_title("Second \"document\"\n☃");
        let serialized = titles.json("First document").to_string();
        let report: serde_json::Value = serde_json::from_str(&serialized).unwrap();
        assert_eq!(report["document_title"], "Second \"document\"\n☃");
        assert_eq!(report["browser_title"], "First document");
        assert_eq!(report["document_title_truncated"], false);
        assert_eq!(report["browser_title_truncated"], false);
    }

    #[test]
    fn report_bounds_both_titles_at_utf8_boundaries_and_marks_truncation() {
        let long_title = format!("{}☃tail", "a".repeat(MAX_DIAGNOSTIC_TITLE_BYTES - 1));
        let mut titles = PageTitles::default();
        titles.record_document_title(&long_title);
        let report = titles.json(&long_title);
        for field in ["document_title", "browser_title"] {
            assert_eq!(report[field], "a".repeat(MAX_DIAGNOSTIC_TITLE_BYTES - 1));
        }
        assert_eq!(report["document_title_truncated"], true);
        assert_eq!(report["browser_title_truncated"], true);

        titles.record_document_title("");
        let report = titles.json(&"b".repeat(MAX_DIAGNOSTIC_TITLE_BYTES));
        assert_eq!(report["document_title"], "");
        assert_eq!(report["document_title_truncated"], false);
        assert_eq!(report["browser_title_truncated"], false);
    }
}
