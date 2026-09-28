//! Browser-owned, session-scoped Geolocation state and asynchronous OS admission.
//!
//! https://www.w3.org/TR/geolocation/ requires secure-context permission,
//! visibility-gated acquisition, optional cache reuse, and document cancellation.
mod dispatch;
mod lifecycle;
use lifecycle::{error, unix_time_millis};
mod native;
#[cfg(test)]
mod tests;

use super::browser_app::TabMessageRouter;
use super::platform::{Hwnd, PostMessageW, WM_APP};
use super::tabs::TabId;
use better_web_browser::renderer_protocol::{
    DocumentId, GeolocationAction, GeolocationErrorCode, GeolocationEvent, GeolocationPosition,
    GeolocationRequest, GeolocationUpdate,
};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub(super) const WM_APP_GEOLOCATION: u32 = WM_APP + 20;
const MAX_ACTIVE_REQUESTS: usize = 64;
const MAX_QUEUED_EVENTS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GeoFailure {
    PermissionDenied,
    PositionUnavailable,
    Timeout,
}

type GeoResult = Result<GeolocationPosition, GeoFailure>;

pub(super) trait GeoHandle {}

/// The production adapter uses WinRT. Tests replace it without ever invoking
/// RequestAccessAsync or opening a system permission dialog.
pub(super) trait GeoProvider {
    fn request_access(
        &self,
        done: Box<dyn FnOnce(Result<(), GeoFailure>) + Send>,
    ) -> Result<(), GeoFailure>;
    fn start(
        &self,
        watch: bool,
        high_accuracy: bool,
        maximum_age_millis: u64,
        timeout_millis: u64,
        deliver: Arc<dyn Fn(GeoResult) + Send + Sync>,
    ) -> Result<Box<dyn GeoHandle>, GeoFailure>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct GeoKey {
    tab: TabId,
    document: DocumentId,
    session_id: u64,
    request_id: u64,
}

impl GeoKey {
    pub(super) fn new(tab: TabId, document: DocumentId, session_id: u64, request_id: u64) -> Self {
        Self {
            tab,
            document,
            session_id,
            request_id,
        }
    }
}

pub(super) type GeoDelivery = Arc<dyn Fn(GeolocationUpdate)>;

#[derive(Clone)]
pub(super) struct GeoContext {
    pub(super) key: GeoKey,
    pub(super) request: GeolocationRequest,
    pub(super) origin: String,
    pub(super) deliver: GeoDelivery,
}

impl GeoContext {
    fn emit(&self, event: GeolocationEvent, terminal: bool) {
        (self.deliver)(GeolocationUpdate {
            document: self.key.document,
            request_id: self.key.request_id,
            terminal,
            event,
        });
    }
}

enum NativeEvent {
    Access(GeoKey, Result<(), GeoFailure>),
    Position(GeoKey, u64, GeoResult),
}

struct ActiveGeo {
    context: GeoContext,
    watch: bool,
    high_accuracy: bool,
    maximum_age_millis: u64,
    timeout_millis: u64,
    access_granted: bool,
    generation: u64,
    source: Option<Box<dyn GeoHandle>>,
    deadline: Option<Instant>,
    remaining: Option<Duration>,
    retry_after: Option<Instant>,
    last_timestamp_millis: Option<u64>,
    acquisition_started_millis: u64,
}

pub(super) struct GeolocationService {
    permissions: HashMap<String, bool>,
    deferred: HashMap<GeoKey, GeoContext>,
    active: HashMap<GeoKey, ActiveGeo>,
    cache: HashMap<(String, bool), GeolocationPosition>,
    events: Arc<Mutex<VecDeque<NativeEvent>>>,
    events_overflow: Arc<AtomicBool>,
    provider: Box<dyn GeoProvider>,
}

impl Default for GeolocationService {
    fn default() -> Self {
        Self::with_provider(Box::new(native::WinRtGeoProvider))
    }
}

impl GeolocationService {
    fn with_provider(provider: Box<dyn GeoProvider>) -> Self {
        Self {
            permissions: HashMap::new(),
            deferred: HashMap::new(),
            active: HashMap::new(),
            cache: HashMap::new(),
            events: Arc::new(Mutex::new(VecDeque::new())),
            events_overflow: Arc::new(AtomicBool::new(false)),
            provider,
        }
    }

    fn permission(&self, origin: &str) -> Option<bool> {
        self.permissions.get(origin).copied()
    }

    fn decide(&mut self, origin: String, granted: bool) {
        self.permissions.insert(origin, granted);
    }

    fn defer(&mut self, context: GeoContext) {
        if self.deferred.contains_key(&context.key)
            || self.active.contains_key(&context.key)
            || self.deferred.len() + self.active.len() >= MAX_ACTIVE_REQUESTS
        {
            context.emit(error(GeoFailure::PositionUnavailable), true);
        } else {
            self.deferred.insert(context.key, context);
        }
    }

    fn take_deferred(&mut self, tab: TabId) -> Vec<GeoContext> {
        let keys: Vec<_> = self
            .deferred
            .keys()
            .filter(|key| key.tab == tab)
            .copied()
            .collect();
        keys.into_iter()
            .filter_map(|key| self.deferred.remove(&key))
            .collect()
    }

    fn clear(&mut self, key: GeoKey) {
        self.deferred.remove(&key);
        self.active.remove(&key);
    }

    fn retire_tab(&mut self, tab: TabId) {
        self.deferred.retain(|key, _| key.tab != tab);
        self.active.retain(|key, _| key.tab != tab);
        self.events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .retain(|event| match event {
                NativeEvent::Access(key, _) | NativeEvent::Position(key, _, _) => key.tab != tab,
            });
    }

    fn queue_event(
        events: &Arc<Mutex<VecDeque<NativeEvent>>>,
        overflow: &Arc<AtomicBool>,
        router: &TabMessageRouter,
        key: GeoKey,
        event: NativeEvent,
    ) {
        let Some(window) = router.destination(key.tab) else {
            return;
        };
        let mut pending = events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let NativeEvent::Position(key, generation, result) = &event
            && let Some(existing) = pending.iter_mut().find(|existing| matches!(existing,
                NativeEvent::Position(old_key, old_generation, _) if old_key == key && old_generation == generation
            ))
        {
            // The latest sample wins, except that an OS permission revocation
            // must not be replaced by a later location sample.
            if !matches!(existing, NativeEvent::Position(_, _, Err(GeoFailure::PermissionDenied))) {
                *existing = NativeEvent::Position(*key, *generation, result.clone());
            }
            drop(pending);
            unsafe { PostMessageW(window as Hwnd, WM_APP_GEOLOCATION, 0, 0); }
            return;
        }
        if pending.len() >= MAX_QUEUED_EVENTS {
            // An explicit terminal error on the UI thread is safer than an
            // unbounded queue or a silently stranded one-shot callback.
            overflow.store(true, Ordering::Release);
        } else {
            pending.push_back(event);
        }
        drop(pending);
        unsafe {
            PostMessageW(window as Hwnd, WM_APP_GEOLOCATION, 0, 0);
        }
    }

    /// Called only after the site grant and foreground-window checks.
    fn start(&mut self, context: GeoContext, router: &TabMessageRouter) {
        let GeolocationAction::Start {
            watch,
            high_accuracy,
            timeout_millis,
            maximum_age_millis,
        } = context.request.action
        else {
            return;
        };
        if self.active.len() + self.deferred.len() >= MAX_ACTIVE_REQUESTS
            || self.active.contains_key(&context.key)
        {
            context.emit(error(GeoFailure::PositionUnavailable), true);
            return;
        }
        let key = context.key;
        self.active.insert(
            key,
            ActiveGeo {
                context,
                watch,
                high_accuracy,
                maximum_age_millis,
                timeout_millis,
                access_granted: false,
                generation: 0,
                source: None,
                deadline: None,
                remaining: finite_timeout(timeout_millis),
                retry_after: None,
                last_timestamp_millis: None,
                acquisition_started_millis: 0,
            },
        );
        let events = Arc::clone(&self.events);
        let overflow = Arc::clone(&self.events_overflow);
        let router = router.clone();
        let result = self.provider.request_access(Box::new(move |result| {
            Self::queue_event(
                &events,
                &overflow,
                &router,
                key,
                NativeEvent::Access(key, result),
            );
        }));
        if let Err(failure) = result
            && let Some(active) = self.active.remove(&key)
        {
            active.context.emit(error(failure), true);
        }
    }

    fn start_source(&mut self, key: GeoKey, router: &TabMessageRouter) {
        let Some(active) = self.active.get_mut(&key) else {
            return;
        };
        if !active.access_granted {
            return;
        }
        if let Some(cached) = cache_hit(
            &self.cache,
            &active.context.origin,
            active.high_accuracy,
            active.maximum_age_millis,
        ) && active.last_timestamp_millis != Some(cached.timestamp_millis)
        {
            active.last_timestamp_millis = Some(cached.timestamp_millis);
            active
                .context
                .emit(GeolocationEvent::Position(cached), !active.watch);
            if !active.watch {
                self.active.remove(&key);
                return;
            }
        }
        if active.timeout_millis == 0 && active.last_timestamp_millis.is_none() {
            active
                .context
                .emit(error(GeoFailure::Timeout), !active.watch);
            if !active.watch {
                self.active.remove(&key);
                return;
            }
        }
        active.generation = active.generation.wrapping_add(1);
        active.acquisition_started_millis = unix_time_millis();
        let generation = active.generation;
        let events = Arc::clone(&self.events);
        let overflow = Arc::clone(&self.events_overflow);
        let router = router.clone();
        let deliver = Arc::new(move |result| {
            Self::queue_event(
                &events,
                &overflow,
                &router,
                key,
                NativeEvent::Position(key, generation, result),
            );
        });
        match self.provider.start(
            active.watch,
            active.high_accuracy,
            active.maximum_age_millis,
            active.timeout_millis,
            deliver,
        ) {
            Ok(source) => {
                active.source = Some(source);
                active.retry_after = None;
                // timeout=0 reports once if no cached position; an active watch
                // then waits for PositionChanged rather than spinning every tick.
                active.deadline = active
                    .remaining
                    .filter(|duration| !duration.is_zero())
                    .and_then(|duration| Instant::now().checked_add(duration));
            }
            Err(failure) => {
                let terminal = !active.watch || failure == GeoFailure::PermissionDenied;
                active.context.emit(error(failure), terminal);
                if terminal {
                    self.active.remove(&key);
                } else {
                    // A broken provider should not be reinvoked every UI tick.
                    active.retry_after = Instant::now().checked_add(Duration::from_secs(5));
                }
            }
        }
    }
}

fn finite_timeout(millis: u64) -> Option<Duration> {
    (millis != u64::MAX).then(|| Duration::from_millis(millis))
}

fn cache_hit(
    cache: &HashMap<(String, bool), GeolocationPosition>,
    origin: &str,
    high_accuracy: bool,
    maximum_age_millis: u64,
) -> Option<GeolocationPosition> {
    if maximum_age_millis == 0 {
        return None;
    }
    let position = cache.get(&(origin.to_owned(), high_accuracy))?;
    let now = unix_time_millis();
    (position.timestamp_millis <= now && now - position.timestamp_millis <= maximum_age_millis)
        .then(|| position.clone())
}
