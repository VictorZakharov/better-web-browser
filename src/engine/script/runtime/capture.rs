//! Renderer-local live capture element lookup and media timing notifications.

use super::*;
use crate::renderer_protocol::{MediaCaptureEvent, MediaCaptureFrame, MediaCaptureUpdate};

impl ScriptRuntime {
    pub fn deliver_media_capture_update(&mut self, update: MediaCaptureUpdate) -> ScriptOutcome {
        let id = update.request_id as u32;
        if let Some(child) = self.child_for_fetch(id) {
            let owner = child.host.borrow().document.id();
            let outcome = child.deliver_media_capture_update(update);
            let outcome = self.collect_frame_result(owner, outcome);
            return self.finish_guarded_run(Ok(outcome));
        }
        if !self.initialized {
            return lifecycle_error("the document's initial scripts have not executed");
        }
        let Some(context) = self.context.as_deref_mut() else {
            return inactive_runtime_outcome();
        };
        let terminal = matches!(
            update.event,
            MediaCaptureEvent::Ended | MediaCaptureEvent::Error(_)
        );
        let host = Rc::clone(&self.host);
        if let MediaCaptureEvent::TrackEnded { track_id } = update.event {
            host.borrow_mut()
                .capture
                .end_track(update.request_id, track_id);
        }
        let result = catch_unwind(AssertUnwindSafe(|| {
            host.borrow_mut().begin_task();
            let mut outcome = ScriptOutcome::default();
            let started = Instant::now();
            if let Err(error) =
                super::super::host_call::media_devices::deliver_capture_event(context, &update)
            {
                outcome
                    .errors
                    .push(format!("MediaCapture callback: {error}"));
            }
            super::module_lifecycle::drain(context, &host, &mut outcome);
            outcome.record_timing("JavaScript MediaCapture callback", started.elapsed());
            outcome
        }));
        if terminal {
            self.host.borrow().fetch_identifiers.borrow_mut().finish(id);
            self.host.borrow_mut().capture.retire(update.request_id);
        }
        self.finish_guarded_run(result)
    }

    pub(crate) fn capture_video_nodes(&self, request_id: u64) -> Vec<(NodeId, bool)> {
        self.host.borrow().capture.video_nodes(request_id)
    }

    pub(crate) fn deliver_media_capture_frame_info(
        &mut self,
        request_id: u64,
        width: u32,
        height: u32,
        timestamp_100ns: u64,
    ) -> ScriptOutcome {
        if !self.initialized {
            return ScriptOutcome::default();
        }
        let Some(context) = self.context.as_deref_mut() else {
            return ScriptOutcome::default();
        };
        let host = Rc::clone(&self.host);
        let result = catch_unwind(AssertUnwindSafe(|| {
            host.borrow_mut().begin_task();
            let mut outcome = ScriptOutcome::default();
            let started = Instant::now();
            let payload = serde_json::json!({
                "id": request_id,
                "width": width,
                "height": height,
                "time": timestamp_100ns as f64 / 10_000_000.0,
            });
            if let Err(error) = context.call_private_hook(
                "__receiveMediaCaptureFrame",
                &[JsValue::from(payload.to_string())],
            ) {
                outcome
                    .errors
                    .push(format!("MediaCapture frame callback: {error}"));
            }
            super::super::module_lifecycle::drain(context, &host, &mut outcome);
            outcome.record_timing("JavaScript MediaCapture frame callback", started.elapsed());
            outcome
        }));
        self.finish_guarded_run(result)
    }

    pub(crate) fn deliver_media_capture_audio_frame(
        &mut self,
        frame: MediaCaptureFrame,
    ) -> ScriptOutcome {
        if !self.initialized {
            return ScriptOutcome::default();
        }
        let Some(context) = self.context.as_deref_mut() else {
            return ScriptOutcome::default();
        };
        let host = Rc::clone(&self.host);
        let result = catch_unwind(AssertUnwindSafe(|| {
            host.borrow_mut().begin_task();
            let mut outcome = ScriptOutcome::default();
            let started = Instant::now();
            if let Err(error) = context.call_private_hook(
                "__receiveMediaCaptureAudioFrame",
                &[
                    JsValue::Number(frame.request_id as f64),
                    JsValue::Number(frame.sequence as f64),
                    JsValue::Number(frame.timestamp_100ns as f64),
                    JsValue::Number(f64::from(frame.width_or_rate)),
                    JsValue::Number(f64::from(frame.stride_or_channels)),
                    JsValue::Number(f64::from(frame.height_or_frames)),
                    JsValue::Bytes(frame.bytes),
                ],
            ) {
                outcome
                    .errors
                    .push(format!("MediaCapture audio callback: {error}"));
            }
            super::module_lifecycle::drain(context, &host, &mut outcome);
            outcome.record_timing("JavaScript MediaCapture audio callback", started.elapsed());
            outcome
        }));
        self.finish_guarded_run(result)
    }
}
