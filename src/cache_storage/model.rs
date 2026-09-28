use super::persistence;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use url::Url;

mod matching;
use matching::{matches_entry, vary_fields};

pub(super) const MAX_ORIGIN_BYTES: usize = 16 * 1024 * 1024;
pub(super) const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
const MAX_NAME_BYTES: usize = 1024;

#[derive(Debug)]
pub enum CacheError {
    InvalidRequest,
    InvalidResponse,
    VaryStar,
    Quota,
    Duplicate,
    Persistence(String),
}

impl std::fmt::Display for CacheError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest => f.write_str("invalid Cache API request"),
            Self::InvalidResponse => f.write_str("invalid Cache API response"),
            Self::VaryStar => f.write_str("a Cache API response has Vary: *"),
            Self::Quota => f.write_str("the Cache API quota was exceeded"),
            Self::Duplicate => f.write_str("duplicate requests in one Cache.addAll operation"),
            Self::Persistence(error) => write!(f, "Cache API storage is unavailable: {error}"),
        }
    }
}

impl std::error::Error for CacheError {}

impl CacheError {
    pub fn name(&self) -> &'static str {
        match self {
            Self::InvalidRequest | Self::InvalidResponse => "TypeError",
            Self::VaryStar => "TypeError",
            Self::Quota => "QuotaExceededError",
            Self::Duplicate => "InvalidStateError",
            Self::Persistence(_) => "UnknownError",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheRequest {
    pub url: String,
    pub method: String,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheResponse {
    pub status: u16,
    pub status_text: String,
    pub response_type: String,
    pub url: String,
    pub redirected: bool,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    pub body_base64: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CacheEntry {
    pub request: CacheRequest,
    pub response: CacheResponse,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheQueryOptions {
    #[serde(default)]
    pub ignore_search: bool,
    #[serde(default)]
    pub ignore_method: bool,
    #[serde(default)]
    pub ignore_vary: bool,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum CacheCommand {
    Open {
        name: String,
    },
    Has {
        name: String,
    },
    Names,
    DeleteCache {
        name: String,
    },
    Match {
        name: Option<String>,
        request: CacheRequest,
        #[serde(default)]
        options: CacheQueryOptions,
    },
    MatchAll {
        name: String,
        request: Option<CacheRequest>,
        #[serde(default)]
        options: CacheQueryOptions,
    },
    Keys {
        name: String,
        request: Option<CacheRequest>,
        #[serde(default)]
        options: CacheQueryOptions,
    },
    Put {
        name: String,
        entries: Vec<CacheEntry>,
    },
    DeleteEntry {
        name: String,
        request: CacheRequest,
        #[serde(default)]
        options: CacheQueryOptions,
    },
}

#[derive(Clone, Default, Deserialize, Serialize)]
pub(super) struct State {
    pub(super) origins: BTreeMap<String, Vec<NamedCache>>,
}

#[derive(Clone, Deserialize, Serialize)]
pub(super) struct NamedCache {
    pub(super) name: String,
    pub(super) entries: Vec<CacheEntry>,
}

pub struct CacheStorage {
    path: Option<PathBuf>,
    // Persistent state is read by the origin-storage worker on first use,
    // never on the browser UI's startup path.
    state: Mutex<Option<State>>,
}

impl CacheStorage {
    pub fn in_memory() -> Self {
        Self {
            path: None,
            state: Mutex::new(Some(State::default())),
        }
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, CacheError> {
        let path = path.as_ref().to_path_buf();
        Ok(Self {
            state: Mutex::new(None),
            path: Some(path),
        })
    }

    pub fn execute(&self, origin: &str, command: CacheCommand) -> Result<Value, CacheError> {
        // The browser derives this value from its committed Fetch client. This
        // check also protects direct callers of the storage model.
        let parsed = Url::parse(origin).map_err(|_| CacheError::InvalidRequest)?;
        if !matches!(parsed.scheme(), "http" | "https")
            || parsed.origin().ascii_serialization() != origin
            || !crate::fetch::Origin::parse(origin)
                .map_err(|_| CacheError::InvalidRequest)?
                .is_potentially_trustworthy()
        {
            return Err(CacheError::InvalidRequest);
        }
        match command {
            CacheCommand::Open { name } => self.mutate(origin, |caches| {
                validate_name(&name)?;
                if caches.iter().any(|cache| cache.name == name) {
                    return Ok((json!(null), false));
                }
                caches.push(NamedCache {
                    name,
                    entries: Vec::new(),
                });
                Ok((json!(null), true))
            }),
            CacheCommand::Has { name } => self.read(origin, |caches| {
                json!(caches.iter().any(|cache| cache.name == name))
            }),
            CacheCommand::Names => self.read(origin, |caches| {
                json!(caches.iter().map(|cache| &cache.name).collect::<Vec<_>>())
            }),
            CacheCommand::DeleteCache { name } => self.mutate(origin, |caches| {
                let previous = caches.len();
                caches.retain(|cache| cache.name != name);
                let changed = previous != caches.len();
                Ok((json!(changed), changed))
            }),
            CacheCommand::Match {
                name,
                request,
                options,
            } => self.read(origin, |caches| {
                let found = caches
                    .iter()
                    .filter(|cache| name.as_ref().is_none_or(|name| *name == cache.name))
                    .flat_map(|cache| &cache.entries)
                    .find(|entry| matches_entry(&request, entry, options));
                json!(found.map(|entry| &entry.response))
            }),
            CacheCommand::MatchAll {
                name,
                request,
                options,
            } => self.read(origin, |caches| {
                json!(
                    caches
                        .iter()
                        .find(|cache| cache.name == name)
                        .into_iter()
                        .flat_map(|cache| &cache.entries)
                        .filter(|entry| request
                            .as_ref()
                            .is_none_or(|request| matches_entry(request, entry, options)))
                        .map(|entry| &entry.response)
                        .collect::<Vec<_>>()
                )
            }),
            CacheCommand::Keys {
                name,
                request,
                options,
            } => self.read(origin, |caches| {
                json!(
                    caches
                        .iter()
                        .find(|cache| cache.name == name)
                        .into_iter()
                        .flat_map(|cache| &cache.entries)
                        .filter(|entry| request
                            .as_ref()
                            .is_none_or(|request| matches_entry(request, entry, options)))
                        .map(|entry| &entry.request)
                        .collect::<Vec<_>>()
                )
            }),
            CacheCommand::Put { name, entries } => self.mutate(origin, |caches| {
                validate_name(&name)?;
                if entries.is_empty() {
                    return Ok((json!(null), false));
                }
                for entry in &entries {
                    validate_entry(entry)?;
                }
                for (index, entry) in entries.iter().enumerate() {
                    if entries[..index].iter().any(|previous| {
                        matches_entry(&entry.request, previous, CacheQueryOptions::default())
                    }) {
                        return Err(CacheError::Duplicate);
                    }
                }
                let cache = caches
                    .iter_mut()
                    .find(|cache| cache.name == name)
                    .ok_or(CacheError::InvalidRequest)?;
                for entry in entries {
                    cache.entries.retain(|stored| {
                        !matches_entry(&entry.request, stored, CacheQueryOptions::default())
                    });
                    cache.entries.push(entry);
                }
                Ok((json!(null), true))
            }),
            CacheCommand::DeleteEntry {
                name,
                request,
                options,
            } => self.mutate(origin, |caches| {
                let Some(cache) = caches.iter_mut().find(|cache| cache.name == name) else {
                    return Ok((json!(false), false));
                };
                let previous = cache.entries.len();
                cache
                    .entries
                    .retain(|entry| !matches_entry(&request, entry, options));
                let changed = previous != cache.entries.len();
                Ok((json!(changed), changed))
            }),
        }
    }

    fn read(
        &self,
        origin: &str,
        f: impl FnOnce(&[NamedCache]) -> Value,
    ) -> Result<Value, CacheError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| CacheError::Persistence("storage lock poisoned".into()))?;
        let state = self.ensure_loaded(&mut state)?;
        Ok(f(state
            .origins
            .get(origin)
            .map(Vec::as_slice)
            .unwrap_or_default()))
    }

    fn mutate(
        &self,
        origin: &str,
        f: impl FnOnce(&mut Vec<NamedCache>) -> Result<(Value, bool), CacheError>,
    ) -> Result<Value, CacheError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| CacheError::Persistence("storage lock poisoned".into()))?;
        let mut candidate = self.ensure_loaded(&mut state)?.clone();
        let (result, changed) = f(candidate.origins.entry(origin.into()).or_default())?;
        if !changed {
            return Ok(result);
        }
        persistence::validate(&candidate, MAX_ORIGIN_BYTES, MAX_TOTAL_BYTES)?;
        if let Some(path) = &self.path {
            persistence::write(path, &candidate)?;
        }
        *state = Some(candidate);
        Ok(result)
    }

    fn ensure_loaded<'a>(&self, state: &'a mut Option<State>) -> Result<&'a State, CacheError> {
        if state.is_none() {
            *state = Some(persistence::load(
                self.path.as_ref().expect("persistent path"),
            )?);
        }
        Ok(state.as_ref().expect("loaded cache state"))
    }
}

fn validate_name(name: &str) -> Result<(), CacheError> {
    if name.len() > MAX_NAME_BYTES {
        Err(CacheError::Quota)
    } else {
        Ok(())
    }
}

fn validate_entry(entry: &CacheEntry) -> Result<(), CacheError> {
    let url = Url::parse(&entry.request.url).map_err(|_| CacheError::InvalidRequest)?;
    if !matches!(url.scheme(), "http" | "https") || entry.request.method != "GET" {
        return Err(CacheError::InvalidRequest);
    }
    if entry.response.response_type == "error"
        || !matches!(
            entry.response.response_type.as_str(),
            "default" | "basic" | "cors"
        )
        || !(200..=599).contains(&entry.response.status)
        || entry.response.status == 206
    {
        return Err(CacheError::InvalidResponse);
    }
    if vary_fields(&entry.response.headers)
        .iter()
        .any(|field| field == "*")
    {
        return Err(CacheError::VaryStar);
    }
    if let Some(body) = &entry.response.body_base64 {
        if body.len() > MAX_BODY_BYTES * 4 / 3 + 4
            || base64::engine::general_purpose::STANDARD
                .decode(body)
                .map_err(|_| CacheError::InvalidResponse)?
                .len()
                > MAX_BODY_BYTES
        {
            return Err(CacheError::Quota);
        }
    }
    Ok(())
}
