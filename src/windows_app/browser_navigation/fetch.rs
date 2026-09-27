//! Browser-owned top-level navigation request construction.

use super::*;
use better_web_browser::fetch::{FetchRequest, FetchSignal, FetchUrl, Referrer};

pub(super) fn response_document_url(
    requested_url: &str,
    fetched_url: &str,
    redirected: bool,
) -> String {
    // Fetch strips fragments from request URLs, but a non-redirecting navigation keeps the
    // requested fragment as the document URL and as the session-history entry's URL.
    if !redirected
        && requested_url.contains('#')
        && FetchUrl::parse(requested_url).is_ok_and(|url| url.as_str() == fetched_url)
    {
        requested_url.to_owned()
    } else {
        fetched_url.to_owned()
    }
}

pub(super) fn fetch_navigation(
    client: &winhttp::HttpClient,
    url: &str,
    signal: &FetchSignal,
    referrer: Option<&str>,
    post_body: Option<FormPost>,
    user_activation: bool,
) -> Result<winhttp::StreamingFetchResponse, String> {
    let mut request = FetchRequest::navigation(url).map_err(|error| error.to_string())?;
    request.user_activation = user_activation;
    if let Some(referrer) = referrer {
        request.origin = Some(Origin::parse(referrer).map_err(|error| error.to_string())?);
        request.referrer =
            Referrer::Url(FetchUrl::parse(referrer).map_err(|error| error.to_string())?);
    }
    if let Some(post) = post_body {
        request
            .set_method("POST")
            .map_err(|error| error.to_string())?;
        request
            .headers
            .set("content-type", &post.content_type)
            .map_err(|error| error.to_string())?;
        request.body = Some(better_web_browser::fetch::Body::from_bytes(post.body));
    }
    let request = request.with_signal(signal.clone());
    client
        .fetch_stream(request)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_redirecting_response_retains_requested_fragment() {
        assert_eq!(
            response_document_url(
                "https://example.test/route#pane",
                "https://example.test/route",
                false,
            ),
            "https://example.test/route#pane"
        );
        assert_eq!(
            response_document_url(
                "https://example.test/route#pane",
                "https://example.test/route",
                true,
            ),
            "https://example.test/route"
        );
        assert_eq!(
            response_document_url(
                "https://example.test/route#pane",
                "https://example.test/other",
                false,
            ),
            "https://example.test/other"
        );
    }
}
