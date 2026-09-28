//! Fake provider integration: permissioned streams carry physical shapes without OS calls.

use super::*;
use better_web_browser::fetch::RequestClient;
use better_web_browser::renderer_protocol::DocumentId;
use std::sync::atomic::AtomicUsize;

struct FakeExtendedProvider {
    samples: Arc<AtomicUsize>,
}

impl SensorProvider for FakeExtendedProvider {
    fn supported(&mut self, kind: SensorKind) -> Result<bool, SensorError> {
        Ok(matches!(
            kind,
            SensorKind::LinearAcceleration
                | SensorKind::Gravity
                | SensorKind::Magnetometer
                | SensorKind::AbsoluteOrientation
                | SensorKind::OrientationAbsoluteLegacy
                | SensorKind::RelativeOrientation
                | SensorKind::AmbientLight
        ))
    }

    fn set_active(&mut self, _: bool) {}

    fn sample(&mut self, kind: SensorKind, _: f64) -> Result<Option<Sample>, SensorError> {
        let stamp = self.samples.fetch_add(1, Ordering::Relaxed) as i64 + 1;
        let reading = match kind {
            SensorKind::LinearAcceleration => SensorReading::ThreeAxis {
                x: 1.2,
                y: -2.4,
                z: 0.3,
                timestamp_ms: stamp as f64,
            },
            SensorKind::Gravity => SensorReading::ThreeAxis {
                x: 0.0,
                y: 0.0,
                z: -9.8,
                timestamp_ms: stamp as f64,
            },
            SensorKind::Magnetometer => SensorReading::ThreeAxis {
                x: 42.5,
                y: -17.2,
                z: 8.1,
                timestamp_ms: stamp as f64,
            },
            SensorKind::AbsoluteOrientation => SensorReading::Quaternion {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                w: 1.0,
                timestamp_ms: stamp as f64,
            },
            SensorKind::OrientationAbsoluteLegacy => SensorReading::Orientation {
                alpha: Some(181.2),
                beta: Some(-2.3),
                gamma: Some(4.5),
                absolute: true,
            },
            SensorKind::RelativeOrientation => SensorReading::Quaternion {
                x: 0.0,
                y: 0.0,
                z: std::f64::consts::FRAC_1_SQRT_2,
                w: std::f64::consts::FRAC_1_SQRT_2,
                timestamp_ms: stamp as f64,
            },
            SensorKind::AmbientLight => SensorReading::Illuminance {
                illuminance: 100.0,
                timestamp_ms: stamp as f64,
            },
            _ => unreachable!("unsupported fake kind"),
        };
        Ok(Some(Sample {
            stamp,
            reading,
            raw_light_lux: (kind == SensorKind::AmbientLight).then_some(81.0),
        }))
    }
}

fn request(owner: SensorOwner, id: u64, kind: SensorKind) -> SensorRequest {
    SensorRequest {
        document: owner.document,
        request_id: id,
        client: RequestClient::default(),
        user_activation: false,
        action: SensorAction::Start {
            kind,
            frequency_hz: Some(50.0),
        },
    }
}

#[test]
fn extended_vector_orientation_and_light_streams_use_fake_provider() {
    let owner = SensorOwner {
        tab: TabId::first(),
        document: DocumentId::new(7).unwrap(),
        session_id: 9,
    };
    let retired = Arc::new(Mutex::new(HashMap::new()));
    let visible = Arc::new(AtomicU64::new(owner.tab.get()));
    let stopping = Arc::new(AtomicBool::new(false));
    let samples = Arc::new(AtomicUsize::new(0));
    let fake = FakeExtendedProvider {
        samples: Arc::clone(&samples),
    };
    let (commands, incoming) = mpsc::channel();
    let (updates, received) = mpsc::channel();
    let thread = std::thread::spawn(move || run(incoming, retired, visible, stopping, fake));
    for (id, kind) in [
        (1, SensorKind::LinearAcceleration),
        (2, SensorKind::Gravity),
        (3, SensorKind::Magnetometer),
        (4, SensorKind::AbsoluteOrientation),
        (5, SensorKind::OrientationAbsoluteLegacy),
        (6, SensorKind::RelativeOrientation),
        (7, SensorKind::AmbientLight),
    ] {
        commands
            .send(Command::Request(
                owner,
                request(owner, id, kind),
                updates.clone(),
            ))
            .unwrap();
        let mut activated = false;
        let mut reading = None;
        for _ in 0..8 {
            let update = received.recv_timeout(Duration::from_secs(1)).unwrap();
            if update.request_id != id {
                continue;
            }
            match update.event {
                SensorEvent::Activated => activated = true,
                SensorEvent::Reading(value) => {
                    reading = Some(value);
                    break;
                }
                event => panic!("unexpected sensor update: {event:?}"),
            }
        }
        assert!(activated);
        let reading = reading.expect("fake reading delivered");
        assert!(match kind {
            SensorKind::LinearAcceleration => {
                matches!(
                    reading,
                    SensorReading::ThreeAxis {
                        x: 1.2,
                        y: -2.4,
                        ..
                    }
                )
            }
            SensorKind::Gravity => {
                matches!(reading, SensorReading::ThreeAxis { z: -9.8, .. })
            }
            SensorKind::Magnetometer => matches!(reading, SensorReading::ThreeAxis { x: 42.5, .. }),
            SensorKind::AbsoluteOrientation => {
                matches!(reading, SensorReading::Quaternion { w: 1.0, .. })
            }
            SensorKind::OrientationAbsoluteLegacy => {
                matches!(
                    reading,
                    SensorReading::Orientation {
                        alpha: Some(181.2),
                        absolute: true,
                        ..
                    }
                )
            }
            SensorKind::RelativeOrientation => {
                matches!(reading, SensorReading::Quaternion { z, .. } if z > 0.7)
            }
            SensorKind::AmbientLight => {
                matches!(
                    reading,
                    SensorReading::Illuminance {
                        illuminance: 100.0,
                        ..
                    }
                )
            }
            _ => false,
        });
        commands
            .send(Command::Request(
                owner,
                SensorRequest {
                    action: SensorAction::Stop { kind },
                    ..request(owner, id, kind)
                },
                updates.clone(),
            ))
            .unwrap();
    }
    commands.send(Command::Shutdown).unwrap();
    thread.join().unwrap();
    assert!(samples.load(Ordering::Relaxed) > 0);
}

#[test]
fn light_threshold_blocks_small_raw_fluctuations_and_same_quantized_bucket() {
    assert!(light_changed(None, 24.0, 0.0));
    assert!(!light_changed(Some((24.0, 0.0)), 26.0, 50.0));
    assert!(!light_changed(Some((24.0, 0.0)), 20.0, 0.0));
    assert!(light_changed(Some((24.0, 0.0)), 50.0, 50.0));
    assert!(!light_changed(Some((50.0, 50.0)), 74.0, 50.0));
    assert!(light_changed(Some((50.0, 50.0)), 76.0, 100.0));
}
