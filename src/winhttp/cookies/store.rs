//! Browser-owned cookie authority with bounded, versioned state.

use super::{
    CookieSnapshot, StoredCookie, cookie_matches, cookie_pairs, evict_for_domain_limit,
    is_secure_url, parse_cookie, remove_oldest, same_site_allows_request, same_site_allows_setting,
    sort_for_header,
};
use crate::fetch::FetchRequest;
use crate::limits::MAX_COOKIES;
use crate::navigation::ParsedUrl;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::SystemTime;

use super::persistence;

pub(in crate::winhttp) struct CookieStore {
    state: Mutex<CookieState>,
    path: Option<PathBuf>,
    persistence: Mutex<()>,
}

pub(super) struct CookieState {
    pub(super) cookies: Vec<StoredCookie>,
    pub(super) next_creation: u64,
    pub(super) version: u64,
}

impl Default for CookieState {
    fn default() -> Self {
        Self {
            cookies: Vec::new(),
            next_creation: 1,
            version: 1,
        }
    }
}

impl CookieStore {
    pub(in crate::winhttp) fn in_memory() -> Self {
        Self {
            state: Mutex::new(CookieState::default()),
            path: None,
            persistence: Mutex::new(()),
        }
    }

    pub(in crate::winhttp) fn open(path: impl Into<PathBuf>) -> Result<Self, String> {
        let path = path.into();
        let state = persistence::load(&path)?;
        Ok(Self {
            state: Mutex::new(state),
            path: Some(path),
            persistence: Mutex::new(()),
        })
    }

    pub(super) fn set_document_cookie(
        &self,
        document_url: &str,
        assignment: &str,
    ) -> Result<(), String> {
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
            super::parse::parse_cookie_internal(parsed, set_cookie, true, SystemTime::now())
        else {
            return Ok(());
        };
        if !same_site_allows_setting(&cookie, request) {
            return Ok(());
        }
        self.store_cookie(cookie, expired, true, is_secure_url(parsed))
    }

    pub(super) fn request_header(&self, request: &FetchRequest) -> Result<Option<String>, String> {
        let Some(parsed) = request.url.parsed() else {
            return Ok(None);
        };
        let (header, changed) = {
            let mut state = self.lock()?;
            let changed = state.purge_expired(SystemTime::now());
            let mut matching = state
                .cookies
                .iter()
                .filter(|cookie| {
                    cookie_matches(cookie, parsed) && same_site_allows_request(cookie, request)
                })
                .collect::<Vec<_>>();
            sort_for_header(&mut matching);
            (cookie_pairs(matching), changed)
        };
        if changed {
            self.persist()?;
        }
        Ok((!header.is_empty()).then_some(header))
    }

    pub(super) fn document_snapshot(&self, document_url: &str) -> Result<CookieSnapshot, String> {
        let parsed = ParsedUrl::parse(document_url).map_err(|error| error.to_string())?;
        let (snapshot, changed) = {
            let mut state = self.lock()?;
            let changed = state.purge_expired(SystemTime::now());
            let mut matching = state
                .cookies
                .iter()
                .filter(|cookie| !cookie.http_only && cookie_matches(cookie, &parsed))
                .collect::<Vec<_>>();
            sort_for_header(&mut matching);
            (
                CookieSnapshot {
                    version: state.version,
                    header: cookie_pairs(matching),
                },
                changed,
            )
        };
        if changed {
            self.persist()?;
        }
        Ok(snapshot)
    }

    fn store_cookie(
        &self,
        mut cookie: StoredCookie,
        expired: bool,
        from_http: bool,
        source_secure: bool,
    ) -> Result<(), String> {
        let changed = {
            let mut state = self.lock()?;
            let mut changed = state.purge_expired(SystemTime::now());
            let same_cookie = |stored: &StoredCookie| {
                stored.name == cookie.name
                    && stored.domain == cookie.domain
                    && stored.path == cookie.path
                    && stored.host_only == cookie.host_only
            };
            let rejects_http_only_overwrite = !from_http
                && state
                    .cookies
                    .iter()
                    .any(|stored| same_cookie(stored) && stored.http_only);
            let rejects_insecure_overlay = !cookie.secure
                && !source_secure
                && state.cookies.iter().any(|stored| {
                    stored.secure
                        && stored.name == cookie.name
                        && (super::domain_matches(&stored.domain, &cookie.domain)
                            || super::domain_matches(&cookie.domain, &stored.domain))
                        && super::path_matches(&cookie.path, &stored.path)
                });
            if rejects_http_only_overwrite || rejects_insecure_overlay {
                changed
            } else {
                let existing = state.cookies.iter().find(|stored| same_cookie(stored));
                let existed = existing.is_some();
                cookie.creation = existing
                    .map(|stored| stored.creation)
                    .unwrap_or_else(|| state.allocate_creation());
                state.cookies.retain(|stored| !same_cookie(stored));
                if !expired {
                    evict_for_domain_limit(&mut state.cookies, &cookie.domain);
                    if state.cookies.len() >= MAX_COOKIES {
                        remove_oldest(&mut state.cookies, |_| true);
                    }
                    state.cookies.push(cookie);
                    changed = true;
                } else {
                    changed |= existed;
                }
                if changed {
                    state.advance_version();
                }
                changed
            }
        };
        if changed {
            self.persist()?;
        }
        Ok(())
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, CookieState>, String> {
        self.state
            .lock()
            .map_err(|_| "HTTP cookie jar is unavailable".to_string())
    }

    fn persist(&self) -> Result<(), String> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let _serial = self
            .persistence
            .lock()
            .map_err(|_| "cookie persistence lock is poisoned".to_string())?;
        let state = self.lock()?;
        persistence::save(path, &state)
    }
}

impl CookieState {
    fn allocate_creation(&mut self) -> u64 {
        let creation = self.next_creation;
        self.next_creation = creation.checked_add(1).unwrap_or(1);
        creation
    }

    fn purge_expired(&mut self, now: SystemTime) -> bool {
        let before = self.cookies.len();
        self.cookies.retain(|cookie| !cookie.is_expired(now));
        let changed = self.cookies.len() != before;
        if changed {
            self.advance_version();
        }
        changed
    }

    fn advance_version(&mut self) {
        self.version = self.version.checked_add(1).unwrap_or(1);
    }
}
