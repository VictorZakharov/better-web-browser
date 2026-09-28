//! Document-scoped geolocation commands. The browser resolves `client` from its
//! registered frame tree; renderer-supplied URLs never authorize location access.

use super::{DocumentId, ProtocolError};
use crate::fetch::RequestClient;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeolocationRequest {
    pub document: DocumentId,
    pub request_id: u64,
    pub client: RequestClient,
    pub action: GeolocationAction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GeolocationAction {
    Start {
        watch: bool,
        high_accuracy: bool,
        /// `u64::MAX` represents the Web IDL Infinity default.
        timeout_millis: u64,
        maximum_age_millis: u64,
    },
    Clear,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeolocationUpdate {
    pub document: DocumentId,
    pub request_id: u64,
    /// Releases a one-shot request or a watch revoked by the browser.
    pub terminal: bool,
    pub event: GeolocationEvent,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GeolocationEvent {
    Position(GeolocationPosition),
    Error {
        code: GeolocationErrorCode,
        message: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum GeolocationErrorCode {
    PermissionDenied = 1,
    PositionUnavailable = 2,
    Timeout = 3,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeolocationPosition {
    pub latitude: f64,
    pub longitude: f64,
    pub accuracy: f64,
    pub altitude: Option<f64>,
    pub altitude_accuracy: Option<f64>,
    pub heading: Option<f64>,
    pub speed: Option<f64>,
    /// Milliseconds since the Unix epoch, at acquisition time.
    pub timestamp_millis: u64,
}

impl GeolocationPosition {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if !self.latitude.is_finite()
            || !(-90.0..=90.0).contains(&self.latitude)
            || !self.longitude.is_finite()
            || !(-180.0..=180.0).contains(&self.longitude)
            || !self.accuracy.is_finite()
            || self.accuracy < 0.0
            || self.altitude.is_some_and(|value| !value.is_finite())
            || self
                .altitude_accuracy
                .is_some_and(|value| !value.is_finite() || value < 0.0)
            || self
                .heading
                .is_some_and(|value| !value.is_finite() || !(0.0..360.0).contains(&value))
            || self
                .speed
                .is_some_and(|value| !value.is_finite() || value < 0.0)
        {
            return Err(ProtocolError::InvalidPayload("geolocation position"));
        }
        Ok(())
    }
}

impl GeolocationRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.request_id == 0 {
            return Err(ProtocolError::InvalidPayload(
                "geolocation request identifier",
            ));
        }
        Ok(())
    }
}

impl GeolocationUpdate {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.request_id == 0 {
            return Err(ProtocolError::InvalidPayload(
                "geolocation update identifier",
            ));
        }
        match &self.event {
            GeolocationEvent::Position(position) => position.validate(),
            GeolocationEvent::Error { message, .. } if message.len() > 512 => {
                Err(ProtocolError::InvalidPayload("geolocation error length"))
            }
            GeolocationEvent::Error { .. } => Ok(()),
        }
    }
}
