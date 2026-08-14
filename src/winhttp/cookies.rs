//! Cookie parsing, storage, scope matching, and request-header generation.

use super::client::HttpClient;
use crate::navigation::ParsedUrl;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StoredCookie {
    pub(super) name: String,
    pub(super) value: String,
    pub(super) domain: String,
    pub(super) path: String,
    pub(super) secure: bool,
    pub(super) host_only: bool,
}

impl HttpClient {
    pub fn set_cookie(&self, document_url: &str, assignment: &str) -> Result<(), String> {
        let parsed = ParsedUrl::parse(document_url).map_err(|error| error.to_string())?;
        let Some((cookie, expired)) = parse_cookie(&parsed, assignment) else {
            return Ok(());
        };
        let mut cookies = self
            .cookies
            .lock()
            .map_err(|_| "HTTP cookie jar is unavailable".to_string())?;
        cookies.retain(|stored| {
            stored.name != cookie.name
                || stored.domain != cookie.domain
                || stored.path != cookie.path
        });
        if !expired {
            cookies.push(cookie);
        }
        Ok(())
    }

    pub(super) fn cookie_header(&self, parsed: &ParsedUrl) -> Result<Option<String>, String> {
        let cookies = self
            .cookies
            .lock()
            .map_err(|_| "HTTP cookie jar is unavailable".to_string())?;
        let mut matching = cookies
            .iter()
            .filter(|cookie| cookie_matches(cookie, parsed))
            .collect::<Vec<_>>();
        matching.sort_unstable_by_key(|cookie| std::cmp::Reverse(cookie.path.len()));
        if matching.is_empty() {
            return Ok(None);
        }
        Ok(Some(format!(
            "Cookie: {}\r\n",
            matching
                .into_iter()
                .map(|cookie| format!("{}={}", cookie.name, cookie.value))
                .collect::<Vec<_>>()
                .join("; ")
        )))
    }
}

pub(super) fn parse_cookie(parsed: &ParsedUrl, assignment: &str) -> Option<(StoredCookie, bool)> {
    let mut parts = assignment.split(';');
    let (name, value) = parts.next()?.trim().split_once('=')?;
    let name = name.trim();
    let value = value.trim();
    if name.is_empty()
        || name
            .bytes()
            .any(|byte| byte <= 0x20 || matches!(byte, b';' | b',' | b'='))
        || value
            .bytes()
            .any(|byte| matches!(byte, b'\r' | b'\n' | b';'))
    {
        return None;
    }

    let host = parsed.host.to_ascii_lowercase();
    let mut domain = host.clone();
    let mut host_only = true;
    let mut path = default_cookie_path(&parsed.path_and_query);
    let mut secure = false;
    let mut expired = false;
    for attribute in parts {
        let attribute = attribute.trim();
        let (attribute_name, attribute_value) = attribute
            .split_once('=')
            .map(|(name, value)| (name.trim(), Some(value.trim())))
            .unwrap_or((attribute, None));
        if attribute_name.eq_ignore_ascii_case("domain") {
            let candidate = attribute_value?
                .trim_start_matches('.')
                .to_ascii_lowercase();
            if candidate.is_empty()
                || (host != candidate && !host.ends_with(&format!(".{candidate}")))
            {
                return None;
            }
            domain = candidate;
            host_only = false;
        } else if attribute_name.eq_ignore_ascii_case("path") {
            if let Some(candidate) = attribute_value.filter(|value| value.starts_with('/')) {
                path = candidate.to_string();
            }
        } else if attribute_name.eq_ignore_ascii_case("secure") {
            secure = true;
        } else if attribute_name.eq_ignore_ascii_case("max-age") {
            expired = attribute_value
                .and_then(|value| value.parse::<i64>().ok())
                .is_some_and(|seconds| seconds <= 0);
        }
    }

    Some((
        StoredCookie {
            name: name.to_string(),
            value: value.to_string(),
            domain,
            path,
            secure,
            host_only,
        },
        expired,
    ))
}

fn default_cookie_path(path_and_query: &str) -> String {
    let path = path_and_query.split('?').next().unwrap_or("/");
    if !path.starts_with('/') || path == "/" {
        return "/".into();
    }
    let Some((directory, _)) = path.rsplit_once('/') else {
        return "/".into();
    };
    if directory.is_empty() {
        "/".into()
    } else {
        directory.to_string()
    }
}

pub(super) fn cookie_matches(cookie: &StoredCookie, parsed: &ParsedUrl) -> bool {
    if cookie.secure && parsed.scheme != "https" {
        return false;
    }
    let host = parsed.host.to_ascii_lowercase();
    let domain_matches = if cookie.host_only {
        host == cookie.domain
    } else {
        host == cookie.domain || host.ends_with(&format!(".{}", cookie.domain))
    };
    if !domain_matches {
        return false;
    }
    let request_path = parsed.path_and_query.split('?').next().unwrap_or("/");
    request_path == cookie.path
        || (request_path.starts_with(&cookie.path)
            && (cookie.path.ends_with('/')
                || request_path.as_bytes().get(cookie.path.len()) == Some(&b'/')))
}
