//! WHATWG URL API parsing, separate from browser navigation scheme policy.

use crate::limits::MAX_URL_BYTES;
use serde::Serialize;
use url::{Url, quirks};

/// An absent base is not the document URL. A supplied base is parsed even when
/// the input is absolute (https://url.spec.whatwg.org/#concept-url-api).
pub fn parse_web_url(reference: &str, base: Option<&str>) -> Option<String> {
    if reference.len() > MAX_URL_BYTES || base.is_some_and(|base| base.len() > MAX_URL_BYTES) {
        return None;
    }
    let base = base
        .map(Url::parse)
        .transpose()
        .ok()?
        .map(normalize_opaque_path);
    let parsed = Url::options()
        .base_url(base.as_ref())
        .parse(reference)
        .ok()?;
    let serialized = normalize_opaque_path(parsed).to_string();
    (serialized.len() <= MAX_URL_BYTES).then_some(serialized)
}

pub fn resolve_web_url(base: &str, reference: &str) -> Option<String> {
    parse_web_url(reference, Some(base))
}

fn normalize_opaque_path(mut parsed: Url) -> Url {
    // The URL opaque-path state encodes only the space directly before ? or #.
    // Keep this space after query/fragment removal as required by live searchParams.
    // https://url.spec.whatwg.org/#opaque-path-state
    if parsed.cannot_be_a_base()
        && (parsed.query().is_some() || parsed.fragment().is_some())
        && let Some(path) = parsed.path().strip_suffix(' ')
    {
        let path = format!("{path}%20");
        parsed.set_path(&path);
    }
    parsed
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebUrlParts {
    pub href: String,
    pub protocol: String,
    pub username: String,
    pub password: String,
    pub host: String,
    pub hostname: String,
    pub port: String,
    pub pathname: String,
    pub search: String,
    pub hash: String,
    pub origin: String,
}

pub fn web_url_parts(value: &str) -> Option<WebUrlParts> {
    if value.len() > MAX_URL_BYTES {
        return None;
    }
    let parsed = Url::parse(value).ok()?;
    Some(WebUrlParts {
        href: quirks::href(&parsed).into(),
        protocol: quirks::protocol(&parsed).into(),
        username: quirks::username(&parsed).into(),
        password: quirks::password(&parsed).into(),
        host: quirks::host(&parsed).into(),
        hostname: quirks::hostname(&parsed).into(),
        port: quirks::port(&parsed).into(),
        pathname: quirks::pathname(&parsed).into(),
        search: quirks::search(&parsed).into(),
        hash: quirks::hash(&parsed).into(),
        origin: quirks::origin(&parsed),
    })
}

/// `url::quirks` implements the Web setters, including ignored invalid input,
/// partial host/port updates, opaque paths, and single query/fragment prefixes.
pub fn set_web_url_component(value: &str, component: &str, input: &str) -> Option<String> {
    if value.len() > MAX_URL_BYTES || input.len() > MAX_URL_BYTES {
        return None;
    }
    let mut parsed = Url::parse(value).ok()?;
    match component {
        "href" => return parse_web_url(input, None),
        "protocol" => {
            let _ = quirks::set_protocol(&mut parsed, input);
        }
        "username" => {
            let _ = quirks::set_username(&mut parsed, input);
        }
        "password" => {
            let _ = quirks::set_password(&mut parsed, input);
        }
        "host" => {
            let _ = quirks::set_host(&mut parsed, input);
        }
        "hostname" => {
            let _ = quirks::set_hostname(&mut parsed, input);
        }
        "port" => {
            let _ = quirks::set_port(&mut parsed, input);
        }
        "pathname" => quirks::set_pathname(&mut parsed, input),
        "search" => quirks::set_search(&mut parsed, input),
        "hash" => quirks::set_hash(&mut parsed, input),
        _ => return None,
    }
    let serialized = parsed.to_string();
    (serialized.len() <= MAX_URL_BYTES).then_some(serialized)
}
