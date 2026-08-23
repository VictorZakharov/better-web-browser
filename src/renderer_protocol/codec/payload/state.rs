use crate::limits::{
    MAX_COOKIE_ASSIGNMENT_BYTES, MAX_COOKIE_HEADER_BYTES, MAX_STORAGE_KEY_BYTES,
    MAX_STORAGE_VALUE_BYTES,
};
use crate::renderer_protocol::wire::{WireReader, WireWriter};
use crate::renderer_protocol::{
    BrowserMessage, CookieMutation, CookieStateSnapshot, DocumentId, ProtocolError,
    RendererMessage, StorageMutationRequest, StorageSnapshotEnd, StorageSnapshotEntry,
    StorageSnapshotStart,
};
use crate::storage::{StorageAreaKind, StorageEntry, StorageMutation, StorageOperation};

pub(super) fn encode_browser_state(
    message: &BrowserMessage,
) -> Result<(u16, Vec<u8>), ProtocolError> {
    let mut writer = WireWriter::new();
    let kind = match message {
        BrowserMessage::CookieSnapshot(snapshot) => {
            snapshot.validate()?;
            writer.u64(snapshot.document.get());
            writer.u64(snapshot.version);
            writer.string(&snapshot.header)?;
            0x0131
        }
        BrowserMessage::StorageSnapshotStart(start) => {
            start.validate()?;
            writer.u64(start.document.get());
            writer.u8(area_tag(start.area));
            writer.u64(start.version);
            writer.u32(start.entry_count);
            0x0133
        }
        BrowserMessage::StorageSnapshotEntry(item) => {
            item.validate()?;
            writer.u64(item.document.get());
            writer.u8(area_tag(item.area));
            writer.string(&item.entry.key)?;
            writer.string(&item.entry.value)?;
            0x0135
        }
        BrowserMessage::StorageSnapshotEnd(end) => {
            end.validate()?;
            writer.u64(end.document.get());
            writer.u8(area_tag(end.area));
            writer.u64(end.version);
            0x0137
        }
        _ => return Err(ProtocolError::InvalidPayload("browser state message")),
    };
    Ok((kind, writer.finish()))
}

pub(super) fn decode_browser_state(
    kind: u16,
    payload: &[u8],
) -> Result<BrowserMessage, ProtocolError> {
    let mut reader = WireReader::new(payload);
    let message = match kind {
        0x0131 => {
            let snapshot = CookieStateSnapshot {
                document: DocumentId::new(reader.u64()?)?,
                version: nonzero(reader.u64()?, "cookie snapshot")?,
                header: reader.string(MAX_COOKIE_HEADER_BYTES)?,
            };
            snapshot.validate()?;
            BrowserMessage::CookieSnapshot(snapshot)
        }
        0x0133 => {
            let start = StorageSnapshotStart {
                document: DocumentId::new(reader.u64()?)?,
                area: decode_area(reader.u8()?)?,
                version: nonzero(reader.u64()?, "storage snapshot")?,
                entry_count: reader.u32()?,
            };
            start.validate()?;
            BrowserMessage::StorageSnapshotStart(start)
        }
        0x0135 => {
            let item = StorageSnapshotEntry {
                document: DocumentId::new(reader.u64()?)?,
                area: decode_area(reader.u8()?)?,
                entry: StorageEntry {
                    key: reader.string(MAX_STORAGE_KEY_BYTES)?,
                    value: reader.string(MAX_STORAGE_VALUE_BYTES)?,
                },
            };
            item.validate()?;
            BrowserMessage::StorageSnapshotEntry(item)
        }
        0x0137 => {
            let end = StorageSnapshotEnd {
                document: DocumentId::new(reader.u64()?)?,
                area: decode_area(reader.u8()?)?,
                version: nonzero(reader.u64()?, "storage snapshot")?,
            };
            end.validate()?;
            BrowserMessage::StorageSnapshotEnd(end)
        }
        _ => return Err(ProtocolError::UnexpectedMessage(kind)),
    };
    reader.finish()?;
    Ok(message)
}

pub(super) fn encode_renderer_state(
    message: &RendererMessage,
) -> Result<(u16, Vec<u8>), ProtocolError> {
    let mut writer = WireWriter::new();
    let kind = match message {
        RendererMessage::CookieMutation(mutation) => {
            mutation.validate()?;
            writer.u64(mutation.document.get());
            writer.string(&mutation.assignment)?;
            0x0132
        }
        RendererMessage::StorageMutation(request) => {
            request.validate()?;
            writer.u64(request.document.get());
            writer.u8(area_tag(request.mutation.area));
            writer.u64(request.mutation.expected_version);
            match &request.mutation.operation {
                StorageOperation::Set { key, value } => {
                    writer.u8(1);
                    writer.string(key)?;
                    writer.string(value)?;
                }
                StorageOperation::Remove { key } => {
                    writer.u8(2);
                    writer.string(key)?;
                }
                StorageOperation::Clear => writer.u8(3),
            }
            0x0134
        }
        _ => return Err(ProtocolError::InvalidPayload("renderer state message")),
    };
    Ok((kind, writer.finish()))
}

pub(super) fn decode_renderer_state(
    kind: u16,
    payload: &[u8],
) -> Result<RendererMessage, ProtocolError> {
    let mut reader = WireReader::new(payload);
    let message = match kind {
        0x0132 => {
            let mutation = CookieMutation {
                document: DocumentId::new(reader.u64()?)?,
                assignment: reader.string(MAX_COOKIE_ASSIGNMENT_BYTES)?,
            };
            mutation.validate()?;
            RendererMessage::CookieMutation(mutation)
        }
        0x0134 => {
            let document = DocumentId::new(reader.u64()?)?;
            let area = decode_area(reader.u8()?)?;
            let expected_version = nonzero(reader.u64()?, "storage mutation")?;
            let operation = match reader.u8()? {
                1 => StorageOperation::Set {
                    key: reader.string(MAX_STORAGE_KEY_BYTES)?,
                    value: reader.string(MAX_STORAGE_VALUE_BYTES)?,
                },
                2 => StorageOperation::Remove {
                    key: reader.string(MAX_STORAGE_KEY_BYTES)?,
                },
                3 => StorageOperation::Clear,
                _ => return Err(ProtocolError::InvalidPayload("storage mutation operation")),
            };
            let request = StorageMutationRequest {
                document,
                mutation: StorageMutation {
                    area,
                    expected_version,
                    operation,
                },
            };
            request.validate()?;
            RendererMessage::StorageMutation(request)
        }
        _ => return Err(ProtocolError::UnexpectedMessage(kind)),
    };
    reader.finish()?;
    Ok(message)
}

fn area_tag(area: StorageAreaKind) -> u8 {
    match area {
        StorageAreaKind::Local => 1,
        StorageAreaKind::Session => 2,
    }
}

fn decode_area(tag: u8) -> Result<StorageAreaKind, ProtocolError> {
    match tag {
        1 => Ok(StorageAreaKind::Local),
        2 => Ok(StorageAreaKind::Session),
        _ => Err(ProtocolError::InvalidPayload("storage area")),
    }
}

fn nonzero(value: u64, field: &'static str) -> Result<u64, ProtocolError> {
    (value != 0)
        .then_some(value)
        .ok_or(ProtocolError::InvalidPayload(field))
}
