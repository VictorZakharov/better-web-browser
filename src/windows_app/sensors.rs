//! Browser-owned physical sensors: permission remains in the UI process, WinRT in one worker.

mod dispatch;
mod native;
mod worker;

use super::tabs::TabId;
use better_web_browser::renderer_process::{SensorDeliveryGate, SensorUpdateSink};
use better_web_browser::renderer_protocol::{
    DocumentId, SensorKind, SensorPermission, SensorRequest,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

const MAX_SENSOR_COMMANDS: usize = 64;

fn clear_visible_tab_gate(visible_tab: &AtomicU64, tab: TabId) {
    let _ = visible_tab.compare_exchange(tab.get(), 0, Ordering::AcqRel, Ordering::Relaxed);
}

#[derive(Default)]
struct OriginPermissions {
    accelerometer: Option<SensorPermission>,
    gyroscope: Option<SensorPermission>,
    magnetometer: Option<SensorPermission>,
    ambient_light: Option<SensorPermission>,
}

impl OriginPermissions {
    fn for_kind(&self, kind: SensorKind) -> Option<SensorPermission> {
        match kind {
            SensorKind::Accelerometer | SensorKind::LinearAcceleration | SensorKind::Gravity => {
                self.accelerometer
            }
            SensorKind::Gyroscope => self.gyroscope,
            SensorKind::Magnetometer => self.magnetometer,
            SensorKind::AmbientLight => self.ambient_light,
            SensorKind::Orientation | SensorKind::Motion | SensorKind::RelativeOrientation => {
                if self.accelerometer == Some(SensorPermission::Denied)
                    || self.gyroscope == Some(SensorPermission::Denied)
                {
                    Some(SensorPermission::Denied)
                } else if self.accelerometer == Some(SensorPermission::Granted)
                    && self.gyroscope == Some(SensorPermission::Granted)
                {
                    Some(SensorPermission::Granted)
                } else {
                    None
                }
            }
            SensorKind::AbsoluteOrientation | SensorKind::OrientationAbsoluteLegacy => {
                if [self.accelerometer, self.gyroscope, self.magnetometer]
                    .contains(&Some(SensorPermission::Denied))
                {
                    Some(SensorPermission::Denied)
                } else if [self.accelerometer, self.gyroscope, self.magnetometer]
                    .iter()
                    .all(|permission| *permission == Some(SensorPermission::Granted))
                {
                    Some(SensorPermission::Granted)
                } else {
                    None
                }
            }
        }
    }

    fn decide(&mut self, kind: SensorKind, permission: SensorPermission) {
        match kind {
            SensorKind::Accelerometer | SensorKind::LinearAcceleration | SensorKind::Gravity => {
                self.accelerometer = Some(permission);
            }
            SensorKind::Gyroscope => self.gyroscope = Some(permission),
            SensorKind::Magnetometer => self.magnetometer = Some(permission),
            SensorKind::AmbientLight => self.ambient_light = Some(permission),
            SensorKind::Orientation | SensorKind::Motion | SensorKind::RelativeOrientation => {
                if permission == SensorPermission::Granted {
                    self.accelerometer = Some(permission);
                    self.gyroscope = Some(permission);
                } else {
                    // Declining a combined request must not revoke a separately
                    // granted Generic Accelerometer or Gyroscope permission.
                    self.accelerometer.get_or_insert(permission);
                    self.gyroscope.get_or_insert(permission);
                }
            }
            SensorKind::AbsoluteOrientation | SensorKind::OrientationAbsoluteLegacy => {
                if permission == SensorPermission::Granted {
                    self.accelerometer = Some(permission);
                    self.gyroscope = Some(permission);
                    self.magnetometer = Some(permission);
                } else {
                    // A declined combined prompt cannot revoke an earlier grant.
                    self.accelerometer.get_or_insert(permission);
                    self.gyroscope.get_or_insert(permission);
                    self.magnetometer.get_or_insert(permission);
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SensorOwner {
    tab: TabId,
    document: DocumentId,
    session_id: u64,
}

pub(super) struct SensorService {
    commands: SyncSender<worker::Command>,
    permissions: Mutex<HashMap<String, OriginPermissions>>,
    latest_owner: Mutex<HashMap<TabId, SensorOwner>>,
    retired: Arc<Mutex<HashMap<TabId, (u64, u64)>>>,
    visible_tab: Arc<AtomicU64>,
    delivery_gate: SensorDeliveryGate,
    stopping: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl SensorService {
    pub(super) fn spawn() -> Result<Self, String> {
        let (commands, incoming) = mpsc::sync_channel(MAX_SENSOR_COMMANDS);
        let retired = Arc::new(Mutex::new(HashMap::new()));
        let visible_tab = Arc::new(AtomicU64::new(0));
        let stopping = Arc::new(AtomicBool::new(false));
        let worker_retired = Arc::clone(&retired);
        let worker_visible = Arc::clone(&visible_tab);
        let worker_stopping = Arc::clone(&stopping);
        let worker = thread::Builder::new()
            .name("breeze-sensors".into())
            .spawn(move || {
                native::run_native_worker(incoming, worker_retired, worker_visible, worker_stopping)
            })
            .map_err(|error| format!("start sensor worker: {error}"))?;
        Ok(Self {
            commands,
            permissions: Mutex::new(HashMap::new()),
            latest_owner: Mutex::new(HashMap::new()),
            retired,
            visible_tab,
            delivery_gate: SensorDeliveryGate::default(),
            stopping,
            worker: Some(worker),
        })
    }

    fn permission(&self, origin: &str, kind: SensorKind) -> Option<SensorPermission> {
        self.permissions
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .get(origin)
            .and_then(|state| state.for_kind(kind))
    }

    fn decide(&self, origin: String, kind: SensorKind, permission: SensorPermission) {
        self.permissions
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .entry(origin)
            .or_default()
            .decide(kind, permission);
    }

    fn send(
        &self,
        owner: SensorOwner,
        request: SensorRequest,
        sink: SensorUpdateSink,
    ) -> Result<(), String> {
        request.validate().map_err(|error| error.to_string())?;
        if request.document != owner.document || worker::is_retired(&self.retired, owner) {
            return Err("sensor document has retired".into());
        }
        self.latest_owner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .entry(owner.tab)
            .and_modify(|latest| {
                if (owner.session_id, owner.document.get())
                    > (latest.session_id, latest.document.get())
                {
                    *latest = owner;
                }
            })
            .or_insert(owner);
        self.commands
            .try_send(worker::Command::Request(
                owner,
                request,
                sink.with_delivery_gate(self.delivery_gate.clone(), owner.tab.get()),
            ))
            .map_err(|error| format!("sensor worker command queue: {error}"))
    }

    fn set_visible_tab(&self, tab: Option<TabId>) {
        self.delivery_gate
            .set_visible_tab(tab.map_or(0, TabId::get));
        self.visible_tab
            .store(tab.map_or(0, TabId::get), Ordering::Release);
    }

    fn clear_visible_tab(&self, tab: TabId) {
        self.delivery_gate.clear_visible_tab(tab.get());
        clear_visible_tab_gate(&self.visible_tab, tab);
    }

    fn retire_tab(&self, tab: TabId, current: Option<SensorOwner>) {
        self.clear_visible_tab(tab);
        let latest = self
            .latest_owner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .remove(&tab);
        let owner = [current, latest]
            .into_iter()
            .flatten()
            .max_by_key(|owner| (owner.session_id, owner.document.get()));
        if let Some(owner) = owner {
            let mut retired = self
                .retired
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            retired
                .entry(tab)
                .and_modify(|latest| {
                    *latest = (*latest).max((owner.session_id, owner.document.get()))
                })
                .or_insert((owner.session_id, owner.document.get()));
            drop(retired);
        }
        let _ = self.commands.try_send(worker::Command::RetireTab(tab));
    }
}

impl Drop for SensorService {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        let _ = self.commands.try_send(worker::Command::Shutdown);
        // The worker owns WinRT objects. Do not block browser shutdown on a faulty sensor driver.
        self.worker.take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switching_tabs_revokes_only_the_old_visible_sensor_gate() {
        let old = TabId::first();
        let next = TabId::allocate();
        let visible = AtomicU64::new(old.get());
        clear_visible_tab_gate(&visible, next);
        assert_eq!(visible.load(Ordering::Acquire), old.get());
        clear_visible_tab_gate(&visible, old);
        assert_eq!(visible.load(Ordering::Acquire), 0);
        visible.store(next.get(), Ordering::Release);
        clear_visible_tab_gate(&visible, old);
        assert_eq!(visible.load(Ordering::Acquire), next.get());
    }

    #[test]
    fn renderer_crash_retires_last_known_owner_even_after_session_is_gone() {
        let owner = SensorOwner {
            tab: TabId::first(),
            document: DocumentId::new(17).unwrap(),
            session_id: 8,
        };
        let (commands, incoming) = mpsc::sync_channel(1);
        let retired = Arc::new(Mutex::new(HashMap::new()));
        let service = SensorService {
            commands,
            permissions: Mutex::new(HashMap::new()),
            latest_owner: Mutex::new(HashMap::from([(owner.tab, owner)])),
            retired: Arc::clone(&retired),
            visible_tab: Arc::new(AtomicU64::new(owner.tab.get())),
            delivery_gate: SensorDeliveryGate::default(),
            stopping: Arc::new(AtomicBool::new(false)),
            worker: None,
        };
        service.retire_tab(owner.tab, None);
        assert_eq!(service.visible_tab.load(Ordering::Acquire), 0);
        assert_eq!(
            retired.lock().unwrap().get(&owner.tab),
            Some(&(owner.session_id, owner.document.get()))
        );
        assert!(matches!(
            incoming.try_recv(),
            Ok(worker::Command::RetireTab(tab)) if tab == owner.tab
        ));
    }

    #[test]
    fn generic_accelerometer_permission_does_not_grant_gyroscope() {
        let mut permissions = OriginPermissions::default();
        permissions.decide(SensorKind::Accelerometer, SensorPermission::Granted);
        assert_eq!(
            permissions.for_kind(SensorKind::Accelerometer),
            Some(SensorPermission::Granted)
        );
        assert_eq!(
            permissions.for_kind(SensorKind::LinearAcceleration),
            Some(SensorPermission::Granted)
        );
        assert_eq!(
            permissions.for_kind(SensorKind::Gravity),
            Some(SensorPermission::Granted)
        );
        assert_eq!(permissions.for_kind(SensorKind::Gyroscope), None);
        assert_eq!(permissions.for_kind(SensorKind::Motion), None);
        assert_eq!(permissions.for_kind(SensorKind::RelativeOrientation), None);
        permissions.decide(SensorKind::Gyroscope, SensorPermission::Granted);
        assert_eq!(
            permissions.for_kind(SensorKind::Orientation),
            Some(SensorPermission::Granted)
        );
        assert_eq!(
            permissions.for_kind(SensorKind::RelativeOrientation),
            Some(SensorPermission::Granted)
        );
        let mut declined = OriginPermissions::default();
        declined.decide(SensorKind::Accelerometer, SensorPermission::Granted);
        declined.decide(SensorKind::Motion, SensorPermission::Denied);
        assert_eq!(
            declined.for_kind(SensorKind::Accelerometer),
            Some(SensorPermission::Granted)
        );
        assert_eq!(
            declined.for_kind(SensorKind::Gyroscope),
            Some(SensorPermission::Denied)
        );
    }

    #[test]
    fn absolute_orientation_requires_all_three_independent_capabilities() {
        let mut permissions = OriginPermissions::default();
        permissions.decide(SensorKind::Accelerometer, SensorPermission::Granted);
        permissions.decide(SensorKind::Gyroscope, SensorPermission::Granted);
        assert_eq!(permissions.for_kind(SensorKind::AbsoluteOrientation), None);
        assert_eq!(
            permissions.for_kind(SensorKind::OrientationAbsoluteLegacy),
            None
        );
        permissions.decide(SensorKind::Magnetometer, SensorPermission::Granted);
        assert_eq!(
            permissions.for_kind(SensorKind::AbsoluteOrientation),
            Some(SensorPermission::Granted)
        );
        assert_eq!(
            permissions.for_kind(SensorKind::OrientationAbsoluteLegacy),
            Some(SensorPermission::Granted)
        );
        let mut declined = OriginPermissions::default();
        declined.decide(SensorKind::Magnetometer, SensorPermission::Granted);
        declined.decide(SensorKind::AbsoluteOrientation, SensorPermission::Denied);
        assert_eq!(
            declined.for_kind(SensorKind::Magnetometer),
            Some(SensorPermission::Granted)
        );
        assert_eq!(
            declined.for_kind(SensorKind::AbsoluteOrientation),
            Some(SensorPermission::Denied)
        );

        let mut legacy = OriginPermissions::default();
        legacy.decide(
            SensorKind::OrientationAbsoluteLegacy,
            SensorPermission::Granted,
        );
        for kind in [
            SensorKind::Accelerometer,
            SensorKind::Gyroscope,
            SensorKind::Magnetometer,
            SensorKind::AbsoluteOrientation,
            SensorKind::OrientationAbsoluteLegacy,
        ] {
            assert_eq!(legacy.for_kind(kind), Some(SensorPermission::Granted));
        }
    }

    #[test]
    fn ambient_light_permission_does_not_grant_motion_capabilities() {
        let mut permissions = OriginPermissions::default();
        permissions.decide(SensorKind::AmbientLight, SensorPermission::Granted);
        assert_eq!(
            permissions.for_kind(SensorKind::AmbientLight),
            Some(SensorPermission::Granted)
        );
        assert_eq!(permissions.for_kind(SensorKind::Accelerometer), None);
        assert_eq!(permissions.for_kind(SensorKind::RelativeOrientation), None);
    }
}
