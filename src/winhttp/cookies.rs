//! Cookie parsing, storage, scope matching, expiration, and request policy.

mod date;
mod parse;
mod persistence;
mod store;

use super::client::HttpClient;
use crate::fetch::{FetchRequest, RequestContext};
use crate::limits::{MAX_COOKIE_HEADER_BYTES, MAX_COOKIES_PER_DOMAIN};
use crate::navigation::ParsedUrl;
use std::net::IpAddr;
use std::str::FromStr;
use std::time::SystemTime;

pub(in crate::winhttp) use store::CookieStore;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CookieSnapshot {
    pub version: u64,
    pub header: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SameSite {
    Default,
    Strict,
    Lax,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StoredCookie {
    pub(super) name: String,
    pub(super) value: String,
    pub(super) domain: String,
    pub(super) path: String,
    pub(super) secure: bool,
    pub(super) host_only: bool,
    pub(super) http_only: bool,
    same_site: SameSite,
    expires_at: Option<SystemTime>,
    creation: u64,
}

impl StoredCookie {
    fn is_expired(&self, now: SystemTime) -> bool {
        self.expires_at.is_some_and(|expires| expires <= now)
    }
}

pub(super) fn parse_cookie(parsed: &ParsedUrl, assignment: &str) -> Option<(StoredCookie, bool)> {
    parse::parse_cookie(parsed, assignment)
}

impl HttpClient {
    pub fn set_cookie(&self, document_url: &str, assignment: &str) -> Result<(), String> {
        self.cookie_store
            .set_document_cookie(document_url, assignment)
    }

    pub(super) fn store_response_cookie(
        &self,
        request: &FetchRequest,
        set_cookie: &str,
    ) -> Result<(), String> {
        self.cookie_store.store_response_cookie(request, set_cookie)
    }

    pub(super) fn cookie_header_value(
        &self,
        request: &FetchRequest,
    ) -> Result<Option<String>, String> {
        self.cookie_store.request_header(request)
    }

    pub fn document_cookie_header(&self, document_url: &str) -> Result<String, String> {
        self.document_cookie_snapshot(document_url)
            .map(|snapshot| snapshot.header)
    }

    pub fn document_cookie_snapshot(&self, document_url: &str) -> Result<CookieSnapshot, String> {
        self.cookie_store.document_snapshot(document_url)
    }
}

pub(super) fn cookie_matches(cookie: &StoredCookie, parsed: &ParsedUrl) -> bool {
    if cookie.secure && !is_secure_url(parsed) {
        return false;
    }
    let host = parsed.host.to_ascii_lowercase();
    if cookie.host_only && host != cookie.domain
        || !cookie.host_only
            && (public_suffix(&cookie.domain) || !domain_matches(&host, &cookie.domain))
    {
        return false;
    }
    let request_path = parsed.path_and_query.split('?').next().unwrap_or("/");
    path_matches(request_path, &cookie.path)
}

pub(super) fn domain_matches(host: &str, domain: &str) -> bool {
    host == domain
        || IpAddr::from_str(host).is_err()
            && host.len() > domain.len()
            && host.ends_with(domain)
            && host.as_bytes().get(host.len() - domain.len() - 1) == Some(&b'.')
}

pub(super) fn path_matches(request_path: &str, cookie_path: &str) -> bool {
    request_path == cookie_path
        || request_path.starts_with(cookie_path)
            && (cookie_path.ends_with('/')
                || request_path.as_bytes().get(cookie_path.len()) == Some(&b'/'))
}

pub(super) fn public_suffix(domain: &str) -> bool {
    psl2::lookup(domain).is_some_and(|result| result.is_known() && result.is_public_suffix())
}

pub(super) fn is_secure_url(parsed: &ParsedUrl) -> bool {
    parsed.scheme == "https"
        || parsed.scheme == "http"
            && matches!(parsed.host.as_str(), "localhost" | "127.0.0.1" | "::1")
}

fn same_site_allows_request(cookie: &StoredCookie, request: &FetchRequest) -> bool {
    if cookie.same_site == SameSite::None || request.origin.is_none() {
        return true;
    }
    if request_is_same_site(request) {
        return true;
    }
    matches!(cookie.same_site, SameSite::Lax | SameSite::Default)
        && request.context == RequestContext::Navigation
        && matches!(request.method.as_str(), "GET" | "HEAD" | "OPTIONS")
}

fn same_site_allows_setting(cookie: &StoredCookie, request: &FetchRequest) -> bool {
    cookie.same_site == SameSite::None
        || request.origin.is_none()
        || request_is_same_site(request)
        || request.context == RequestContext::Navigation
}

fn request_is_same_site(request: &FetchRequest) -> bool {
    let Some(origin) = &request.origin else {
        return true;
    };
    let Ok(source) = ParsedUrl::parse(&format!("{}/", origin.serialize())) else {
        return false;
    };
    let Some(target) = request.url.parsed() else {
        return false;
    };
    source.scheme == target.scheme && site_host(&source.host) == site_host(&target.host)
}

fn site_host(host: &str) -> &str {
    psl2::lookup(host)
        .and_then(|domain| domain.registrable_domain())
        .unwrap_or(host)
}

fn sort_for_header(cookies: &mut Vec<&StoredCookie>) {
    cookies.sort_by_key(|cookie| (std::cmp::Reverse(cookie.path.len()), cookie.creation));
}

fn cookie_pairs(cookies: Vec<&StoredCookie>) -> String {
    let mut header = String::new();
    for cookie in cookies {
        let pair = if cookie.name.is_empty() {
            cookie.value.clone()
        } else {
            format!("{}={}", cookie.name, cookie.value)
        };
        let separator = usize::from(!header.is_empty()) * 2;
        if header
            .len()
            .saturating_add(separator)
            .saturating_add(pair.len())
            > MAX_COOKIE_HEADER_BYTES
        {
            break;
        }
        if separator != 0 {
            header.push_str("; ");
        }
        header.push_str(&pair);
    }
    header
}

fn evict_for_domain_limit(cookies: &mut Vec<StoredCookie>, domain: &str) {
    if cookies
        .iter()
        .filter(|cookie| cookie.domain == domain)
        .count()
        >= MAX_COOKIES_PER_DOMAIN
    {
        remove_oldest(cookies, |cookie| cookie.domain == domain);
    }
}

fn remove_oldest(cookies: &mut Vec<StoredCookie>, predicate: impl Fn(&StoredCookie) -> bool) {
    if let Some((index, _)) = cookies
        .iter()
        .enumerate()
        .filter(|(_, cookie)| predicate(cookie))
        .min_by_key(|(_, cookie)| cookie.creation)
    {
        cookies.remove(index);
    }
}

#[cfg(test)]
mod tests;
