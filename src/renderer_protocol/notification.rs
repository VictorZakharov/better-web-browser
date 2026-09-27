//! Bounded notification requests. The browser resolves the origin from `client`.

use super::{DocumentId, ProtocolError};
use crate::fetch::RequestClient;

pub const MAX_NOTIFICATION_TITLE_BYTES: usize = 512;
pub const MAX_NOTIFICATION_BODY_BYTES: usize = 2 * 1024;
pub const MAX_NOTIFICATION_TAG_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NotificationPermission {
    #[default]
    Default,
    Granted,
    Denied,
}

impl NotificationPermission {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Granted => "granted",
            Self::Denied => "denied",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationRequest {
    pub document: DocumentId,
    pub request_id: u64,
    pub client: RequestClient,
    pub action: NotificationAction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotificationAction {
    RequestPermission,
    Show {
        title: String,
        body: String,
        tag: String,
    },
    Close,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationUpdate {
    pub document: DocumentId,
    pub request_id: u64,
    pub event: NotificationEvent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotificationEvent {
    Permission(NotificationPermission),
    Shown,
    Clicked,
    Closed,
    Error,
}

impl NotificationRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.request_id == 0 {
            return Err(ProtocolError::InvalidPayload(
                "notification request identifier",
            ));
        }
        if let NotificationAction::Show { title, body, tag } = &self.action
            && (title.len() > MAX_NOTIFICATION_TITLE_BYTES
                || body.len() > MAX_NOTIFICATION_BODY_BYTES
                || tag.len() > MAX_NOTIFICATION_TAG_BYTES)
        {
            return Err(ProtocolError::InvalidPayload("notification text"));
        }
        Ok(())
    }
}

impl NotificationUpdate {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.request_id == 0 {
            return Err(ProtocolError::InvalidPayload(
                "notification update identifier",
            ));
        }
        Ok(())
    }
}
