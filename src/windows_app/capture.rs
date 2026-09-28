//! Private browser-owned admission for a future `getUserMedia` request path.
//!
//! No renderer message reaches this service yet. A future dispatcher must derive `CaptureContext`
//! from the registered top-level client, show permission UI only while foreground, and attach a
//! nonblocking broker revoker after a capture process has actually started. Permission decisions
//! are scoped to this BrowserApplication, never persisted to the profile.
//!
//! https://w3c.github.io/mediacapture-main/#dom-mediadevices-getusermedia
mod lifecycle;
#[cfg(test)]
mod tests;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CaptureTicket {
    key: CaptureKey,
    generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CaptureAdmission {
    Prompt(CaptureTicket, CaptureKinds),
    Ready(CaptureTicket),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CaptureFailure {
    NotAllowed,
    Stale,
    Busy,
}

#[derive(Clone, Copy, Default)]
struct Permission {
    camera: Option<bool>,
    microphone: Option<bool>,
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
    ticket: CaptureTicket,
    _lease: CaptureLease,
}

#[derive(Default)]
pub(super) struct CaptureCoordinator {
    permissions: HashMap<Origin, Permission>,
    pending: HashMap<CaptureKey, Pending>,
    active: HashMap<CaptureKey, Active>,
    generation: u64,
}

impl CaptureCoordinator {
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
        let permission = self
            .permissions
            .get(&context.origin)
            .copied()
            .unwrap_or_default();
        if (context.kinds.camera && permission.camera == Some(false))
            || (context.kinds.microphone && permission.microphone == Some(false))
        {
            return Err(CaptureFailure::NotAllowed);
        }
        let prompt = CaptureKinds {
            camera: context.kinds.camera && permission.camera.is_none(),
            microphone: context.kinds.microphone && permission.microphone.is_none(),
        };
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
                ready: !prompt.any(),
            },
        );
        Ok(if prompt.any() {
            CaptureAdmission::Prompt(ticket, prompt)
        } else {
            CaptureAdmission::Ready(ticket)
        })
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
        let permission = self.permissions.entry(current.origin.clone()).or_default();
        if let Some(granted) = camera {
            permission.camera = Some(granted);
        }
        if let Some(granted) = microphone {
            permission.microphone = Some(granted);
        }
        if camera == Some(false) || microphone == Some(false) {
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
}
