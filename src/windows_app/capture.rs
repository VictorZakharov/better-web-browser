//! Browser-owned admission for `getUserMedia` requests.
//!
//! Every Start requires a fresh foreground decision. A grant belongs to one document, renderer
//! session, and request, and never persists to the profile or a later request.
//!
//! https://w3c.github.io/mediacapture-main/#dom-mediadevices-getusermedia
mod dispatch;
mod lifecycle;
mod service;
#[cfg(test)]
mod tests;

pub(super) use service::{CaptureService, CaptureServiceStatus, WM_APP_CAPTURE};

use super::tabs::TabId;
use better_web_browser::fetch::Origin;
use better_web_browser::renderer_protocol::DocumentId;
use std::collections::HashMap;

const MAX_CAPTURE_REQUESTS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CaptureKinds {
    pub(super) camera: bool,
    pub(super) microphone: bool,
}

impl CaptureKinds {
    fn any(self) -> bool {
        self.camera || self.microphone
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct CaptureKey {
    pub(super) tab: TabId,
    pub(super) document: DocumentId,
    pub(super) renderer_session: u64,
    pub(super) client_id: u64,
    pub(super) request_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CaptureContext {
    pub(super) key: CaptureKey,
    /// Browser-resolved effective origin, never a renderer-supplied URL string.
    pub(super) origin: Origin,
    pub(super) kinds: CaptureKinds,
}

impl CaptureContext {
    fn admissible(&self) -> bool {
        self.kinds.any()
            && self.key.client_id == 0 // No child-frame Permissions Policy path exists yet.
            && self.key.renderer_session != 0
            && self.key.request_id != 0
            && self.origin.is_potentially_trustworthy()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct CaptureTicket {
    key: CaptureKey,
    generation: u64,
}

impl CaptureTicket {
    pub(super) fn key(self) -> CaptureKey {
        self.key
    }

    pub(super) fn capture_id(self) -> u64 {
        self.generation
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CaptureAdmission {
    Prompt(CaptureTicket, CaptureKinds),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CaptureFailure {
    NotAllowed,
    Stale,
    Busy,
}

struct Pending {
    context: CaptureContext,
    ticket: CaptureTicket,
    prompt: CaptureKinds,
    ready: bool,
}

/// The closure must only revoke a broker capability or signal its worker; it must not block the
/// UI thread waiting for native capture shutdown, or reenter BrowserApplication.
pub(super) struct CaptureLease(Option<Box<dyn FnOnce()>>);

impl CaptureLease {
    pub(super) fn new(revoke: impl FnOnce() + 'static) -> Self {
        Self(Some(Box::new(revoke)))
    }
}

impl Drop for CaptureLease {
    fn drop(&mut self) {
        if let Some(revoke) = self.0.take() {
            revoke();
        }
    }
}

struct Active {
    context: CaptureContext,
    running: CaptureKinds,
    ticket: CaptureTicket,
    _lease: CaptureLease,
}

#[derive(Default)]
pub(super) struct CaptureCoordinator {
    denied: HashMap<Origin, CaptureKinds>,
    pending: HashMap<CaptureKey, Pending>,
    active: HashMap<CaptureKey, Active>,
    generation: u64,
}

impl CaptureCoordinator {
    pub(super) fn context_for_key(&self, key: CaptureKey) -> Option<&CaptureContext> {
        self.pending
            .get(&key)
            .map(|pending| &pending.context)
            .or_else(|| self.active.get(&key).map(|active| &active.context))
    }

    pub(super) fn pending_context(&self, ticket: CaptureTicket) -> Option<&CaptureContext> {
        self.pending
            .get(&ticket.key)
            .filter(|pending| pending.ticket == ticket && pending.ready)
            .map(|pending| &pending.context)
    }

    pub(super) fn active_context(&self, ticket: CaptureTicket) -> Option<&CaptureContext> {
        self.active
            .get(&ticket.key)
            .filter(|active| active.ticket == ticket)
            .map(|active| &active.context)
    }

    pub(super) fn active_kinds(&self, ticket: CaptureTicket) -> Option<CaptureKinds> {
        self.active
            .get(&ticket.key)
            .filter(|active| active.ticket == ticket)
            .map(|active| active.running)
    }

    /// A renderer may stop only its own registered request. A stale or different-origin Stop
    /// cannot revoke another document's grant, even when a request identifier was reused.
    pub(super) fn cancel_request(
        &mut self,
        current: &CaptureContext,
    ) -> Result<CaptureTicket, CaptureFailure> {
        if let Some(pending) = self.pending.get(&current.key) {
            if pending.context != *current {
                return Err(CaptureFailure::Stale);
            }
            let ticket = pending.ticket;
            self.pending.remove(&current.key);
            return Ok(ticket);
        }
        if let Some(active) = self.active.get(&current.key) {
            if active.context != *current {
                return Err(CaptureFailure::Stale);
            }
            let ticket = active.ticket;
            self.active.remove(&current.key);
            return Ok(ticket);
        }
        Err(CaptureFailure::Stale)
    }

    pub(super) fn begin(
        &mut self,
        context: CaptureContext,
        foreground: bool,
    ) -> Result<CaptureAdmission, CaptureFailure> {
        if !foreground || !context.admissible() {
            return Err(CaptureFailure::NotAllowed);
        }
        if self.pending.contains_key(&context.key)
            || self.active.contains_key(&context.key)
            || self.pending.len() + self.active.len() >= MAX_CAPTURE_REQUESTS
        {
            return Err(CaptureFailure::Busy);
        }
        if self.denied.get(&context.origin).is_some_and(|denied| {
            (context.kinds.camera && denied.camera)
                || (context.kinds.microphone && denied.microphone)
        }) {
            return Err(CaptureFailure::NotAllowed);
        }
        // A permission is one-shot. Do not inherit a grant (or denial) from another request.
        let prompt = context.kinds;
        self.generation = self.generation.checked_add(1).ok_or(CaptureFailure::Busy)?;
        let ticket = CaptureTicket {
            key: context.key,
            generation: self.generation,
        };
        self.pending.insert(
            context.key,
            Pending {
                context,
                ticket,
                prompt,
                ready: false,
            },
        );
        Ok(CaptureAdmission::Prompt(ticket, prompt))
    }

    pub(super) fn complete_prompt(
        &mut self,
        ticket: CaptureTicket,
        current: &CaptureContext,
        foreground: bool,
        camera: Option<bool>,
        microphone: Option<bool>,
    ) -> Result<(), CaptureFailure> {
        let Some(pending) = self.pending.get(&ticket.key) else {
            return Err(CaptureFailure::Stale);
        };
        if pending.ticket != ticket {
            return Err(CaptureFailure::Stale);
        }
        if !foreground
            || !current.admissible()
            || pending.context != *current
            || pending.ready
            || !pending.prompt.any()
            || camera.is_some() != pending.prompt.camera
            || microphone.is_some() != pending.prompt.microphone
        {
            self.pending.remove(&ticket.key);
            return Err(CaptureFailure::Stale);
        }
        if camera == Some(false) || microphone == Some(false) {
            let denied = self
                .denied
                .entry(current.origin.clone())
                .or_insert(CaptureKinds {
                    camera: false,
                    microphone: false,
                });
            denied.camera |= camera == Some(false);
            denied.microphone |= microphone == Some(false);
            self.pending.remove(&ticket.key);
            return Err(CaptureFailure::NotAllowed);
        }
        self.pending
            .get_mut(&ticket.key)
            .expect("ticket checked")
            .ready = true;
        Ok(())
    }

    pub(super) fn attach(
        &mut self,
        ticket: CaptureTicket,
        current: &CaptureContext,
        foreground: bool,
        lease: CaptureLease,
    ) -> Result<(), CaptureFailure> {
        let Some(pending) = self.pending.get(&ticket.key) else {
            return Err(CaptureFailure::Stale); // `lease` revokes on every rejected attach.
        };
        if pending.ticket != ticket {
            return Err(CaptureFailure::Stale);
        }
        if !foreground || !current.admissible() || pending.context != *current || !pending.ready {
            self.pending.remove(&ticket.key);
            return Err(CaptureFailure::Stale);
        }
        self.pending.remove(&ticket.key);
        self.active.insert(
            ticket.key,
            Active {
                context: current.clone(),
                running: current.kinds,
                ticket,
                _lease: lease,
            },
        );
        Ok(())
    }

    pub(super) fn retire_tab(&mut self, tab: TabId) {
        self.pending.retain(|key, _| key.tab != tab);
        self.active.retain(|key, _| key.tab != tab);
    }

    #[cfg(test)]
    pub(super) fn stop(&mut self, ticket: CaptureTicket) -> Result<(), CaptureFailure> {
        let Some(active) = self.active.get(&ticket.key) else {
            return Err(CaptureFailure::Stale);
        };
        if active.ticket != ticket {
            return Err(CaptureFailure::Stale);
        }
        self.active.remove(&ticket.key);
        Ok(())
    }

    /// Stop exactly one source; the other stays authorized only under this same live grant.
    pub(super) fn stop_track(
        &mut self,
        current: &CaptureContext,
        track_id: u8,
    ) -> Result<(CaptureTicket, CaptureKinds), CaptureFailure> {
        let Some(active) = self.active.get_mut(&current.key) else {
            return Err(CaptureFailure::Stale);
        };
        if active.context != *current {
            return Err(CaptureFailure::Stale);
        }
        let was_running = match track_id {
            1 => std::mem::replace(&mut active.running.camera, false),
            2 => std::mem::replace(&mut active.running.microphone, false),
            _ => return Err(CaptureFailure::Stale),
        };
        if !was_running {
            return Err(CaptureFailure::Stale);
        }
        let ticket = active.ticket;
        let remaining = active.running;
        if !remaining.any() {
            self.active.remove(&current.key);
        }
        Ok((ticket, remaining))
    }
}
