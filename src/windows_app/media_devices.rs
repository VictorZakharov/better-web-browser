//! Browser-owned, privacy-limited media-device discovery.
//!
//! The Media Capture specification exposes at most one anonymous entry per
//! input kind before capture. This service only transmits presence bits;
//! native device identifiers and names never cross into renderer IPC.
//! https://www.w3.org/TR/mediacapture-streams/#device-information-exposure

mod dispatch;
mod native;
#[cfg(test)]
mod tests;

use super::browser_app::TabMessageRouter;
use super::platform::{Hwnd, PostMessageW, WM_APP};
use super::tabs::TabId;
use better_web_browser::renderer_protocol::{
    DocumentId, MediaDeviceError, MediaDeviceRequest, MediaDeviceResult, MediaDeviceUpdate,
};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub(super) const WM_APP_MEDIA_DEVICES: u32 = WM_APP + 21;
const MAX_REQUESTS: usize = 64;
const MAX_EVENTS: usize = 128;
const ENUMERATION_DEADLINE: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Presence {
    microphone: bool,
    camera: bool,
}

type PresenceResult = Result<Presence, MediaDeviceError>;

/// Tests inject a provider that cannot touch OS devices or open a permission UI.
trait MediaDeviceProvider {
    fn enumerate(
        &self,
        done: Box<dyn FnOnce(PresenceResult) + Send>,
    ) -> Result<(), MediaDeviceError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct RequestKey {
    tab: TabId,
    document: DocumentId,
    session_id: u64,
    request_id: u64,
}

impl RequestKey {
    fn new(tab: TabId, document: DocumentId, session_id: u64, request_id: u64) -> Self {
        Self {
            tab,
            document,
            session_id,
            request_id,
        }
    }
}

type Delivery = Arc<dyn Fn(MediaDeviceUpdate)>;

struct Context {
    key: RequestKey,
    request: MediaDeviceRequest,
    deliver: Delivery,
}

impl Context {
    fn emit(&self, result: PresenceResult) {
        let result = match result {
            Ok(Presence { microphone, camera }) => {
                MediaDeviceResult::Presence { microphone, camera }
            }
            Err(error) => MediaDeviceResult::Error(error),
        };
        (self.deliver)(MediaDeviceUpdate {
            document: self.key.document,
            request_id: self.key.request_id,
            result,
        });
    }
}

#[derive(Clone, Copy)]
struct NativeEvent {
    key: RequestKey,
    result: PresenceResult,
    completed_at: Instant,
}

struct ActiveRequest {
    context: Context,
    deadline: Instant,
}

pub(super) struct MediaDeviceService {
    deferred: HashMap<RequestKey, Context>,
    active: HashMap<RequestKey, ActiveRequest>,
    events: Arc<Mutex<VecDeque<NativeEvent>>>,
    overflow: Arc<AtomicBool>,
    provider: Box<dyn MediaDeviceProvider>,
}

impl Default for MediaDeviceService {
    fn default() -> Self {
        Self::with_provider(Box::new(native::WinRtMediaDeviceProvider::default()))
    }
}

impl MediaDeviceService {
    fn with_provider(provider: Box<dyn MediaDeviceProvider>) -> Self {
        Self {
            deferred: HashMap::new(),
            active: HashMap::new(),
            events: Arc::new(Mutex::new(VecDeque::new())),
            overflow: Arc::new(AtomicBool::new(false)),
            provider,
        }
    }

    fn defer(&mut self, context: Context) {
        if self.contains_or_full(context.key) {
            context.emit(Err(MediaDeviceError::NotReadable));
        } else {
            self.deferred.insert(context.key, context);
        }
    }

    fn take_deferred(&mut self, tab: TabId) -> Vec<Context> {
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

    fn start(&mut self, context: Context, router: &TabMessageRouter) {
        if self.contains_or_full(context.key) {
            context.emit(Err(MediaDeviceError::NotReadable));
            return;
        }
        let key = context.key;
        self.active.insert(
            key,
            ActiveRequest {
                context,
                deadline: Instant::now() + ENUMERATION_DEADLINE,
            },
        );
        let events = Arc::clone(&self.events);
        let overflow = Arc::clone(&self.overflow);
        let router = router.clone();
        let result = self.provider.enumerate(Box::new(move |result| {
            Self::queue_event(
                &events,
                &overflow,
                &router,
                NativeEvent {
                    key,
                    result,
                    completed_at: Instant::now(),
                },
            );
        }));
        if let Err(error) = result
            && let Some(active) = self.active.remove(&key)
        {
            active.context.emit(Err(error));
        }
    }

    fn contains_or_full(&self, key: RequestKey) -> bool {
        self.active.contains_key(&key)
            || self.deferred.contains_key(&key)
            || self.active.len() + self.deferred.len() >= MAX_REQUESTS
    }

    fn queue_event(
        events: &Arc<Mutex<VecDeque<NativeEvent>>>,
        overflow: &Arc<AtomicBool>,
        router: &TabMessageRouter,
        event: NativeEvent,
    ) {
        let Some(window) = router.destination(event.key.tab) else {
            return;
        };
        let mut pending = events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if pending.len() < MAX_EVENTS {
            pending.push_back(event);
        } else {
            overflow.store(true, Ordering::Release);
        }
        drop(pending);
        unsafe { PostMessageW(window as Hwnd, WM_APP_MEDIA_DEVICES, 0, 0) };
    }

    fn drain_events(&mut self, router: &TabMessageRouter, window: Hwnd, visible: Option<TabId>) {
        let mut events = self
            .events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if self.overflow.swap(false, Ordering::AcqRel) {
            // Explicitly reject bounded-queue overflow; never strand a promise.
            events.clear();
            events.extend(self.active.keys().copied().map(|key| NativeEvent {
                key,
                result: Err(MediaDeviceError::NotReadable),
                completed_at: Instant::now(),
            }));
        }
        let mut retained = VecDeque::new();
        let mut ready = Vec::new();
        while let Some(event) = events.pop_front() {
            if !self.active.contains_key(&event.key) {
                continue;
            }
            if router.destination(event.key.tab) == Some(window as usize)
                && visible == Some(event.key.tab)
            {
                ready.push(event);
            } else {
                retained.push_back(event);
            }
        }
        *events = retained;
        drop(events);
        for event in ready {
            if let Some(active) = self.active.remove(&event.key) {
                active
                    .context
                    .emit(if event.completed_at <= active.deadline {
                        event.result
                    } else {
                        Err(MediaDeviceError::NotReadable)
                    });
            }
        }
        let expired: Vec<_> = self
            .active
            .iter()
            .filter(|(key, active)| {
                router.destination(key.tab) == Some(window as usize)
                    && visible == Some(key.tab)
                    && Instant::now() >= active.deadline
            })
            .map(|(key, _)| *key)
            .collect();
        for key in expired {
            if let Some(active) = self.active.remove(&key) {
                active.context.emit(Err(MediaDeviceError::NotReadable));
            }
        }
    }

    fn retire_tab(&mut self, tab: TabId) {
        self.deferred.retain(|key, _| key.tab != tab);
        self.active.retain(|key, _| key.tab != tab);
        self.events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .retain(|event| event.key.tab != tab);
    }
}
