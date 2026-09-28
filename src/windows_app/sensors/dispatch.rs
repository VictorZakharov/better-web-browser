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
    DocumentId, SensorAction, SensorError, SensorEvent, SensorKind, SensorPermission,
    SensorRequest, SensorUpdate,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    IsIconic, MB_DEFBUTTON2, MB_ICONQUESTION, MB_YESNO, SIZE_MINIMIZED,
};

const SENSOR_PROMPT_FLAGS: u32 = MB_YESNO | MB_ICONQUESTION | MB_DEFBUTTON2;

#[derive(Clone, Debug, PartialEq, Eq)]
struct SensorAuthority {
    tab: TabId,
    document: DocumentId,
    session_id: u64,
    origin: String,
}

fn same_sensor_authority(
    expected: &SensorAuthority,
    current: Option<&SensorAuthority>,
    foreground: bool,
) -> bool {
    foreground && current == Some(expected)
}

impl BrowserState {
    fn sensor_foreground(&self, tab: TabId) -> bool {
        self.benchmark.is_none()
            && self.tabs.active_id() == tab
            && unsafe { IsWindowVisible(self.window) } != 0
            && unsafe { IsIconic(self.window) } == 0
            && unsafe { GetForegroundWindow() } == self.window
    }

    pub(in crate::windows_app) fn sync_sensor_visibility(&mut self) {
        let tab = self.tabs.active_id();
        if self.sensor_foreground(tab) {
            self.app.sensor_service.set_visible_tab(Some(tab));
        } else {
            self.app.sensor_service.clear_visible_tab(tab);
        }
    }

    pub(in crate::windows_app) fn clear_visible_sensor_tab(&self, tab: TabId) {
        self.app.sensor_service.clear_visible_tab(tab);
    }

    pub(in crate::windows_app) fn handle_sensor_window_size(&mut self, reason: usize) {
        if reason == SIZE_MINIMIZED as usize {
            self.clear_visible_sensor_tab(self.tabs.active_id());
        } else {
            self.sync_sensor_visibility();
        }
    }

    pub(in crate::windows_app) fn retire_sensors_for_tab(&mut self, tab_id: TabId) {
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
        self.app.sensor_service.retire_tab(tab_id, owner);
    }

    pub(in crate::windows_app) fn retire_sensors_for_window(&mut self) {
        let ids = self.tabs.iter().map(|tab| tab.id).collect::<Vec<_>>();
        for id in ids {
            self.retire_sensors_for_tab(id);
        }
    }

    fn current_sensor_authority(
        &mut self,
        tab_id: TabId,
        request: &SensorRequest,
    ) -> Option<SensorAuthority> {
        let tab = self.tabs.get_mut(tab_id)?;
        if !tab.navigation.owns_document(request.document) || request.client.id != 0 {
            return None;
        }
        let session_id = tab.renderer_session.as_ref()?.snapshot().session_id;
        let owner = tab
            .renderer_fetches
            .resolve_client(request.document, &tab.reader_url, request.client)
            .ok()?;
        trustworthy_sensor_origin(&owner.origin).then(|| SensorAuthority {
            tab: tab_id,
            document: request.document,
            session_id,
            origin: owner.origin.serialize(),
        })
    }

    fn sensor_authority_is_current(
        &mut self,
        expected: &SensorAuthority,
        request: &SensorRequest,
    ) -> bool {
        let foreground = self.sensor_foreground(expected.tab);
        let current = self.current_sensor_authority(expected.tab, request);
        same_sensor_authority(expected, current.as_ref(), foreground)
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
        let eligible = self.sensor_foreground(tab_id);
        let sensor_owner = SensorOwner {
            tab: tab_id,
            document: request.document,
            session_id,
        };
        let authority = SensorAuthority {
            tab: tab_id,
            document: request.document,
            session_id,
            origin: origin.clone(),
        };
        match request.action {
            SensorAction::RequestPermission { kind, absolute } => {
                let permission_kind = if absolute {
                    SensorKind::OrientationAbsoluteLegacy
                } else {
                    kind
                };
                if !matches!(kind, SensorKind::Orientation | SensorKind::Motion) {
                    emit(
                        &sink,
                        &request,
                        SensorEvent::Permission(SensorPermission::Denied),
                    );
                } else if let Some(permission) =
                    self.app.sensor_service.permission(&origin, permission_kind)
                {
                    emit(&sink, &request, SensorEvent::Permission(permission));
                } else if eligible
                    && request.user_activation
                    && self.has_transient_activation(tab_id, request.document)
                {
                    let permission = self.prompt_sensor_permission(&origin, permission_kind);
                    if self.sensor_authority_is_current(&authority, &request) {
                        self.app
                            .sensor_service
                            .decide(origin.clone(), permission_kind, permission);
                        emit(&sink, &request, SensorEvent::Permission(permission));
                    } else {
                        emit(&sink, &request, SensorEvent::Error(SensorError::NotAllowed));
                    }
                } else {
                    // W3C Device Orientation §6: prompt state without transient
                    // activation rejects rather than resolving to "denied".
                    emit(&sink, &request, SensorEvent::Error(SensorError::NotAllowed));
                }
            }
            SensorAction::Start { kind, .. } => {
                let mut permission = self.app.sensor_service.permission(&origin, kind);
                if permission.is_none()
                    && matches!(
                        kind,
                        SensorKind::Accelerometer
                            | SensorKind::LinearAcceleration
                            | SensorKind::Gravity
                            | SensorKind::Gyroscope
                            | SensorKind::Magnetometer
                            | SensorKind::AbsoluteOrientation
                            | SensorKind::RelativeOrientation
                            | SensorKind::AmbientLight
                    )
                    && eligible
                {
                    let prompted = self.prompt_sensor_permission(&origin, kind);
                    if !self.sensor_authority_is_current(&authority, &request) {
                        emit(&sink, &request, SensorEvent::Error(SensorError::NotAllowed));
                        return;
                    }
                    self.app
                        .sensor_service
                        .decide(origin.clone(), kind, prompted);
                    permission = Some(prompted);
                }
                if !self.sensor_authority_is_current(&authority, &request)
                    || permission != Some(SensorPermission::Granted)
                {
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
            SensorKind::LinearAcceleration => "linear accelerometer",
            SensorKind::Gravity => "gravity accelerometer",
            SensorKind::Gyroscope => "gyroscope",
            SensorKind::Magnetometer => "magnetometer",
            SensorKind::AbsoluteOrientation => "accelerometer, gyroscope, and magnetometer",
            SensorKind::OrientationAbsoluteLegacy => "accelerometer, gyroscope, and magnetometer",
            SensorKind::RelativeOrientation => "accelerometer and gyroscope",
            SensorKind::AmbientLight => "ambient light",
            SensorKind::Orientation | SensorKind::Motion => "motion and orientation",
        };
        let question = wide(&format!(
            "Allow {origin} to read this device's {capability} sensors for this browser session?"
        ));
        let title = wide("Breeze sensor permission");
        // The default button is No, so Enter never silently grants access.
        // Only an active, visible interactive tab reaches this dialog.
        if unsafe {
            MessageBoxW(
                self.window,
                question.as_ptr(),
                title.as_ptr(),
                SENSOR_PROMPT_FLAGS,
            )
        } == 6
        {
            SensorPermission::Granted
        } else {
            SensorPermission::Denied
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_permission_prompt_defaults_to_no() {
        assert_eq!(SENSOR_PROMPT_FLAGS & MB_YESNO, MB_YESNO);
        assert_eq!(SENSOR_PROMPT_FLAGS & MB_DEFBUTTON2, MB_DEFBUTTON2);
    }

    #[test]
    fn only_secure_and_numeric_loopback_origins_may_request_physical_sensors() {
        for url in [
            "https://example.test/page",
            "http://127.0.0.1:8080/page",
            "http://[::1]:8080/page",
        ] {
            assert!(
                trustworthy_sensor_origin(&Origin::parse(url).unwrap()),
                "{url}"
            );
        }
        for url in ["http://example.test/page", "http://localhost/page"] {
            assert!(
                !trustworthy_sensor_origin(&Origin::parse(url).unwrap()),
                "{url}"
            );
        }
    }

    #[test]
    fn modal_permission_result_cannot_authorize_a_replaced_document_or_background_tab() {
        let expected = SensorAuthority {
            tab: TabId::first(),
            document: DocumentId::new(12).unwrap(),
            session_id: 4,
            origin: "https://example.test".into(),
        };
        assert!(same_sensor_authority(&expected, Some(&expected), true));
        assert!(!same_sensor_authority(&expected, Some(&expected), false));
        assert!(!same_sensor_authority(&expected, None, true));
        for changed in [
            SensorAuthority {
                session_id: 5,
                ..expected.clone()
            },
            SensorAuthority {
                document: DocumentId::new(13).unwrap(),
                ..expected.clone()
            },
            SensorAuthority {
                origin: "https://other.test".into(),
                ..expected.clone()
            },
        ] {
            assert!(!same_sensor_authority(&expected, Some(&changed), true));
        }
    }
}
