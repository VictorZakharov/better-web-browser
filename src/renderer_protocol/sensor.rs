//! Bounded requests and updates for browser-owned physical sensors.

use super::{DocumentId, ProtocolError};
use crate::fetch::RequestClient;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SensorKind {
    Orientation,
    Motion,
    Accelerometer,
    Gyroscope,
    Magnetometer,
    AbsoluteOrientation,
    RelativeOrientation,
    AmbientLight,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SensorPermission {
    Granted,
    Denied,
}

impl SensorPermission {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Granted => "granted",
            Self::Denied => "denied",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SensorError {
    NotAllowed,
    NotReadable,
    NotSupported,
}

impl SensorError {
    pub const fn dom_name(self) -> &'static str {
        match self {
            Self::NotAllowed => "NotAllowedError",
            Self::NotReadable => "NotReadableError",
            Self::NotSupported => "NotSupportedError",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SensorAction {
    RequestPermission {
        kind: SensorKind,
        absolute: bool,
    },
    Start {
        kind: SensorKind,
        frequency_hz: Option<f64>,
    },
    Stop {
        kind: SensorKind,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct SensorRequest {
    pub document: DocumentId,
    pub request_id: u64,
    pub client: RequestClient,
    pub user_activation: bool,
    pub action: SensorAction,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SensorReading {
    Orientation {
        alpha: Option<f64>,
        beta: Option<f64>,
        gamma: Option<f64>,
        absolute: bool,
    },
    Motion {
        acceleration: Option<[f64; 3]>,
        acceleration_including_gravity: Option<[f64; 3]>,
        rotation_rate: Option<[f64; 3]>,
        interval_ms: f64,
    },
    ThreeAxis {
        x: f64,
        y: f64,
        z: f64,
        timestamp_ms: f64,
    },
    Quaternion {
        x: f64,
        y: f64,
        z: f64,
        w: f64,
        timestamp_ms: f64,
    },
    Illuminance {
        illuminance: f64,
        timestamp_ms: f64,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum SensorEvent {
    Permission(SensorPermission),
    Activated,
    Reading(SensorReading),
    Error(SensorError),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SensorUpdate {
    pub document: DocumentId,
    pub request_id: u64,
    pub event: SensorEvent,
}

impl SensorRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.request_id == 0 {
            return Err(ProtocolError::InvalidPayload("sensor request identifier"));
        }
        if let SensorAction::RequestPermission { kind, absolute } = self.action
            && (!matches!(kind, SensorKind::Orientation | SensorKind::Motion)
                || (absolute && kind != SensorKind::Orientation))
        {
            return Err(ProtocolError::InvalidPayload("sensor permission kind"));
        }
        if let SensorAction::Start { frequency_hz, .. } = self.action
            && frequency_hz.is_some_and(|frequency| {
                !frequency.is_finite() || frequency <= 0.0 || frequency > 60.0
            })
        {
            return Err(ProtocolError::InvalidPayload("sensor frequency"));
        }
        Ok(())
    }
}

impl SensorUpdate {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.request_id == 0 {
            return Err(ProtocolError::InvalidPayload("sensor update identifier"));
        }
        if let SensorEvent::Reading(reading) = &self.event {
            let finite = |value: f64| value.is_finite();
            let vector = |values: &[f64; 3]| values.iter().copied().all(finite);
            let valid = match reading {
                SensorReading::Orientation {
                    alpha, beta, gamma, ..
                } => alpha
                    .iter()
                    .chain(beta.iter())
                    .chain(gamma.iter())
                    .copied()
                    .all(finite),
                SensorReading::Motion {
                    acceleration,
                    acceleration_including_gravity,
                    rotation_rate,
                    interval_ms,
                } => {
                    finite(*interval_ms)
                        && *interval_ms >= 0.0
                        && acceleration.as_ref().is_none_or(vector)
                        && acceleration_including_gravity.as_ref().is_none_or(vector)
                        && rotation_rate.as_ref().is_none_or(vector)
                }
                SensorReading::ThreeAxis {
                    x,
                    y,
                    z,
                    timestamp_ms,
                } => [*x, *y, *z, *timestamp_ms].into_iter().all(finite) && *timestamp_ms >= 0.0,
                SensorReading::Quaternion {
                    x,
                    y,
                    z,
                    w,
                    timestamp_ms,
                } => {
                    let values = [*x, *y, *z, *w];
                    values.into_iter().all(finite)
                        && finite(*timestamp_ms)
                        && *timestamp_ms >= 0.0
                        && values.into_iter().map(|value| value * value).sum::<f64>() > 0.0
                }
                SensorReading::Illuminance {
                    illuminance,
                    timestamp_ms,
                } => {
                    finite(*illuminance)
                        && *illuminance >= 0.0
                        && finite(*timestamp_ms)
                        && *timestamp_ms >= 0.0
                }
            };
            if !valid {
                return Err(ProtocolError::InvalidPayload("sensor reading"));
            }
        }
        Ok(())
    }
}
