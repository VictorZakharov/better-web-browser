//! Bounded private HTTP cache for completed GET responses.
//!
//! A streamed response is admitted only after EOF. Reuse is partitioned by the browser-owned
//! request context, credentials and cookie state; `Vary` further keys the server-selected fields.
//! This intentionally does not implement heuristic freshness or shared-cache behavior.
//! https://www.rfc-editor.org/rfc/rfc9111.html#section-4

use crate::fetch::{
    CredentialsMode, FetchRequest, HeaderList, RequestCache, RequestContext, RequestDestination,
    RequestMode,
};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc2822;

const MAX_ENTRIES: usize = 64;
const MAX_ENTRY_BYTES: usize = 2 * 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 24 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Partition {
    url: String,
    origin: Option<String>,
    context: RequestContext,
    destination: RequestDestination,
    mode: RequestMode,
    credentials: CredentialsMode,
    cookie: Option<String>,
}

impl Partition {
    fn new(request: &FetchRequest, outbound: &HeaderList) -> Self {
        Self {
            url: request.url.as_str().to_owned(),
            origin: request.origin.as_ref().map(|origin| origin.serialize()),
            context: request.context,
            destination: request.destination,
            mode: request.mode,
            credentials: request.credentials,
            cookie: outbound.get("cookie").map(str::to_owned),
        }
    }
}

#[derive(Clone)]
pub(super) struct CachedResponse {
    partition: Partition,
    vary: Vec<String>,
    request_headers: HeaderList,
    pub(super) status: u16,
    pub(super) headers: HeaderList,
    pub(super) body: Vec<u8>,
    stored_at: Instant,
    initial_age: Duration,
    freshness_lifetime: Duration,
    no_cache: bool,
    must_revalidate: bool,
}

impl CachedResponse {
    fn matches(&self, partition: &Partition, outbound: &HeaderList) -> bool {
        self.partition == *partition
            && self.vary.iter().all(|name| {
                self.request_headers.values(name).collect::<Vec<_>>()
                    == outbound.values(name).collect::<Vec<_>>()
            })
    }

    pub(super) fn fresh(&self) -> bool {
        !self.no_cache
            && self.initial_age.saturating_add(self.stored_at.elapsed()) < self.freshness_lifetime
    }

    pub(super) fn can_serve_stale(&self) -> bool {
        !self.no_cache && !self.must_revalidate
    }

    pub(super) fn response_headers(&self) -> HeaderList {
        let mut headers = self.headers.clone();
        let age = self
            .initial_age
            .saturating_add(self.stored_at.elapsed())
            .as_secs();
        let _ = headers.set("age", &age.to_string());
        headers
    }

    pub(super) fn add_validator(&self, headers: &mut HeaderList) {
        if headers.contains("if-none-match") || headers.contains("if-modified-since") {
            return;
        }
        if let Some(etag) = self.headers.get("etag") {
            let _ = headers.set("if-none-match", etag);
        } else if let Some(modified) = self.headers.get("last-modified") {
            let _ = headers.set("if-modified-since", modified);
        }
    }

    pub(super) fn has_validator(&self) -> bool {
        self.headers.contains("etag") || self.headers.contains("last-modified")
    }

    pub(super) fn revalidated(mut self, headers: &HeaderList) -> Option<Self> {
        // RFC 9111 section 3.2: replace stored fields named by the 304, retaining the
        // representation body and fields omitted from the validation response.
        for field in headers.iter() {
            if matches!(
                field.name(),
                "content-length" | "transfer-encoding" | "set-cookie"
            ) {
                continue;
            }
            self.headers.set(field.name(), field.value()).ok()?;
        }
        let policy = CachePolicy::for_response(&self.headers, SystemTime::now())?;
        self.stored_at = Instant::now();
        self.initial_age = policy.initial_age;
        self.freshness_lifetime = policy.freshness_lifetime;
        self.no_cache = policy.no_cache;
        self.must_revalidate = policy.must_revalidate;
        Some(self)
    }
}

#[derive(Default)]
pub(in crate::winhttp) struct ResponseCache {
    entries: VecDeque<CachedResponse>,
    bytes: usize,
}

impl ResponseCache {
    pub(super) fn lookup(
        &self,
        request: &FetchRequest,
        outbound: &HeaderList,
    ) -> Option<CachedResponse> {
        let partition = Partition::new(request, outbound);
        self.entries
            .iter()
            .rev()
            .find(|entry| entry.matches(&partition, outbound))
            .cloned()
    }

    pub(super) fn insert(&mut self, response: CachedResponse) {
        if response.body.len() > MAX_ENTRY_BYTES {
            return;
        }
        // A changed Vary policy invalidates older variants for this resource;
        // otherwise an old broad match could shadow the new representation.
        let mut index = 0;
        while index < self.entries.len() {
            let entry = &self.entries[index];
            if entry.partition == response.partition
                && (entry.vary != response.vary
                    || entry.matches(&response.partition, &response.request_headers))
            {
                self.bytes = self.bytes.saturating_sub(entry.body.len());
                self.entries.remove(index);
            } else {
                index += 1;
            }
        }
        self.bytes += response.body.len();
        self.entries.push_back(response);
        while self.entries.len() > MAX_ENTRIES || self.bytes > MAX_TOTAL_BYTES {
            if let Some(oldest) = self.entries.pop_front() {
                self.bytes = self.bytes.saturating_sub(oldest.body.len());
            }
        }
    }
}

pub(super) fn cacheable_request(request: &FetchRequest, outbound: &HeaderList) -> bool {
    request.method == "GET"
        && request.body.is_none()
        && request.cache != RequestCache::NoStore
        && !outbound.contains("authorization")
        && !outbound.contains("range")
        && !request.headers.contains("if-none-match")
        && !request.headers.contains("if-modified-since")
        && !has_directive(&request.headers, "no-store")
}

pub(super) fn request_forces_validation(request: &FetchRequest) -> bool {
    has_directive(&request.headers, "no-cache")
        || directive_seconds(&request.headers, "max-age") == Some(0)
        || request.headers.get("pragma").is_some_and(|value| {
            value
                .split(',')
                .any(|token| token.trim().eq_ignore_ascii_case("no-cache"))
        })
}

pub(super) struct CacheCandidate {
    cache: Arc<Mutex<ResponseCache>>,
    response: CachedResponse,
    permitted: bool,
}

impl CacheCandidate {
    pub(super) fn new(
        cache: Arc<Mutex<ResponseCache>>,
        request: &FetchRequest,
        outbound: &HeaderList,
        status: u16,
        headers: &HeaderList,
        requested_at: Instant,
    ) -> Option<Self> {
        if request.cache == RequestCache::NoStore
            || status != 200
            || headers.contains("set-cookie")
            || headers.contains("content-range")
            || !cacheable_request(request, outbound)
        {
            return None;
        }
        let vary = vary_fields(headers)?;
        let mut policy = CachePolicy::for_response(headers, SystemTime::now())?;
        policy.initial_age = policy.initial_age.saturating_add(requested_at.elapsed());
        Some(Self {
            cache,
            response: CachedResponse {
                partition: Partition::new(request, outbound),
                vary,
                request_headers: outbound.clone(),
                status,
                headers: headers.clone(),
                body: Vec::new(),
                stored_at: Instant::now(),
                initial_age: policy.initial_age,
                freshness_lifetime: policy.freshness_lifetime,
                no_cache: policy.no_cache,
                must_revalidate: policy.must_revalidate,
            },
            permitted: true,
        })
    }

    pub(super) fn push(&mut self, chunk: &[u8]) {
        if !self.permitted {
            return;
        }
        if chunk.len() > MAX_ENTRY_BYTES.saturating_sub(self.response.body.len()) {
            self.permitted = false;
            self.response.body.clear();
        } else {
            self.response.body.extend_from_slice(chunk);
        }
    }

    pub(super) fn finish(self) {
        if self.permitted
            && let Ok(mut cache) = self.cache.lock()
        {
            cache.insert(self.response);
        }
    }
}

fn vary_fields(headers: &HeaderList) -> Option<Vec<String>> {
    let mut fields = Vec::new();
    for header in headers.values("vary") {
        for field in header.split(',').map(str::trim) {
            if field == "*" {
                return None;
            }
            if field.is_empty()
                || !field
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            {
                return None;
            }
            let field = field.to_ascii_lowercase();
            if !fields.contains(&field) {
                fields.push(field);
            }
        }
    }
    Some(fields)
}

struct CachePolicy {
    freshness_lifetime: Duration,
    initial_age: Duration,
    no_cache: bool,
    must_revalidate: bool,
}

impl CachePolicy {
    fn for_response(headers: &HeaderList, received_at: SystemTime) -> Option<Self> {
        if has_directive(headers, "no-store") {
            return None;
        }
        let date = headers.get("date").and_then(parse_http_date);
        let lifetime = directive_seconds(headers, "max-age")
            .map(Duration::from_secs)
            .or_else(|| {
                let expires = headers.get("expires").and_then(parse_http_date)?;
                Some(
                    expires
                        .duration_since(date.unwrap_or(received_at))
                        .unwrap_or_default(),
                )
            })
            .unwrap_or_default();
        // A response with no explicit freshness and no validator is not useful to retain.
        if lifetime.is_zero() && !headers.contains("etag") && !headers.contains("last-modified") {
            return None;
        }
        let apparent_age = date
            .and_then(|date| received_at.duration_since(date).ok())
            .unwrap_or_default();
        let age = headers
            .get("age")
            .and_then(|value| value.parse::<u64>().ok())
            .map(Duration::from_secs)
            .unwrap_or_default();
        Some(Self {
            freshness_lifetime: lifetime,
            initial_age: apparent_age.max(age),
            no_cache: has_directive(headers, "no-cache"),
            must_revalidate: has_directive(headers, "must-revalidate"),
        })
    }
}

fn parse_http_date(value: &str) -> Option<SystemTime> {
    let timestamp = OffsetDateTime::parse(value, &Rfc2822)
        .ok()?
        .unix_timestamp();
    if timestamp >= 0 {
        UNIX_EPOCH.checked_add(Duration::from_secs(timestamp as u64))
    } else {
        UNIX_EPOCH.checked_sub(Duration::from_secs(timestamp.unsigned_abs()))
    }
}

fn has_directive(headers: &HeaderList, name: &str) -> bool {
    headers
        .values("cache-control")
        .flat_map(|value| value.split(','))
        .any(|part| {
            part.trim()
                .split('=')
                .next()
                .is_some_and(|token| token.trim().eq_ignore_ascii_case(name))
        })
}

fn directive_seconds(headers: &HeaderList, name: &str) -> Option<u64> {
    for part in headers
        .values("cache-control")
        .flat_map(|value| value.split(','))
    {
        let Some((token, value)) = part.trim().split_once('=') else {
            continue;
        };
        if token.trim().eq_ignore_ascii_case(name) {
            return value.trim().trim_matches('"').parse().ok();
        }
    }
    None
}

#[cfg(test)]
mod tests;
