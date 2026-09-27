use crate::renderer_protocol::notification::{
    MAX_NOTIFICATION_BODY_BYTES, MAX_NOTIFICATION_TAG_BYTES, MAX_NOTIFICATION_TITLE_BYTES,
};
use crate::renderer_protocol::wire::{WireReader, WireWriter};
use crate::renderer_protocol::{
    DocumentId, NotificationAction, NotificationEvent, NotificationPermission, NotificationRequest,
    NotificationUpdate, ProtocolError,
};

pub(super) fn encode_request(request: &NotificationRequest) -> Result<Vec<u8>, ProtocolError> {
    request.validate()?;
    let mut writer = WireWriter::new();
    writer.u64(request.document.get());
    writer.u64(request.request_id);
    writer.u64(request.client.id);
    writer.bool(request.client.opaque);
    match &request.action {
        NotificationAction::RequestPermission => writer.u8(1),
        NotificationAction::Show { title, body, tag } => {
            writer.u8(2);
            writer.string(title)?;
            writer.string(body)?;
            writer.string(tag)?;
        }
        NotificationAction::Close => writer.u8(3),
    }
    Ok(writer.finish())
}

pub(super) fn decode_request(bytes: &[u8]) -> Result<NotificationRequest, ProtocolError> {
    let mut reader = WireReader::new(bytes);
    let document = DocumentId::new(reader.u64()?)?;
    let request_id = reader.u64()?;
    let client = crate::fetch::RequestClient {
        id: reader.u64()?,
        opaque: reader.bool()?,
    };
    let action = match reader.u8()? {
        1 => NotificationAction::RequestPermission,
        2 => NotificationAction::Show {
            title: reader.string(MAX_NOTIFICATION_TITLE_BYTES)?,
            body: reader.string(MAX_NOTIFICATION_BODY_BYTES)?,
            tag: reader.string(MAX_NOTIFICATION_TAG_BYTES)?,
        },
        3 => NotificationAction::Close,
        _ => return Err(ProtocolError::InvalidPayload("notification action")),
    };
    reader.finish()?;
    let request = NotificationRequest {
        document,
        request_id,
        client,
        action,
    };
    request.validate()?;
    Ok(request)
}

pub(super) fn encode_update(update: &NotificationUpdate) -> Result<Vec<u8>, ProtocolError> {
    update.validate()?;
    let mut writer = WireWriter::new();
    writer.u64(update.document.get());
    writer.u64(update.request_id);
    writer.u8(match update.event {
        NotificationEvent::Permission(NotificationPermission::Default) => 1,
        NotificationEvent::Permission(NotificationPermission::Granted) => 2,
        NotificationEvent::Permission(NotificationPermission::Denied) => 3,
        NotificationEvent::Shown => 4,
        NotificationEvent::Clicked => 5,
        NotificationEvent::Closed => 6,
        NotificationEvent::Error => 7,
    });
    Ok(writer.finish())
}

pub(super) fn decode_update(bytes: &[u8]) -> Result<NotificationUpdate, ProtocolError> {
    let mut reader = WireReader::new(bytes);
    let document = DocumentId::new(reader.u64()?)?;
    let request_id = reader.u64()?;
    let event = match reader.u8()? {
        1 => NotificationEvent::Permission(NotificationPermission::Default),
        2 => NotificationEvent::Permission(NotificationPermission::Granted),
        3 => NotificationEvent::Permission(NotificationPermission::Denied),
        4 => NotificationEvent::Shown,
        5 => NotificationEvent::Clicked,
        6 => NotificationEvent::Closed,
        7 => NotificationEvent::Error,
        _ => return Err(ProtocolError::InvalidPayload("notification event")),
    };
    reader.finish()?;
    let update = NotificationUpdate {
        document,
        request_id,
        event,
    };
    update.validate()?;
    Ok(update)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_frames_validate_bounds_and_client_identity() {
        let request = NotificationRequest {
            document: DocumentId::new(7).unwrap(),
            request_id: 4,
            client: crate::fetch::RequestClient {
                id: 9,
                opaque: true,
            },
            action: NotificationAction::Show {
                title: "title".into(),
                body: "body".into(),
                tag: "tag".into(),
            },
        };
        assert_eq!(
            decode_request(&encode_request(&request).unwrap()).unwrap(),
            request
        );
        let mut truncated = encode_request(&request).unwrap();
        truncated.pop();
        assert!(decode_request(&truncated).is_err());
        let update = NotificationUpdate {
            document: request.document,
            request_id: 4,
            event: NotificationEvent::Shown,
        };
        assert_eq!(
            decode_update(&encode_update(&update).unwrap()).unwrap(),
            update
        );
        let oversized = NotificationRequest {
            action: NotificationAction::Show {
                title: "x".repeat(MAX_NOTIFICATION_TITLE_BYTES + 1),
                body: String::new(),
                tag: String::new(),
            },
            ..request
        };
        assert!(encode_request(&oversized).is_err());
    }
}
