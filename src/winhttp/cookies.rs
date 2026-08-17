//! Cookie parsing, storage, scope matching, expiration, and request policy.

mod date;
mod parse;

use super::client::HttpClient;
use crate::fetch::{FetchRequest, RequestContext};
use crate::navigation::ParsedUrl;
use std::net::IpAddr;
use std::str::FromStr;
use std::sync::atomic::Ordering;
use std::time::SystemTime;

const MAX_COOKIES: usize = 3_000;
const MAX_COOKIES_PER_DOMAIN: usize = 180;

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
        let parsed = ParsedUrl::parse(document_url).map_err(|error| error.to_string())?;
        let Some((cookie, expired)) = parse_cookie(&parsed, assignment) else {
            return Ok(());
        };
        self.store_cookie(cookie, expired, false, is_secure_url(&parsed))
    }

    pub(super) fn store_response_cookie(
        &self,
        request: &FetchRequest,
        set_cookie: &str,
    ) -> Result<(), String> {
        let Some(parsed) = request.url.parsed() else {
            return Ok(());
        };
        let Some((cookie, expired)) =
            parse::parse_cookie_internal(parsed, set_cookie, true, SystemTime::now())
        else {
            return Ok(());
        };
        if !same_site_allows_setting(&cookie, request) {
            return Ok(());
        }
        self.store_cookie(cookie, expired, true, is_secure_url(parsed))
    }

    fn store_cookie(
        &self,
        mut cookie: StoredCookie,
        expired: bool,
        from_http: bool,
        source_secure: bool,
    ) -> Result<(), String> {
        let mut cookies = self
            .cookies
            .lock()
            .map_err(|_| "HTTP cookie jar is unavailable".to_string())?;
        let now = SystemTime::now();
        cookies.retain(|stored| !stored.is_expired(now));
        let same_cookie = |stored: &StoredCookie| {
            stored.name == cookie.name
                && stored.domain == cookie.domain
                && stored.path == cookie.path
                && stored.host_only == cookie.host_only
        };
        if !from_http
            && cookies
                .iter()
                .any(|stored| same_cookie(stored) && stored.http_only)
        {
            return Ok(());
        }
        if !cookie.secure
            && !source_secure
            && cookies.iter().any(|stored| {
                stored.secure
                    && stored.name == cookie.name
                    && (domain_matches(&stored.domain, &cookie.domain)
                        || domain_matches(&cookie.domain, &stored.domain))
                    && path_matches(&cookie.path, &stored.path)
            })
        {
            return Ok(());
        }
        if let Some(existing) = cookies.iter().find(|stored| same_cookie(stored)) {
            cookie.creation = existing.creation;
        } else {
            cookie.creation = self.next_cookie_creation.fetch_add(1, Ordering::Relaxed);
        }
        cookies.retain(|stored| !same_cookie(stored));
        if expired {
            return Ok(());
        }
        evict_for_domain_limit(&mut cookies, &cookie.domain);
        if cookies.len() >= MAX_COOKIES {
            remove_oldest(&mut cookies, |_| true);
        }
        cookies.push(cookie);
        Ok(())
    }

    pub(super) fn cookie_header_value(
        &self,
        request: &FetchRequest,
    ) -> Result<Option<String>, String> {
        let Some(parsed) = request.url.parsed() else {
            return Ok(None);
        };
        let mut cookies = self
            .cookies
            .lock()
            .map_err(|_| "HTTP cookie jar is unavailable".to_string())?;
        let now = SystemTime::now();
        cookies.retain(|cookie| !cookie.is_expired(now));
        let mut matching = cookies
            .iter()
            .filter(|cookie| {
                cookie_matches(cookie, parsed) && same_site_allows_request(cookie, request)
            })
            .collect::<Vec<_>>();
        sort_for_header(&mut matching);
        let header = cookie_pairs(matching);
        Ok((!header.is_empty()).then_some(header))
    }

    pub fn document_cookie_header(&self, document_url: &str) -> Result<String, String> {
        let parsed = ParsedUrl::parse(document_url).map_err(|error| error.to_string())?;
        let mut cookies = self
            .cookies
            .lock()
            .map_err(|_| "HTTP cookie jar is unavailable".to_string())?;
        let now = SystemTime::now();
        cookies.retain(|cookie| !cookie.is_expired(now));
        let mut matching = cookies
            .iter()
            .filter(|cookie| !cookie.http_only && cookie_matches(cookie, &parsed))
            .collect::<Vec<_>>();
        sort_for_header(&mut matching);
        Ok(cookie_pairs(matching))
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
    cookies
        .into_iter()
        .map(|cookie| {
            if cookie.name.is_empty() {
                cookie.value.clone()
            } else {
                format!("{}={}", cookie.name, cookie.value)
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
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
