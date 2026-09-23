use crate::limits::MAX_INDEXED_DB_IPC_BYTES;
use crate::renderer_protocol::wire::{WireReader, WireWriter};
use crate::renderer_protocol::{DatabaseCommand, DatabaseEvent, DocumentId, ProtocolError};

pub(super) fn encode_command(command: &DatabaseCommand) -> Result<Vec<u8>, ProtocolError> {
    command.validate()?;
    let mut writer = WireWriter::new();
    writer.u64(command.document.get());
    writer.u64(command.request_id);
    writer.u64(command.client.id);
    writer.bool(command.client.opaque);
    writer.string(&command.payload)?;
    Ok(writer.finish())
}

pub(super) fn decode_command(payload: &[u8]) -> Result<DatabaseCommand, ProtocolError> {
    let mut reader = WireReader::new(payload);
    let command = DatabaseCommand {
        document: DocumentId::new(reader.u64()?)?,
        request_id: reader.u64()?,
        client: crate::fetch::RequestClient {
            id: reader.u64()?,
            opaque: reader.bool()?,
        },
        payload: reader.string(MAX_INDEXED_DB_IPC_BYTES)?,
    };
    reader.finish()?;
    command.validate()?;
    Ok(command)
}

pub(super) fn encode_event(event: &DatabaseEvent) -> Result<Vec<u8>, ProtocolError> {
    event.validate()?;
    let mut writer = WireWriter::new();
    writer.u64(event.document.get());
    writer.u64(event.request_id);
    writer.string(&event.payload)?;
    Ok(writer.finish())
}

pub(super) fn decode_event(payload: &[u8]) -> Result<DatabaseEvent, ProtocolError> {
    let mut reader = WireReader::new(payload);
    let event = DatabaseEvent {
        document: DocumentId::new(reader.u64()?)?,
        request_id: reader.u64()?,
        payload: reader.string(MAX_INDEXED_DB_IPC_BYTES)?,
    };
    reader.finish()?;
    event.validate()?;
    Ok(event)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_frames_roundtrip_and_reject_truncation() {
        let command = DatabaseCommand {
            document: DocumentId::new(3).unwrap(),
            request_id: 9,
            client: Default::default(),
            payload: "{\"op\":\"Open\"}".into(),
        };
        let mut bytes = encode_command(&command).unwrap();
        assert_eq!(decode_command(&bytes).unwrap(), command);
        bytes.pop();
        assert!(decode_command(&bytes).is_err());
        let event = DatabaseEvent {
            document: command.document,
            request_id: 9,
            payload: "{\"version\":1}".into(),
        };
        assert_eq!(decode_event(&encode_event(&event).unwrap()).unwrap(), event);
    }
}
