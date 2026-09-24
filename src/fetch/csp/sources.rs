//! CSP source-expression recognition and URL matching.
use url::Url;

pub(super) fn supported(source: &str) -> bool {
    let lower = source.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "'none'"
            | "'self'"
            | "'unsafe-inline'"
            | "'unsafe-eval'"
            | "'strict-dynamic'"
            | "'report-sample'"
            | "*"
    ) {
        return true;
    }
    if nonce_value(source).is_some() {
        return true;
    }
    if source.contains(['\'', '"', '@', '?', '#', '\\']) || !source.is_ascii() {
        return false;
    }
    if let Some(scheme) = source.strip_suffix(':') {
        return valid_scheme(scheme);
    }
    host_parts(source, "https").is_some()
}

pub(super) fn nonce_value(source: &str) -> Option<&str> {
    let value = source.strip_prefix("'nonce-")?.strip_suffix('\'')?;
    valid_base64_value(value).then_some(value)
}

pub(super) fn hash_source(source: &str) -> bool {
    ["'sha256-", "'sha384-", "'sha512-"].iter().any(|prefix| {
        source
            .strip_prefix(prefix)
            .and_then(|value| value.strip_suffix('\''))
            .is_some_and(valid_base64_value)
    })
}

fn valid_base64_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'-' | b'_' | b'=')
        })
}

pub(super) fn matches(source: &str, url: &Url, origin: &Url, redirects: usize) -> bool {
    if source.eq_ignore_ascii_case("'self'") {
        if url.scheme() == "blob" {
            return false;
        }
        // CSP3 §6.7.2.8 includes same-host secure upgrades such as wss:.
        // Explicit non-default ports still have to agree.
        // https://www.w3.org/TR/CSP/#match-url-to-source-expression
        return url.origin() == origin.origin()
            || (origin.host_str() == url.host_str()
                && origin.port() == url.port()
                && (matches!(url.scheme(), "https" | "wss")
                    || (origin.scheme() == "http" && matches!(url.scheme(), "http" | "ws"))));
    }
    if source == "*" {
        return matches!(url.scheme(), "http" | "https" | "ws" | "wss" | "ftp")
            || url.scheme() == origin.scheme();
    }
    if source.starts_with('\'') {
        return false;
    }
    if let Some(scheme) = source.strip_suffix(':') {
        return scheme_matches(scheme, url.scheme());
    }
    let Some((scheme, host, port, path)) = host_parts(source, origin.scheme()) else {
        return false;
    };
    if !scheme_matches(scheme, url.scheme()) {
        return false;
    }
    let Some(actual) = url.host_str() else {
        return false;
    };
    let host_matches = if host == "*" {
        true
    } else if let Some(suffix) = host.strip_prefix("*.") {
        actual.len() > suffix.len() + 1
            && actual
                .to_ascii_lowercase()
                .ends_with(&format!(".{}", suffix.to_ascii_lowercase()))
    } else {
        actual.eq_ignore_ascii_case(host)
    };
    if !host_matches {
        return false;
    }
    if port != Some("*") {
        let expected = port.and_then(|port| port.parse::<u16>().ok());
        if let Some(expected) = expected {
            if url.port_or_known_default() != Some(expected)
                && !(expected == 80 && url.scheme() == "https" && url.port().is_none())
            {
                return false;
            }
        } else if url.port().is_some() {
            return false;
        }
    }
    if redirects > 0 || path.is_empty() {
        return true;
    }
    // Percent-decode segments separately: an escaped slash must not create a path boundary.
    let wanted: Vec<_> = path.split('/').collect();
    let actual: Vec<_> = url.path().split('/').collect();
    let prefix = path.ends_with('/');
    let length = wanted.len() - usize::from(prefix);
    if actual.len() < length || (!prefix && actual.len() != length) {
        return false;
    }
    wanted[..length]
        .iter()
        .zip(&actual)
        .all(|(a, b)| decode(a) == decode(b))
}

fn host_parts<'a>(
    source: &'a str,
    default_scheme: &'a str,
) -> Option<(&'a str, &'a str, Option<&'a str>, &'a str)> {
    let (scheme, rest) = source.split_once("://").unwrap_or((default_scheme, source));
    if !valid_scheme(scheme) {
        return None;
    }
    let (authority, path) = rest
        .find('/')
        .map_or((rest, ""), |index| (&rest[..index], &rest[index..]));
    let (host, port) = authority
        .split_once(':')
        .map_or((authority, None), |(host, port)| (host, Some(port)));
    if host.is_empty()
        || !host
            .bytes()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, b'-' | b'.' | b'*'))
    {
        return None;
    }
    if host.contains('*') && host != "*" && (!host.starts_with("*.") || host[2..].contains('*')) {
        return None;
    }
    if port.is_some_and(|port| port != "*" && port.parse::<u16>().is_err()) {
        return None;
    }
    Some((scheme, host, port, path))
}
fn valid_scheme(scheme: &str) -> bool {
    scheme
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_alphabetic)
        && scheme
            .bytes()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, b'+' | b'-' | b'.'))
}
fn scheme_matches(source: &str, target: &str) -> bool {
    source.eq_ignore_ascii_case(target)
        || matches!(
            (source.to_ascii_lowercase().as_str(), target),
            ("http", "https") | ("ws", "wss" | "http" | "https")
        )
}
fn decode(value: &str) -> Vec<u8> {
    let bytes = value.as_bytes();
    let mut result = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(a), Some(b)) = (
                (bytes[index + 1] as char).to_digit(16),
                (bytes[index + 2] as char).to_digit(16),
            )
        {
            result.push((a * 16 + b) as u8);
            index += 3;
        } else {
            result.push(bytes[index]);
            index += 1;
        }
    }
    result
}
