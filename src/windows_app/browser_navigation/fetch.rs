//! Browser-owned top-level navigation request construction.

use super::*;
use better_web_browser::fetch::{FetchRequest, FetchSignal, FetchUrl, Referrer};

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
