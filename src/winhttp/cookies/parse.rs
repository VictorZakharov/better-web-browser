//! The permissive Set-Cookie consumer parser from RFC 6265bis section 5.6.

use super::date::parse_cookie_date;
use super::{SameSite, StoredCookie, domain_matches, is_secure_url, public_suffix};
use crate::navigation::ParsedUrl;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MAX_COOKIE_PAIR_BYTES: usize = 4_096;
const MAX_ATTRIBUTE_VALUE_BYTES: usize = 1_024;
pub(super) const MAX_PERSISTENT_AGE: Duration = Duration::from_secs(400 * 24 * 60 * 60);

#[derive(Debug, Clone, Copy)]
enum MaxAge {
    Expired,
    Seconds(u64),
}

pub(super) fn parse_cookie(parsed: &ParsedUrl, assignment: &str) -> Option<(StoredCookie, bool)> {
    parse_cookie_internal(parsed, assignment, false, SystemTime::now())
}

pub(super) fn parse_cookie_internal(
    parsed: &ParsedUrl,
    assignment: &str,
    from_http: bool,
    now: SystemTime,
) -> Option<(StoredCookie, bool)> {
    if assignment.bytes().any(is_excluded_control) {
        return None;
    }

    let (pair, attributes) = assignment.split_once(';').unwrap_or((assignment, ""));
    let (name, value) = pair.split_once('=').unwrap_or(("", pair));
    let name = trim_whitespace(name);
    let value = trim_whitespace(value);
    if name.is_empty() && value.is_empty()
        || name.len().saturating_add(value.len()) > MAX_COOKIE_PAIR_BYTES
    {
        return None;
    }

    let host = parsed.host.to_ascii_lowercase();
    let default_path = default_cookie_path(&parsed.path_and_query);
    let mut domain_attribute = None;
    let mut path_attribute = None;
    let mut secure = false;
    let mut http_only = false;
    let mut same_site = SameSite::Default;
    let mut max_age = None;
    let mut expires_at = None;

    for attribute in attributes.split(';') {
        let (attribute_name, attribute_value) =
            attribute.split_once('=').unwrap_or((attribute, ""));
        let attribute_name = trim_whitespace(attribute_name);
        let attribute_value = trim_whitespace(attribute_value);
        if attribute_value.len() > MAX_ATTRIBUTE_VALUE_BYTES {
            continue;
        }

        if attribute_name.eq_ignore_ascii_case("domain") {
            if !attribute_value
                .bytes()
                .all(|byte| (0x01..=0x7f).contains(&byte))
            {
                return None;
            }
            let candidate = attribute_value
                .strip_prefix('.')
                .unwrap_or(attribute_value)
                .to_ascii_lowercase();
            domain_attribute = Some(candidate);
        } else if attribute_name.eq_ignore_ascii_case("path") {
            path_attribute = Some(if attribute_value.starts_with('/') {
                attribute_value.to_string()
            } else {
                default_path.clone()
            });
        } else if attribute_name.eq_ignore_ascii_case("secure") {
            secure = true;
        } else if attribute_name.eq_ignore_ascii_case("httponly") {
            http_only = true;
        } else if attribute_name.eq_ignore_ascii_case("max-age") {
            if let Some(parsed) = parse_max_age(attribute_value) {
                max_age = Some(parsed);
            }
        } else if attribute_name.eq_ignore_ascii_case("expires") {
            if let Some(parsed) = parse_cookie_date(attribute_value) {
                expires_at = Some(clamp_expiry(parsed, now));
            }
        } else if attribute_name.eq_ignore_ascii_case("samesite") {
            same_site = if attribute_value.eq_ignore_ascii_case("strict") {
                SameSite::Strict
            } else if attribute_value.eq_ignore_ascii_case("lax") {
                SameSite::Lax
            } else if attribute_value.eq_ignore_ascii_case("none") {
                SameSite::None
            } else {
                SameSite::Default
            };
        }
    }

    let (domain, host_only) = apply_domain_attribute(&host, domain_attribute)?;
    let path_attribute_present = path_attribute.is_some();
    let path = path_attribute.unwrap_or(default_path);
    if secure && !is_secure_url(parsed)
        || http_only && !from_http
        || same_site == SameSite::None && !secure
    {
        return None;
    }
    if starts_with_ascii_case_insensitive(name, "__Secure-") && !secure
        || starts_with_ascii_case_insensitive(name, "__Host-")
            && (!secure || !host_only || !path_attribute_present || path != "/")
        || name.is_empty()
            && (starts_with_ascii_case_insensitive(value, "__Secure-")
                || starts_with_ascii_case_insensitive(value, "__Host-"))
    {
        return None;
    }

    if let Some(max_age) = max_age {
        expires_at = Some(match max_age {
            MaxAge::Expired => UNIX_EPOCH,
            MaxAge::Seconds(seconds) => now
                .checked_add(Duration::from_secs(seconds).min(MAX_PERSISTENT_AGE))
                .unwrap_or(now),
        });
    }
    let expired = expires_at.is_some_and(|expires| expires <= now);
    Some((
        StoredCookie {
            name: name.to_string(),
            value: value.to_string(),
            domain,
            path,
            secure,
            host_only,
            http_only,
            same_site,
            expires_at,
            creation: 0,
        },
        expired,
    ))
}

fn apply_domain_attribute(host: &str, domain_attribute: Option<String>) -> Option<(String, bool)> {
    let Some(mut domain) = domain_attribute else {
        return Some((host.to_string(), true));
    };
    if public_suffix(&domain) {
        if domain != host {
            return None;
        }
        domain.clear();
    }
    if domain.is_empty() {
        return Some((host.to_string(), true));
    }
    domain_matches(host, &domain).then_some((domain, false))
}

fn parse_max_age(value: &str) -> Option<MaxAge> {
    if let Some(digits) = value.strip_prefix('-') {
        return (!digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()))
            .then_some(MaxAge::Expired);
    }
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some(MaxAge::Seconds(value.parse().unwrap_or(u64::MAX)))
}

fn clamp_expiry(expiry: SystemTime, now: SystemTime) -> SystemTime {
    let limit = now.checked_add(MAX_PERSISTENT_AGE).unwrap_or(now);
    expiry.min(limit)
}

fn default_cookie_path(path_and_query: &str) -> String {
    let path = path_and_query.split('?').next().unwrap_or("/");
    if !path.starts_with('/') || path == "/" {
        return "/".into();
    }
    match path.rsplit_once('/') {
        Some(("", _)) | None => "/".into(),
        Some((directory, _)) => directory.to_string(),
    }
}

fn trim_whitespace(value: &str) -> &str {
    value.trim_matches([' ', '\t'])
}

fn is_excluded_control(byte: u8) -> bool {
    matches!(byte, 0x00..=0x08 | 0x0a..=0x1f | 0x7f)
}

fn starts_with_ascii_case_insensitive(value: &str, prefix: &str) -> bool {
    value
        .get(..prefix.len())
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_age_parser_accepts_large_values_and_rejects_non_decimal_forms() {
        assert!(matches!(parse_max_age("0"), Some(MaxAge::Seconds(0))));
        assert!(matches!(
            parse_max_age("-999999999999999999999999"),
            Some(MaxAge::Expired)
        ));
        assert!(matches!(
            parse_max_age("999999999999999999999999"),
            Some(MaxAge::Seconds(u64::MAX))
        ));
        assert!(parse_max_age("+10").is_none());
        assert!(parse_max_age("1.5").is_none());
    }
}
