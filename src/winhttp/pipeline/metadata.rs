//! Browser-owned Fetch Metadata headers, never writable by page script.
//!
//! https://www.w3.org/TR/fetch-metadata/#integration-with-fetch-and-html

use crate::fetch::{
    FetchError, FetchRequest, FetchUrl, HeaderList, RequestDestination, RequestMode,
};
use crate::navigation::ParsedUrl;

pub(super) fn append_fetch_metadata(
    headers: &mut HeaderList,
    request: &FetchRequest,
    url_list: &[FetchUrl],
) -> Result<(), FetchError> {
    // Fetch Metadata is only sent to potentially trustworthy URLs. HTTPS and
    // loopback HTTP are the transport URLs Breeze currently treats as such.
    let Some(target) = request.url.parsed() else {
        return Ok(());
    };
    if target.scheme != "https" && !is_loopback(&target.host) {
        return Ok(());
    }

    headers.set("sec-fetch-dest", destination(request))?;
    headers.set("sec-fetch-mode", mode(request.mode))?;
    headers.set("sec-fetch-site", site(request, url_list))?;
    if request.mode == RequestMode::Navigate && request.user_activation {
        headers.set("sec-fetch-user", "?1")?;
    }
    Ok(())
}

fn is_loopback(host: &str) -> bool {
    host == "localhost"
        || host.ends_with(".localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn destination(request: &FetchRequest) -> &'static str {
    match request.destination {
        RequestDestination::Document if request.resulting_client.id != 0 => "iframe",
        RequestDestination::Document => "document",
        RequestDestination::Style => "style",
        RequestDestination::Image => "image",
        RequestDestination::Script => "script",
        RequestDestination::Worker => "worker",
        RequestDestination::Font => "font",
        RequestDestination::Fetch => "empty",
        RequestDestination::Video => "video",
    }
}

fn mode(mode: RequestMode) -> &'static str {
    match mode {
        RequestMode::Navigate => "navigate",
        RequestMode::SameOrigin => "same-origin",
        RequestMode::NoCors => "no-cors",
        RequestMode::Cors => "cors",
    }
}

fn site(request: &FetchRequest, url_list: &[FetchUrl]) -> &'static str {
    let Some(origin) = &request.origin else {
        return if request.mode == RequestMode::Navigate {
            // Address-bar/bookmark navigations have no initiating site. Keep
            // this value through redirects, as the specification recommends.
            "none"
        } else {
            "cross-site"
        };
    };
    let Ok(source) = ParsedUrl::parse(&format!("{}/", origin.serialize())) else {
        return "cross-site";
    };
    let mut relation = "same-origin";
    for url in url_list {
        let Some(target) = url.parsed() else {
            return "cross-site";
        };
        if origin.is_same_origin(&url.origin()) {
            continue;
        }
        if !crate::winhttp::site::same_site(&source, target) {
            return "cross-site";
        }
        relation = "same-site";
    }
    relation
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::{Origin, RequestClient};

    fn header(request: &FetchRequest, chain: &[&str], name: &str) -> Option<String> {
        let mut headers = HeaderList::new();
        let urls = chain
            .iter()
            .map(|url| FetchUrl::parse(url).unwrap())
            .collect::<Vec<_>>();
        append_fetch_metadata(&mut headers, request, &urls).unwrap();
        headers.get(name).map(str::to_owned)
    }

    #[test]
    fn direct_navigation_has_document_navigate_and_none_metadata() {
        let request = FetchRequest::navigation("https://example.com/").unwrap();
        let chain = [request.url.as_str()];
        assert_eq!(
            header(&request, &chain, "sec-fetch-dest").as_deref(),
            Some("document")
        );
        assert_eq!(
            header(&request, &chain, "sec-fetch-mode").as_deref(),
            Some("navigate")
        );
        assert_eq!(
            header(&request, &chain, "sec-fetch-site").as_deref(),
            Some("none")
        );
        assert_eq!(header(&request, &chain, "sec-fetch-user"), None);
        let mut activated = request.clone();
        activated.user_activation = true;
        assert_eq!(
            header(&activated, &chain, "sec-fetch-user").as_deref(),
            Some("?1")
        );
    }

    #[test]
    fn subresource_site_tracks_origin_and_redirect_chain() {
        let request = FetchRequest::subresource(
            "https://cdn.example.com/a.png",
            "https://www.example.com/page",
            RequestDestination::Image,
        )
        .unwrap();
        assert_eq!(
            header(
                &request,
                &["https://cdn.example.com/a.png"],
                "sec-fetch-site"
            )
            .as_deref(),
            Some("same-site")
        );
        assert_eq!(
            header(
                &request,
                &[
                    "https://elsewhere.test/a.png",
                    "https://cdn.example.com/a.png"
                ],
                "sec-fetch-site"
            )
            .as_deref(),
            Some("cross-site")
        );
        assert_eq!(
            header(
                &request,
                &["https://cdn.example.com/a.png"],
                "sec-fetch-dest"
            )
            .as_deref(),
            Some("image")
        );
        assert_eq!(
            header(
                &request,
                &["https://cdn.example.com/a.png"],
                "sec-fetch-mode"
            )
            .as_deref(),
            Some("no-cors")
        );
    }

    #[test]
    fn child_document_uses_iframe_destination() {
        let mut request = FetchRequest::navigation("https://example.com/child").unwrap();
        request.resulting_client = RequestClient {
            id: 1,
            opaque: false,
        };
        request.origin = Some(Origin::parse("https://example.com/").unwrap());
        assert_eq!(
            header(&request, &[request.url.as_str()], "sec-fetch-dest").as_deref(),
            Some("iframe")
        );
        assert_eq!(
            header(&request, &[request.url.as_str()], "sec-fetch-site").as_deref(),
            Some("same-origin")
        );
    }

    #[test]
    fn non_navigation_cannot_claim_user_activation() {
        let mut request = FetchRequest::subresource(
            "https://example.com/app.js",
            "https://example.com/",
            RequestDestination::Script,
        )
        .unwrap();
        request.user_activation = true;
        assert_eq!(
            header(&request, &[request.url.as_str()], "sec-fetch-user"),
            None
        );
    }

    #[test]
    fn insecure_network_urls_do_not_receive_metadata() {
        let request = FetchRequest::navigation("http://example.com/").unwrap();
        assert_eq!(
            header(&request, &[request.url.as_str()], "sec-fetch-site"),
            None
        );
        let loopback = FetchRequest::navigation("http://127.0.0.1/").unwrap();
        assert_eq!(
            header(&loopback, &[loopback.url.as_str()], "sec-fetch-site").as_deref(),
            Some("none")
        );
        let loopback_alias = FetchRequest::navigation("http://app.localhost/").unwrap();
        assert_eq!(
            header(
                &loopback_alias,
                &[loopback_alias.url.as_str()],
                "sec-fetch-site"
            )
            .as_deref(),
            Some("none")
        );
    }
}
