//! Lossless nullable DOMStrings in one strictly bounded change record.
use crate::renderer_protocol::wire::{WireReader, WireWriter};
use crate::renderer_protocol::{DocumentId, ProtocolError, StorageSync};
use crate::storage::{StorageChange, StorageString, StorageUpdate};

pub(super) fn encode(sync: &StorageSync) -> Result<Vec<u8>, ProtocolError> {
    sync.validate()?;
    let mut writer = WireWriter::new();
    writer.u64(sync.document.get());
    writer.u8(super::state::area_tag(sync.update.area));
    writer.u64(sync.update.version);
    writer.u64(sync.update.acknowledgement);
    writer.string(&sync.update.source_url)?;
    writer.bool(sync.update.change.is_some());
    if let Some(change) = &sync.update.change {
        for value in [&change.key, &change.old_value, &change.new_value] {
            writer.bool(value.is_some());
            if let Some(value) = value {
                writer.storage_string(value)?;
            }
        }
    }
    Ok(writer.finish())
}

pub(super) fn decode(payload: &[u8]) -> Result<StorageSync, ProtocolError> {
    let mut reader = WireReader::new(payload);
    let document = DocumentId::new(reader.u64()?)?;
    let area = super::state::decode_area(reader.u8()?)?;
    let version = reader.u64()?;
    let acknowledgement = reader.u64()?;
    let source_url = reader.string(crate::limits::MAX_URL_BYTES)?;
    let change = if reader.bool()? {
        Some(StorageChange {
            version,
            key: nullable(&mut reader)?,
            old_value: nullable(&mut reader)?,
            new_value: nullable(&mut reader)?,
        })
    } else {
        None
    };
    reader.finish()?;
    let sync = StorageSync {
        document,
        update: StorageUpdate {
            area,
            version,
            acknowledgement,
            change,
            source_url,
        },
    };
    sync.validate()?;
    Ok(sync)
}

fn nullable(reader: &mut WireReader<'_>) -> Result<Option<StorageString>, ProtocolError> {
    if reader.bool()? {
        reader
            .storage_string(crate::limits::MAX_STORAGE_BYTES_PER_ORIGIN)
            .map(Some)
    } else {
        Ok(None)
    }
}
