//! Browser-owned screen wake locks. Renderer IDs are never OS-handle authority.

mod dispatch;
pub(in crate::windows_app) mod native;
#[cfg(test)]
mod tests;

use super::tabs::TabId;
use better_web_browser::renderer_protocol::DocumentId;
use std::collections::HashSet;

const MAX_ACTIVE_LOCKS: usize = 64;
const MAX_LOCKS_PER_TAB: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct Key {
    tab: TabId,
    document: DocumentId,
    session_id: u64,
    request_id: u64,
}

impl Key {
    pub(super) fn new(tab: TabId, document: DocumentId, session_id: u64, request_id: u64) -> Self {
        Self {
            tab,
            document,
            session_id,
            request_id,
        }
    }
}

pub(super) trait PlatformWakeLock {
    fn acquire(&mut self) -> bool;
    fn release(&mut self);
}

pub(super) struct WakeLockService<P: PlatformWakeLock> {
    platform: P,
    active: HashSet<Key>,
}

impl<P: PlatformWakeLock> WakeLockService<P> {
    pub(super) fn new(platform: P) -> Self {
        Self {
            platform,
            active: HashSet::new(),
        }
    }

    pub(super) fn acquire(&mut self, key: Key) -> bool {
        if self.active.contains(&key)
            || self.active.len() >= MAX_ACTIVE_LOCKS
            || self
                .active
                .iter()
                .filter(|existing| existing.tab == key.tab)
                .count()
                >= MAX_LOCKS_PER_TAB
        {
            return false;
        }
        if self.active.is_empty() {
            // The Screen Wake Lock spec deliberately hides OS acquisition failure:
            // power policy and battery state must not become a fingerprinting API.
            let _ = self.platform.acquire();
        }
        self.active.insert(key);
        true
    }

    pub(super) fn release(&mut self, key: Key) -> bool {
        if !self.active.remove(&key) {
            return false;
        }
        if self.active.is_empty() {
            self.platform.release();
        }
        true
    }

    pub(super) fn retire_tab(&mut self, tab: TabId) -> Vec<Key> {
        let retired = self
            .active
            .iter()
            .copied()
            .filter(|key| key.tab == tab)
            .collect::<Vec<_>>();
        for key in &retired {
            self.active.remove(key);
        }
        if !retired.is_empty() && self.active.is_empty() {
            self.platform.release();
        }
        retired
    }
}
