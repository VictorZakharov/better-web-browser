use crate::limits::MAX_URL_BYTES;
use std::fmt;
use url::{Host, Url, form_urlencoded};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedUrl {
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub path_and_query: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlError(pub String);

impl fmt::Display for UrlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for UrlError {}

impl ParsedUrl {
    pub fn parse(input: &str) -> Result<Self, UrlError> {
        if input.len() > MAX_URL_BYTES {
            return Err(UrlError(format!(
                "URL exceeds the {MAX_URL_BYTES}-byte limit"
            )));
        }
        let input = input.trim();
        let mut parsed = Url::parse(input).map_err(|error| UrlError(error.to_string()))?;
        let scheme = parsed.scheme().to_ascii_lowercase();
        if scheme != "http" && scheme != "https" {
            return Err(UrlError(format!("unsupported URL scheme: {scheme}")));
        }
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err(UrlError(
                "URL credentials are not accepted in browser addresses".into(),
            ));
        }
        let host = match parsed.host() {
            Some(Host::Domain(host)) => host.to_string(),
            Some(Host::Ipv4(host)) => host.to_string(),
            Some(Host::Ipv6(host)) => host.to_string(),
            None => return Err(UrlError("URL host is missing".into())),
        };
        let port = parsed
            .port_or_known_default()
            .ok_or_else(|| UrlError("URL port is missing".into()))?;
        parsed.set_fragment(None);
        let mut path_and_query = parsed.path().to_string();
        if let Some(query) = parsed.query() {
            path_and_query.push('?');
            path_and_query.push_str(query);
        }

        let result = Self {
            scheme,
            host,
            port,
            path_and_query,
        };
        if result.canonical().len() > MAX_URL_BYTES {
            return Err(UrlError(format!(
                "canonical URL exceeds the {MAX_URL_BYTES}-byte limit"
            )));
        }
        Ok(result)
    }

    pub fn origin(&self) -> String {
        let default_port = (self.scheme == "https" && self.port == 443)
            || (self.scheme == "http" && self.port == 80);
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        if default_port {
            format!("{}://{host}", self.scheme)
        } else {
            format!("{}://{host}:{}", self.scheme, self.port)
        }
    }

    pub fn canonical(&self) -> String {
        format!("{}{}", self.origin(), self.path_and_query)
    }
}

pub fn normalize_user_input(input: &str) -> Result<String, UrlError> {
    if input.len() > MAX_URL_BYTES {
        return Err(UrlError(format!(
            "address exceeds the {MAX_URL_BYTES}-byte limit"
        )));
    }
    let input = input.trim();
    if input.is_empty() {
        return Err(UrlError("enter an address or search".into()));
    }

    if input.starts_with("http://") || input.starts_with("https://") {
        return Ok(ParsedUrl::parse(input)?.canonical());
    }

    let looks_like_search = input.chars().any(char::is_whitespace)
        || (!input.contains('.') && !input.starts_with("localhost") && !input.contains(':'));
    if looks_like_search {
        let search = format!(
            "https://duckduckgo.com/html/?q={}",
            encode_www_form_component(input)
        );
        if search.len() > MAX_URL_BYTES {
            return Err(UrlError(format!(
                "search URL exceeds the {MAX_URL_BYTES}-byte limit"
            )));
        }
        return Ok(search);
    }

    let candidate = format!("https://{input}");
    Ok(ParsedUrl::parse(&candidate)?.canonical())
}

pub fn resolve_url(base: &str, reference: &str) -> Option<String> {
    if base.len() > MAX_URL_BYTES || reference.len() > MAX_URL_BYTES {
        return None;
    }
    let reference = reference.trim();
    if reference.is_empty() {
        return Some(base.to_string());
    }
    let lowered = reference.to_ascii_lowercase();
    if lowered.starts_with("javascript:")
        || lowered.starts_with("mailto:")
        || lowered.starts_with("tel:")
        || lowered.starts_with("data:")
    {
        return None;
    }
    let base = Url::parse(base).ok()?;
    let resolved = base.join(reference).ok()?;
    if !matches!(resolved.scheme(), "http" | "https")
        || !resolved.username().is_empty()
        || resolved.password().is_some()
    {
        return None;
    }
    let serialized = resolved.to_string();
    (serialized.len() <= MAX_URL_BYTES).then_some(serialized)
}

/// Parses a URL for the Web `URL` interface without applying navigation policy.
///
/// Browser navigation deliberately accepts only credential-free HTTP(S) URLs,
/// while the Web URL parser must also represent schemes such as `data:` and
/// retain fragments. Callers still apply their own scheme policy afterward.
pub fn resolve_web_url(base: &str, reference: &str) -> Option<String> {
    if base.len() > MAX_URL_BYTES || reference.len() > MAX_URL_BYTES {
        return None;
    }
    let base = Url::parse(base).ok()?;
    let resolved = base.join(reference).ok()?;
    let serialized = resolved.to_string();
    (serialized.len() <= MAX_URL_BYTES).then_some(serialized)
}

/// Resolves a subresource reference, including embedded `data:` resources.
/// Navigations deliberately continue to reject `data:` URLs in [`resolve_url`].
pub fn resolve_resource_url(base: &str, reference: &str) -> Option<String> {
    if base.len() > MAX_URL_BYTES || reference.len() > MAX_EMBEDDED_RESOURCE_URL_BYTES {
        return None;
    }
    let reference = reference.trim();
    if reference
        .get(..5)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("data:"))
    {
        return Some(reference.to_string());
    }
    resolve_url(base, reference)
}

const MAX_EMBEDDED_RESOURCE_URL_BYTES: usize = crate::limits::MAX_EMBEDDED_IMAGE_URL_BYTES;

pub fn encode_www_form_component(input: &str) -> String {
    form_urlencoded::Serializer::new(String::new())
        .append_pair("", input)
        .finish()
        .trim_start_matches('=')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_https_url() {
        let url = ParsedUrl::parse("https://example.com:8443/a?q=1#section").unwrap();
        assert_eq!(url.host, "example.com");
        assert_eq!(url.port, 8443);
        assert_eq!(url.path_and_query, "/a?q=1");
        assert_eq!(url.canonical(), "https://example.com:8443/a?q=1");
    }

    #[test]
    fn normalizes_addresses_and_searches() {
        assert_eq!(
            normalize_user_input("example.com/docs").unwrap(),
            "https://example.com/docs"
        );
        assert_eq!(
            normalize_user_input("small fast browser").unwrap(),
            "https://duckduckgo.com/html/?q=small+fast+browser"
        );
    }

    #[test]
    fn resolves_relative_links() {
        let base = "https://example.com/a/b/page.html?old=1";
        assert_eq!(
            resolve_url(base, "../next?q=2").unwrap(),
            "https://example.com/a/next?q=2"
        );
        assert_eq!(
            resolve_url(base, "/root").unwrap(),
            "https://example.com/root"
        );
    }

    #[test]
    fn serializes_unicode_and_spaces_in_relative_url_queries() {
        assert_eq!(
            resolve_url(
                "https://example.com/page",
                "?a=b&notin=\u{2209}\u{00ac}&;& &"
            )
            .as_deref(),
            Some("https://example.com/page?a=b&notin=%E2%88%89%C2%AC&;&%20&")
        );
    }

    #[test]
    fn permits_data_urls_only_for_subresources() {
        let embedded = "data:image/svg+xml,%3Csvg/%3E";
        assert_eq!(resolve_url("https://example.com/", embedded), None);
        assert_eq!(
            resolve_resource_url("https://example.com/", embedded).as_deref(),
            Some(embedded)
        );
    }

    #[test]
    fn web_url_resolution_preserves_non_navigation_schemes_and_fragments() {
        assert_eq!(
            resolve_web_url("https://example.com/base", "data:,hello#part").as_deref(),
            Some("data:,hello#part")
        );
        assert_eq!(
            resolve_web_url("https://example.com/base", "../next#part").as_deref(),
            Some("https://example.com/next#part")
        );
        assert!(resolve_web_url("not a URL", "https://example.com/").is_none());
    }

    #[test]
    fn rejects_oversized_urls_before_parsing_or_encoding() {
        let oversized = "a".repeat(MAX_URL_BYTES + 1);
        assert!(normalize_user_input(&oversized).is_err());
        assert!(ParsedUrl::parse(&format!("https://example.com/{oversized}")).is_err());
        assert_eq!(resolve_url("https://example.com/", &oversized), None);
    }

    #[test]
    fn rejects_resolution_and_search_expansion_beyond_the_url_budget() {
        let reference = format!("/{}", "é".repeat(MAX_URL_BYTES / 3));
        assert!(resolve_url("https://example.com/", &reference).is_none());
        let query = format!("{} search", "é".repeat(MAX_URL_BYTES / 3));
        assert!(normalize_user_input(&query).is_err());
    }
}
