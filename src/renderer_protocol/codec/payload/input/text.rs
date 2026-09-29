//! Bounded text-edit payload shared by legacy accessibility and native control input.

use crate::limits::MAX_RENDERER_TEXT_INPUT_BYTES;
use crate::renderer_protocol::input::{DocumentNodeId, TextInput};
use crate::renderer_protocol::wire::{WireReader, WireWriter};
use crate::renderer_protocol::{DocumentId, ProtocolError};

pub(super) fn encode(writer: &mut WireWriter, input: &TextInput) -> Result<(), ProtocolError> {
    writer.u128(input.target.get());
    writer.string(&input.value)?;
    writer.u32(input.selection_start);
    writer.u32(input.selection_end);
    Ok(())
}

pub(super) fn decode(
    reader: &mut WireReader<'_>,
    document: DocumentId,
    sequence: u64,
) -> Result<TextInput, ProtocolError> {
    Ok(TextInput {
        document,
        sequence,
        target: DocumentNodeId::new(reader.u128()?)?,
        value: reader.string(MAX_RENDERER_TEXT_INPUT_BYTES)?,
        selection_start: reader.u32()?,
        selection_end: reader.u32()?,
    })
}
