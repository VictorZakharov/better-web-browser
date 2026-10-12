//! An accepted physical press reserves space for its eventual release.
//!
//! Repeat/down events keep ordinary backpressure. Key-up is not replaceable
//! state and must not be discarded behind a full queue, otherwise a document
//! can retain a pressed movement key forever. All retained releases stay FIFO.
use super::*;
use better_web_browser::renderer_protocol::{KeyPhase, KeyboardInput};
use std::collections::HashSet;

// Win32 virtual-key identities are an eight-bit namespace. Wire validation
// separately caps each key/code string at 64 bytes. The reserve is not an
// enlarged ordinary input queue and cannot grow with typematic repeat rate.
pub(super) const MAX_RESERVATIONS: usize = 256;

pub(in crate::windows_app) struct Admission {
    document: DocumentId,
    sequence: u64,
    phase: KeyPhase,
    identity: String,
}

impl Admission {
    pub(in crate::windows_app) fn from_input(input: &DocumentInput) -> Option<Self> {
        let DocumentInput::Keyboard(KeyboardInput {
            document,
            sequence,
            phase,
            key,
            code,
            ..
        }) = input
        else {
            return None;
        };
        Some(Self {
            document: *document,
            sequence: *sequence,
            phase: *phase,
            // `key` changes when Shift changes while a physical key is held.
            // Unknown native virtual keys have distinct fallback key names.
            identity: if code == "Unidentified" { key } else { code }.clone(),
        })
    }
}

#[derive(Default)]
pub(super) struct Reservations {
    held: HashSet<(DocumentId, String)>,
    queued_releases: HashSet<(DocumentId, u64)>,
}

impl Reservations {
    pub(super) fn can_accept(&self, key: &Admission) -> bool {
        key.phase == KeyPhase::Up
            || self.held.contains(&(key.document, key.identity.clone()))
            || self.held.len() + self.queued_releases.len() < MAX_RESERVATIONS
    }

    pub(super) fn owes_release(&self, key: &Admission) -> bool {
        key.phase == KeyPhase::Up && self.held.contains(&(key.document, key.identity.clone()))
    }

    pub(super) fn accepted(&mut self, key: Admission, queued: bool) {
        let identity = (key.document, key.identity);
        match key.phase {
            KeyPhase::Down => {
                self.held.insert(identity);
            }
            KeyPhase::Up => {
                if self.held.remove(&identity) && queued {
                    self.queued_releases.insert((key.document, key.sequence));
                }
            }
        }
        debug_assert!(self.held.len() + self.queued_releases.len() <= MAX_RESERVATIONS);
    }

    pub(super) fn is_reserved(&self, input: &DocumentInput) -> bool {
        matches!(input, DocumentInput::Keyboard(key) if key.phase == KeyPhase::Up)
            && self
                .queued_releases
                .contains(&(input.document(), input.sequence()))
    }

    pub(super) fn delivered(&mut self, document: DocumentId, sequence: u64) {
        self.queued_releases.remove(&(document, sequence));
    }

    pub(super) fn clear(&mut self) {
        self.held.clear();
        self.queued_releases.clear();
    }
}
