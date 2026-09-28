//! Bounded BroadcastChannel control frames.

use crate::renderer_protocol::broadcast_channel::{
    MAX_BROADCAST_MESSAGE_BYTES, MAX_BROADCAST_NAME_BYTES,
};
use crate::renderer_protocol::wire::{WireReader, WireWriter};
use crate::renderer_protocol::{
    BroadcastCommand, BroadcastDelivery, BroadcastOperation, DocumentId, ProtocolError,
};

pub(super) fn encode_command(command: &BroadcastCommand) -> Result<Vec<u8>, ProtocolError> {
    command.validate()?;
    let mut writer = WireWriter::new();
    writer.u64(command.document.get());
    writer.u64(command.channel_id);
    match &command.operation {
        BroadcastOperation::Open { name } => {
            writer.u8(1);
            writer.string(name)?;
        }
        BroadcastOperation::Post { serialized } => {
            writer.u8(2);
            writer.string(serialized)?;
        }
        BroadcastOperation::Close => writer.u8(3),
    }
    Ok(writer.finish())
}

pub(super) fn decode_command(bytes: &[u8]) -> Result<BroadcastCommand, ProtocolError> {
    let mut reader = WireReader::new(bytes);
    let document = DocumentId::new(reader.u64()?)?;
    let channel_id = reader.u64()?;
    let operation = match reader.u8()? {
        1 => BroadcastOperation::Open {
            name: reader.string(MAX_BROADCAST_NAME_BYTES)?,
        },
        2 => BroadcastOperation::Post {
            serialized: reader.string(MAX_BROADCAST_MESSAGE_BYTES)?,
        },
        3 => BroadcastOperation::Close,
        _ => return Err(ProtocolError::InvalidPayload("broadcast operation")),
    };
    reader.finish()?;
    let command = BroadcastCommand {
        document,
        channel_id,
        operation,
    };
    command.validate()?;
    Ok(command)
}

pub(super) fn encode_delivery(delivery: &BroadcastDelivery) -> Result<Vec<u8>, ProtocolError> {
    delivery.validate()?;
    let mut writer = WireWriter::new();
    writer.u64(delivery.document.get());
    writer.u64(delivery.channel_id);
    writer.string(&delivery.origin)?;
    writer.string(&delivery.serialized)?;
    Ok(writer.finish())
}

pub(super) fn decode_delivery(bytes: &[u8]) -> Result<BroadcastDelivery, ProtocolError> {
    let mut reader = WireReader::new(bytes);
    let delivery = BroadcastDelivery {
        document: DocumentId::new(reader.u64()?)?,
        channel_id: reader.u64()?,
        origin: reader.string(crate::limits::MAX_URL_BYTES)?,
        serialized: reader.string(MAX_BROADCAST_MESSAGE_BYTES)?,
    };
    reader.finish()?;
    delivery.validate()?;
    Ok(delivery)
}
