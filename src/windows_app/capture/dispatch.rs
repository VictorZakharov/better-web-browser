//! Browser-owned permission and stream routing for untrusted renderer capture intents.

use super::*;
use crate::windows_app::{app_state::BrowserState, platform::MessageBoxW, win32_helpers::wide};
use better_web_browser::capture_process::{CapturedSample, CapturedSampleKind};
use better_web_browser::fetch::RequestClient;
use better_web_browser::renderer_process::MediaCaptureSink;
use better_web_browser::renderer_protocol::{
    MediaCaptureAction, MediaCaptureError, MediaCaptureEvent, MediaCaptureFrame,
    MediaCaptureFrameKind, MediaCaptureRequest, MediaCaptureUpdate,
};
use std::sync::atomic::Ordering;

struct Route {
    key: CaptureKey,
    client: RequestClient,
    origin: Option<Origin>,
    sink: MediaCaptureSink,
}

impl Route {
    fn send(&self, event: MediaCaptureEvent) -> Result<(), String> {
        self.sink.try_send_update(MediaCaptureUpdate {
            document: self.key.document,
            request_id: self.key.request_id,
            event,
        })
    }
}

impl BrowserState {
    fn capture_route(&mut self, tab_id: TabId, request: &MediaCaptureRequest) -> Option<Route> {
        self.tabs.get_mut(tab_id).and_then(|tab| {
            if !tab.navigation.owns_document(request.document) {
                return None;
            }
            let session = tab.renderer_session.as_ref()?;
            let session_id = session.snapshot().session_id;
            let owner = tab
                .renderer_fetches
                .resolve_client(request.document, &tab.reader_url, request.client)
                .ok();
            Some(Route {
                key: CaptureKey {
                    tab: tab_id,
                    document: request.document,
                    renderer_session: session_id,
                    client_id: request.client.id,
                    request_id: request.request_id,
                },
                client: request.client,
                origin: owner.map(|owner| owner.origin),
                sink: session.media_capture_sink(request.document),
            })
        })
    }

    fn live_capture_route(&mut self, context: &CaptureContext) -> Option<Route> {
        let request = MediaCaptureRequest {
            document: context.key.document,
            request_id: context.key.request_id,
            client: context.client,
            action: MediaCaptureAction::Start {
                camera: context.kinds.camera,
                microphone: context.kinds.microphone,
            },
        };
        let route = self.capture_route(context.key.tab, &request)?;
        (route.key == context.key && route.origin.as_ref() == Some(&context.origin))
            .then_some(route)
    }

    fn ticket_context(&self, ticket: CaptureTicket) -> Option<CaptureContext> {
        let capture = self.app.capture.borrow();
        capture
            .pending_context(ticket)
            .or_else(|| capture.active_context(ticket))
            .cloned()
    }

    pub(in crate::windows_app) fn handle_media_capture_request(
        &mut self,
        tab_id: TabId,
        request: MediaCaptureRequest,
    ) {
        if request.validate().is_err() {
            return;
        }
        let Some(route) = self.capture_route(tab_id, &request) else {
            return;
        };
        let Some(origin) = route.origin.clone() else {
            let _ = route.send(MediaCaptureEvent::Error(MediaCaptureError::NotAllowed));
            return;
        };
        match request.action {
            MediaCaptureAction::Stop { track_id } => {
                // A Stop is authorized only for its original document/session/client and origin.
                let context = self
                    .app
                    .capture
                    .borrow()
                    .context_for_key(route.key)
                    .cloned();
                if let Some(context) = context {
                    if context.origin != origin {
                        let _ = route.send(MediaCaptureEvent::Error(MediaCaptureError::NotAllowed));
                        return;
                    }
                    if track_id == 0 {
                        if let Ok(ticket) = self.app.capture.borrow_mut().cancel_request(&context) {
                            self.app.capture_service.borrow_mut().revoke(ticket);
                        }
                        let _ = route.send(MediaCaptureEvent::Ended);
                    } else if let Ok((ticket, remaining)) =
                        self.app.capture.borrow_mut().stop_track(&context, track_id)
                    {
                        self.app
                            .capture_service
                            .borrow_mut()
                            .revoke_track(ticket, track_id);
                        let event = if remaining.camera || remaining.microphone {
                            MediaCaptureEvent::TrackEnded { track_id }
                        } else {
                            MediaCaptureEvent::Ended
                        };
                        let _ = route.send(event);
                    }
                    return;
                }
                let _ = route.send(MediaCaptureEvent::Ended);
            }
            MediaCaptureAction::Start { camera, microphone } => {
                let context = CaptureContext {
                    key: route.key,
                    client: route.client,
                    origin,
                    kinds: CaptureKinds { camera, microphone },
                };
                let admission = self
                    .app
                    .capture
                    .borrow_mut()
                    .begin(context.clone(), self.capture_foreground(tab_id));
                let CaptureAdmission::Prompt(ticket, kinds) = match admission {
                    Ok(admission) => admission,
                    Err(_) => {
                        let _ = route.send(MediaCaptureEvent::Error(MediaCaptureError::NotAllowed));
                        return;
                    }
                };
                let devices = match (kinds.camera, kinds.microphone) {
                    (true, true) => "camera and microphone",
                    (true, false) => "camera",
                    (false, true) => "microphone",
                    (false, false) => unreachable!("admission requires a device"),
                };
                let question = wide(&format!(
                    "Allow {} to use your {devices} for this request?",
                    context.origin.serialize()
                ));
                let title = wide("Breeze camera and microphone permission");
                // MB_YESNO | MB_ICONQUESTION. capture_foreground excludes hidden automation.
                let granted = unsafe {
                    MessageBoxW(self.window, question.as_ptr(), title.as_ptr(), 0x24) == 6
                };
                let Some(current) = self.live_capture_route(&context) else {
                    let _ = self.app.capture.borrow_mut().cancel_request(&context);
                    let _ = route.send(MediaCaptureEvent::Error(MediaCaptureError::Abort));
                    return;
                };
                let prompt_result = self.app.capture.borrow_mut().complete_prompt(
                    ticket,
                    &context,
                    self.capture_foreground(tab_id),
                    kinds.camera.then_some(granted),
                    kinds.microphone.then_some(granted),
                );
                if let Err(failure) = prompt_result {
                    let error = match failure {
                        CaptureFailure::NotAllowed => MediaCaptureError::NotAllowed,
                        CaptureFailure::Stale | CaptureFailure::Busy => MediaCaptureError::Abort,
                    };
                    let _ = current.send(MediaCaptureEvent::Error(error));
                    return;
                }
                if self
                    .app
                    .capture_service
                    .borrow_mut()
                    .start(ticket, kinds)
                    .is_err()
                {
                    let _ = self.app.capture.borrow_mut().cancel_request(&context);
                    let _ = current.send(MediaCaptureEvent::Error(MediaCaptureError::NotReadable));
                }
            }
        }
    }

    /// Runs on the UI thread, but only drains bounded mailboxes; native capture is worker-owned.
    pub(in crate::windows_app) fn tick_capture(&mut self) {
        let statuses = self.app.capture_service.borrow_mut().statuses();
        for status in statuses {
            let ticket = match status {
                CaptureServiceStatus::Started(ticket)
                | CaptureServiceStatus::Failed(ticket, _)
                | CaptureServiceStatus::TrackEnded(ticket, _) => ticket,
            };
            let Some(context) = self.ticket_context(ticket) else {
                self.app.capture_service.borrow_mut().revoke(ticket);
                continue;
            };
            let Some(route) = self.live_capture_route(&context) else {
                self.app.capture_service.borrow_mut().revoke(ticket);
                let _ = self.app.capture.borrow_mut().cancel_request(&context);
                continue;
            };
            match status {
                CaptureServiceStatus::Started(_) => {
                    let tokens = self.app.capture_service.borrow().tokens(ticket);
                    if tokens.len()
                        != usize::from(context.kinds.camera) + usize::from(context.kinds.microphone)
                    {
                        self.app.capture_service.borrow_mut().revoke(ticket);
                        let _ = self.app.capture.borrow_mut().cancel_request(&context);
                        let _ =
                            route.send(MediaCaptureEvent::Error(MediaCaptureError::NotReadable));
                        continue;
                    }
                    let lease = CaptureLease::new(move || {
                        for token in tokens {
                            token.store(true, Ordering::Release);
                        }
                    });
                    if self
                        .app
                        .capture
                        .borrow_mut()
                        .attach(
                            ticket,
                            &context,
                            self.capture_foreground(ticket.key().tab),
                            lease,
                        )
                        .is_err()
                    {
                        self.app.capture_service.borrow_mut().revoke(ticket);
                        let _ = route.send(MediaCaptureEvent::Error(MediaCaptureError::Abort));
                        continue;
                    }
                    if route
                        .send(MediaCaptureEvent::Started {
                            camera: context.kinds.camera,
                            microphone: context.kinds.microphone,
                        })
                        .is_err()
                    {
                        let _ = self.app.capture.borrow_mut().cancel_request(&context);
                        self.app.capture_service.borrow_mut().revoke(ticket);
                    }
                }
                CaptureServiceStatus::Failed(_, error) => {
                    let _ = self.app.capture.borrow_mut().cancel_request(&context);
                    self.app.capture_service.borrow_mut().revoke(ticket);
                    let _ = route.send(MediaCaptureEvent::Error(error));
                }
                CaptureServiceStatus::TrackEnded(_, track_id) => {
                    match self.app.capture.borrow_mut().stop_track(&context, track_id) {
                        Ok((_, remaining)) => {
                            let event = if remaining.camera || remaining.microphone {
                                MediaCaptureEvent::TrackEnded { track_id }
                            } else {
                                MediaCaptureEvent::Ended
                            };
                            let _ = route.send(event);
                        }
                        Err(_) => {
                            let _ = self.app.capture.borrow_mut().cancel_request(&context);
                            self.app.capture_service.borrow_mut().revoke(ticket);
                            let _ = route
                                .send(MediaCaptureEvent::Error(MediaCaptureError::NotReadable));
                        }
                    }
                }
            }
        }
        let samples = self.app.capture_service.borrow_mut().samples();
        for (ticket, sample) in samples {
            let context = self.app.capture.borrow().active_context(ticket).cloned();
            let Some(context) = context else {
                self.app.capture_service.borrow_mut().revoke(ticket);
                continue;
            };
            let running = self.app.capture.borrow().active_kinds(ticket);
            let permitted = matches!(
                (sample.kind, running),
                (
                    CapturedSampleKind::VideoNv12,
                    Some(CaptureKinds { camera: true, .. })
                ) | (
                    CapturedSampleKind::AudioPcm16,
                    Some(CaptureKinds {
                        microphone: true,
                        ..
                    })
                )
            );
            if !permitted {
                continue;
            }
            let Some(route) = self.live_capture_route(&context) else {
                let _ = self.app.capture.borrow_mut().cancel_request(&context);
                self.app.capture_service.borrow_mut().revoke(ticket);
                continue;
            };
            if !self.capture_foreground(ticket.key().tab) {
                let _ = self.app.capture.borrow_mut().cancel_request(&context);
                self.app.capture_service.borrow_mut().revoke(ticket);
                let _ = route.send(MediaCaptureEvent::Ended);
                continue;
            }
            let frame = frame_for_request(context.key, sample);
            if route.sink.try_send_frame(frame).is_err() {
                let _ = self.app.capture.borrow_mut().cancel_request(&context);
                self.app.capture_service.borrow_mut().revoke(ticket);
                let _ = route.send(MediaCaptureEvent::Error(MediaCaptureError::NotReadable));
            }
        }
    }
}

fn frame_for_request(key: CaptureKey, sample: CapturedSample) -> MediaCaptureFrame {
    MediaCaptureFrame {
        document: key.document,
        request_id: key.request_id,
        track_id: sample.track_id,
        sequence: sample.sequence,
        timestamp_100ns: sample.timestamp_100ns,
        kind: match sample.kind {
            CapturedSampleKind::VideoNv12 => MediaCaptureFrameKind::VideoNv12,
            CapturedSampleKind::AudioPcm16 => MediaCaptureFrameKind::AudioPcm16,
        },
        width_or_rate: sample.width_or_rate,
        height_or_frames: sample.height_or_frames,
        stride_or_channels: sample.stride_or_channels,
        bytes: sample.bytes,
    }
}
