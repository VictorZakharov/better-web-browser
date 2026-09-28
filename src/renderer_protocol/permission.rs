//! Browser-authoritative permission queries and state notifications.

use super::{DocumentId, ProtocolError};
use crate::fetch::RequestClient;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PermissionName {
    Notifications,
    Geolocation,
    Accelerometer,
    Gyroscope,
    Magnetometer,
    AmbientLightSensor,
}

impl PermissionName {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Notifications => "notifications",
            Self::Geolocation => "geolocation",
            Self::Accelerometer => "accelerometer",
            Self::Gyroscope => "gyroscope",
            Self::Magnetometer => "magnetometer",
            Self::AmbientLightSensor => "ambient-light-sensor",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionState {
    Prompt,
    Granted,
    Denied,
}

impl PermissionState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Prompt => "prompt",
            Self::Granted => "granted",
            Self::Denied => "denied",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermissionRequest {
    pub document: DocumentId,
    pub request_id: u64,
    pub client: RequestClient,
    pub embedded: bool,
    pub name: PermissionName,
}

impl PermissionRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.request_id == 0 || self.request_id > u64::from(u32::MAX) {
            return Err(ProtocolError::InvalidPayload("permission query"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermissionUpdate {
    pub document: DocumentId,
    /// Query ID retained for routing subsequent changes to the owning frame realm.
    pub request_id: u64,
    pub name: PermissionName,
    pub state: PermissionState,
    /// A bounded browser registry refused this query; the Promise rejects.
    pub rejected: bool,
}

impl PermissionUpdate {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.request_id == 0 || self.request_id > u64::from(u32::MAX) {
            return Err(ProtocolError::InvalidPayload("permission update"));
        }
        Ok(())
    }
}
