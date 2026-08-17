//! JavaScript MIME checks shared by document and worker module graphs.

// HTML delegates this legacy-compatible set to the MIME Sniffing Standard's JavaScript MIME type.
pub(super) fn is_javascript_mime_type(content_type: &str) -> bool {
    let essence = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    matches!(
        essence.as_str(),
        "application/ecmascript"
            | "application/javascript"
            | "application/x-ecmascript"
            | "application/x-javascript"
            | "text/ecmascript"
            | "text/javascript"
            | "text/javascript1.0"
            | "text/javascript1.1"
            | "text/javascript1.2"
            | "text/javascript1.3"
            | "text/javascript1.4"
            | "text/javascript1.5"
            | "text/jscript"
            | "text/livescript"
            | "text/x-ecmascript"
            | "text/x-javascript"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use better_web_browser::engine::ScriptKind;
    use better_web_browser::fetch::{Body, FetchResponse, FetchUrl, HeaderList, ResponseType};

    #[test]
    fn accepts_javascript_mime_essences_case_insensitively() {
        for content_type in [
            "text/javascript",
            "TEXT/JAVASCRIPT; charset=utf-8",
            " application/ecmascript ",
            "text/javascript1.5",
            "application/x-javascript",
        ] {
            assert!(is_javascript_mime_type(content_type), "{content_type}");
        }
    }

    #[test]
    fn rejects_missing_and_non_javascript_mime_types() {
        for content_type in ["", "text/plain", "text/html", "application/json"] {
            assert!(!is_javascript_mime_type(content_type), "{content_type}");
        }
    }

    #[test]
    fn module_responses_require_javascript_mime_while_classic_scripts_remain_compatible() {
        let mut headers = HeaderList::new();
        headers.append("content-type", "text/html").unwrap();
        let response = FetchResponse {
            response_type: ResponseType::Basic,
            url_list: vec![FetchUrl::parse("https://example.com/module.js").unwrap()],
            status: 200,
            headers,
            body: Body::from_bytes(b"export default 1".to_vec()),
        };

        assert!(
            crate::windows_app::resources::validate_script_response(&response, ScriptKind::Classic)
                .is_ok()
        );
        let error =
            crate::windows_app::resources::validate_script_response(&response, ScriptKind::Module)
                .unwrap_err();
        assert!(error.message().contains("non-JavaScript MIME type"));
    }
}
