//! Browser-authoritative sensor permission, origin, foreground, and retirement policy.

use super::SensorOwner;
use crate::windows_app::{
    app_state::BrowserState,
    platform::{GetForegroundWindow, IsWindowVisible, MessageBoxW},
    tabs::TabId,
    win32_helpers::wide,
};
use better_web_browser::fetch::Origin;
use better_web_browser::renderer_process::SensorUpdateSink;
use better_web_browser::renderer_protocol::{
    SensorAction, SensorError, SensorEvent, SensorKind, SensorPermission, SensorRequest,
    SensorUpdate,
};

impl BrowserState {
    pub(in crate::windows_app) fn sync_sensor_visibility(&mut self) {
        let tab = self.tabs.active_id();
        if self.benchmark.is_none()
            && unsafe { IsWindowVisible(self.window) } != 0
            && unsafe { GetForegroundWindow() } == self.window
        {
            self.app.sensor_service.set_visible_tab(Some(tab));
        } else {
            self.app.sensor_service.clear_visible_tab(tab);
        }
    }

    pub(in crate::windows_app) fn retire_sensors_for_tab(&mut self, tab_id: TabId) {
        self.app.sensor_service.clear_visible_tab(tab_id);
        let owner = self.tabs.get_mut(tab_id).and_then(|tab| {
            let session_id = tab.renderer_session.as_ref()?.snapshot().session_id;
            let document = tab
                .navigation
                .active_document()
                .or_else(|| tab.navigation.document_id().ok())?;
            Some(SensorOwner {
                tab: tab_id,
                document,
                session_id,
            })
        });
        if let Some(owner) = owner {
            self.app.sensor_service.retire(owner);
        }
    }

    pub(in crate::windows_app) fn retire_sensors_for_window(&mut self) {
        let ids = self.tabs.iter().map(|tab| tab.id).collect::<Vec<_>>();
        for id in ids {
            self.retire_sensors_for_tab(id);
        }
    }

    pub(in crate::windows_app) fn handle_sensor_request(
        &mut self,
        tab_id: TabId,
        request: SensorRequest,
    ) {
        if request.validate().is_err() {
            return;
        }
        let Some((sink, session_id, owner)) = self.tabs.get_mut(tab_id).and_then(|tab| {
            if !tab.navigation.owns_document(request.document) {
                return None;
            }
            let session = tab.renderer_session.as_ref()?;
            Some((
                session.sensor_update_sink(request.document),
                session.snapshot().session_id,
                tab.renderer_fetches.resolve_client(
                    request.document,
                    &tab.reader_url,
                    request.client,
                ),
            ))
        }) else {
            return;
        };
        let Ok(owner) = owner else {
            reject(&sink, &request);
            return;
        };
        // Until Permissions Policy is implemented for sensors, descendants cannot inherit
        // a top-level permission or activate a device. The effective origin also preserves
        // sandboxed-frame opacity; a URL string alone is not an authorization source.
        if request.client.id != 0 || !trustworthy_sensor_origin(&owner.origin) {
            reject(&sink, &request);
            return;
        }
        let origin = owner.origin.serialize();
        let eligible = self.benchmark.is_none()
            && self.tabs.active_id() == tab_id
            && unsafe { IsWindowVisible(self.window) } != 0
            && unsafe { GetForegroundWindow() } == self.window;
        let sensor_owner = SensorOwner {
            tab: tab_id,
            document: request.document,
            session_id,
        };
        match request.action {
            SensorAction::RequestPermission { kind, absolute } => {
                let permission = if absolute
                    || !matches!(kind, SensorKind::Orientation | SensorKind::Motion)
                {
                    SensorPermission::Denied
                } else if let Some(permission) = self.app.sensor_service.permission(&origin, kind) {
                    permission
                } else if eligible
                    && request.user_activation
                    && self.has_transient_activation(tab_id, request.document)
                {
                    self.prompt_sensor_permission(&origin, kind)
                } else {
                    SensorPermission::Denied
                };
                emit(&sink, &request, SensorEvent::Permission(permission));
            }
            SensorAction::Start { kind, .. } => {
                let mut permission = self.app.sensor_service.permission(&origin, kind);
                if permission.is_none()
                    && matches!(
                        kind,
                        SensorKind::Accelerometer
                            | SensorKind::Gyroscope
                            | SensorKind::Magnetometer
                            | SensorKind::AbsoluteOrientation
                            | SensorKind::RelativeOrientation
                            | SensorKind::AmbientLight
                    )
                    && eligible
                {
                    permission = Some(self.prompt_sensor_permission(&origin, kind));
                }
                if !eligible || permission != Some(SensorPermission::Granted) {
                    emit(&sink, &request, SensorEvent::Error(SensorError::NotAllowed));
                } else {
                    self.sync_sensor_visibility();
                    let request_id = request.request_id;
                    if self
                        .app
                        .sensor_service
                        .send(sensor_owner, request, sink.clone())
                        .is_err()
                    {
                        let _ = sink.try_send(SensorUpdate {
                            document: sensor_owner.document,
                            request_id,
                            event: SensorEvent::Error(SensorError::NotReadable),
                        });
                    }
                }
            }
            SensorAction::Stop { .. } => {
                let _ = self.app.sensor_service.send(sensor_owner, request, sink);
            }
        }
    }

    fn prompt_sensor_permission(&mut self, origin: &str, kind: SensorKind) -> SensorPermission {
        let capability = match kind {
            SensorKind::Accelerometer => "accelerometer",
            SensorKind::Gyroscope => "gyroscope",
            SensorKind::Magnetometer => "magnetometer",
            SensorKind::AbsoluteOrientation => "accelerometer, gyroscope, and magnetometer",
            SensorKind::RelativeOrientation => "accelerometer and gyroscope",
            SensorKind::AmbientLight => "ambient light",
            SensorKind::Orientation | SensorKind::Motion => "motion and orientation",
        };
        let question = wide(&format!(
            "Allow {origin} to read this device's {capability} sensors for this browser session?"
        ));
        let title = wide("Breeze sensor permission");
        // Win32 MB_YESNO | MB_ICONQUESTION. Only an active, visible interactive tab reaches here.
        let permission =
            if unsafe { MessageBoxW(self.window, question.as_ptr(), title.as_ptr(), 0x24) } == 6 {
                SensorPermission::Granted
            } else {
                SensorPermission::Denied
            };
        self.app
            .sensor_service
            .decide(origin.into(), kind, permission);
        permission
    }
}

fn trustworthy_sensor_origin(origin: &Origin) -> bool {
    if origin.is_secure() {
        return true;
    }
    // Numeric loopback is a potentially trustworthy origin. Domain localhost is
    // intentionally excluded until the transport guarantees loopback-only resolution.
    let serialized = origin.serialize();
    let Ok(url) = url::Url::parse(&serialized) else {
        return false;
    };
    if url.scheme() != "http" {
        return false;
    }
    match url.host() {
        Some(url::Host::Ipv4(address)) => address.is_loopback(),
        Some(url::Host::Ipv6(address)) => address.is_loopback(),
        _ => false,
    }
}

fn emit(sink: &SensorUpdateSink, request: &SensorRequest, event: SensorEvent) {
    let _ = sink.try_send(SensorUpdate {
        document: request.document,
        request_id: request.request_id,
        event,
    });
}

fn reject(sink: &SensorUpdateSink, request: &SensorRequest) {
    let event = match request.action {
        SensorAction::RequestPermission { .. } => SensorEvent::Permission(SensorPermission::Denied),
        _ => SensorEvent::Error(SensorError::NotAllowed),
    };
    emit(sink, request, event);
}
