//! File-picker intent and bounded, path-free selection stream.

use crate::fetch::RequestClient;
use crate::renderer_protocol::wire::{WireReader, WireWriter};
use crate::renderer_protocol::{
    DocumentId, DocumentNodeId, FilePickerRequest, FilePickerUpdate, MAX_FILE_PICKER_ACCEPT_BYTES,
    MAX_FILE_PICKER_CHUNK_BYTES, MAX_FILE_PICKER_FILES, MAX_FILE_PICKER_MIME_BYTES,
    MAX_FILE_PICKER_NAME_BYTES, ProtocolError, SelectedFileMetadata,
};

pub(super) fn encode_request(request: &FilePickerRequest) -> Result<Vec<u8>, ProtocolError> {
    request.validate()?;
    let mut writer = WireWriter::new();
    writer.u64(request.document.get());
    writer.u64(request.request_id);
    writer.u128(request.node.get());
    writer.u64(request.client.id);
    writer.bool(request.client.opaque);
    writer.bool(request.multiple);
    writer.string(&request.accept)?;
    Ok(writer.finish())
}

pub(super) fn decode_request(bytes: &[u8]) -> Result<FilePickerRequest, ProtocolError> {
    let mut reader = WireReader::new(bytes);
    let request = FilePickerRequest {
        document: DocumentId::new(reader.u64()?)?,
        request_id: reader.u64()?,
        node: DocumentNodeId::new(reader.u128()?)?,
        client: RequestClient {
            id: reader.u64()?,
            opaque: reader.bool()?,
        },
        multiple: reader.bool()?,
        accept: reader.string(MAX_FILE_PICKER_ACCEPT_BYTES)?,
    };
    reader.finish()?;
    request.validate()?;
    Ok(request)
}

pub(super) fn encode_update(update: &FilePickerUpdate) -> Result<Vec<u8>, ProtocolError> {
    update.validate()?;
    let mut writer = WireWriter::new();
    writer.u64(update.document().get());
    writer.u64(update.request_id());
    match update {
        FilePickerUpdate::Start { files, .. } => {
            writer.u8(1);
            writer.u8(files.len() as u8);
            for file in files {
                writer.string(&file.name)?;
                writer.string(&file.mime_type)?;
                writer.u64(file.last_modified as u64);
                writer.u32(file.size);
            }
        }
        FilePickerUpdate::Chunk {
            file_index,
            offset,
            bytes,
            ..
        } => {
            writer.u8(2);
            writer.u8(*file_index);
            writer.u32(*offset);
            writer.bytes(bytes)?;
        }
        FilePickerUpdate::End { .. } => writer.u8(3),
        FilePickerUpdate::Canceled { .. } => writer.u8(4),
        FilePickerUpdate::Failed { .. } => writer.u8(5),
    }
    Ok(writer.finish())
}

pub(super) fn decode_update(bytes: &[u8]) -> Result<FilePickerUpdate, ProtocolError> {
    let mut reader = WireReader::new(bytes);
    let document = DocumentId::new(reader.u64()?)?;
    let request_id = reader.u64()?;
    let update = match reader.u8()? {
        1 => {
            let count = reader.u8()? as usize;
            if count == 0 || count > MAX_FILE_PICKER_FILES {
                return Err(ProtocolError::InvalidPayload("selected file count"));
            }
            let mut files = Vec::with_capacity(count);
            for _ in 0..count {
                files.push(SelectedFileMetadata {
                    name: reader.string(MAX_FILE_PICKER_NAME_BYTES)?,
                    mime_type: reader.string(MAX_FILE_PICKER_MIME_BYTES)?,
                    last_modified: reader.u64()? as i64,
                    size: reader.u32()?,
                });
            }
            FilePickerUpdate::Start {
                document,
                request_id,
                files,
            }
        }
        2 => FilePickerUpdate::Chunk {
            document,
            request_id,
            file_index: reader.u8()?,
            offset: reader.u32()?,
            bytes: reader.bytes(MAX_FILE_PICKER_CHUNK_BYTES)?,
        },
        3 => FilePickerUpdate::End {
            document,
            request_id,
        },
        4 => FilePickerUpdate::Canceled {
            document,
            request_id,
        },
        5 => FilePickerUpdate::Failed {
            document,
            request_id,
        },
        _ => return Err(ProtocolError::InvalidPayload("file picker update kind")),
    };
    reader.finish()?;
    update.validate()?;
    Ok(update)
}

#[cfg(test)]
mod tests;
