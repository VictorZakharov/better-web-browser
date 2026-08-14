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

        Ok(Self {
            scheme,
            host,
            port,
            path_and_query,
        })
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
        return Ok(format!(
            "https://duckduckgo.com/html/?q={}",
            encode_www_form_component(input)
        ));
    }

    let candidate = format!("https://{input}");
    Ok(ParsedUrl::parse(&candidate)?.canonical())
}

pub fn resolve_url(base: &str, reference: &str) -> Option<String> {
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
    Some(resolved.to_string())
}

/// Resolves a subresource reference, including embedded `data:` resources.
/// Navigations deliberately continue to reject `data:` URLs in [`resolve_url`].
pub fn resolve_resource_url(base: &str, reference: &str) -> Option<String> {
    let reference = reference.trim();
    if reference
        .get(..5)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("data:"))
    {
        return Some(reference.to_string());
    }
    resolve_url(base, reference)
}

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
}
