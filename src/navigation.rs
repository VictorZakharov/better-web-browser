use std::fmt;

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
        let without_fragment = input.split('#').next().unwrap_or(input);
        let (scheme, remainder) = without_fragment
            .split_once("://")
            .ok_or_else(|| UrlError("URL must include http:// or https://".into()))?;
        let scheme = scheme.to_ascii_lowercase();
        if scheme != "http" && scheme != "https" {
            return Err(UrlError(format!("unsupported URL scheme: {scheme}")));
        }
        if remainder.is_empty() || remainder.chars().any(char::is_whitespace) {
            return Err(UrlError(
                "URL contains an invalid host or whitespace".into(),
            ));
        }

        let authority_end = remainder.find(['/', '?']).unwrap_or(remainder.len());
        let authority = &remainder[..authority_end];
        if authority.is_empty() || authority.contains('@') {
            return Err(UrlError(
                "URL host is missing or contains credentials".into(),
            ));
        }

        let default_port = if scheme == "https" { 443 } else { 80 };
        let (host, port) = if let Some(bracketed) = authority.strip_prefix('[') {
            let closing = bracketed
                .find(']')
                .ok_or_else(|| UrlError("invalid IPv6 host".into()))?;
            let host = &bracketed[..closing];
            let suffix = &bracketed[closing + 1..];
            let port = if suffix.is_empty() {
                default_port
            } else {
                suffix
                    .strip_prefix(':')
                    .ok_or_else(|| UrlError("invalid IPv6 port".into()))?
                    .parse::<u16>()
                    .map_err(|_| UrlError("invalid URL port".into()))?
            };
            (host.to_string(), port)
        } else if let Some((host, candidate_port)) = authority.rsplit_once(':') {
            if host.contains(':') {
                return Err(UrlError("IPv6 hosts must use brackets".into()));
            }
            let port = candidate_port
                .parse::<u16>()
                .map_err(|_| UrlError("invalid URL port".into()))?;
            (host.to_string(), port)
        } else {
            (authority.to_string(), default_port)
        };

        if host.is_empty() {
            return Err(UrlError("URL host is missing".into()));
        }

        let remainder = &remainder[authority_end..];
        let path_and_query = if remainder.is_empty() {
            "/".to_string()
        } else if remainder.starts_with('?') {
            format!("/{remainder}")
        } else {
            remainder.to_string()
        };

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
            encode_query(input)
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
    if lowered.starts_with("http://") || lowered.starts_with("https://") {
        return ParsedUrl::parse(reference).ok().map(|url| url.canonical());
    }

    let base = ParsedUrl::parse(base).ok()?;
    if reference.starts_with("//") {
        return ParsedUrl::parse(&format!("{}:{reference}", base.scheme))
            .ok()
            .map(|url| url.canonical());
    }
    if reference.starts_with('#') {
        return Some(format!("{}{}", base.canonical(), reference));
    }

    let reference = reference.split('#').next().unwrap_or(reference);
    let path = if reference.starts_with('/') {
        reference.to_string()
    } else if reference.starts_with('?') {
        let current_path = base.path_and_query.split('?').next().unwrap_or("/");
        format!("{current_path}{reference}")
    } else {
        let current_path = base.path_and_query.split('?').next().unwrap_or("/");
        let directory = current_path
            .rsplit_once('/')
            .map(|(directory, _)| format!("{directory}/"))
            .unwrap_or_else(|| "/".into());
        format!("{directory}{reference}")
    };
    Some(format!("{}{}", base.origin(), normalize_path(&path)))
}

fn normalize_path(path_and_query: &str) -> String {
    let (path, query) = path_and_query
        .split_once('?')
        .map(|(path, query)| (path, Some(query)))
        .unwrap_or((path_and_query, None));
    let trailing_slash = path.ends_with('/');
    let mut segments = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }
    let mut normalized = format!("/{}", segments.join("/"));
    if trailing_slash && normalized != "/" {
        normalized.push('/');
    }
    if let Some(query) = query {
        normalized.push('?');
        normalized.push_str(query);
    }
    normalized
}

fn encode_query(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char)
            }
            b' ' => encoded.push('+'),
            other => {
                const HEX: &[u8; 16] = b"0123456789ABCDEF";
                encoded.push('%');
                encoded.push(HEX[(other >> 4) as usize] as char);
                encoded.push(HEX[(other & 0x0f) as usize] as char);
            }
        }
    }
    encoded
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
}
