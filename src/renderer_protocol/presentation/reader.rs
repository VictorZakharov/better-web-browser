//! Checked wire representation for renderer-extracted Reader content.

use super::super::wire::{WireReader, WireWriter};
use crate::document::{Block, BlockKind, Document, Span};
use crate::limits::{MAX_DOM_NODES, MAX_RENDERED_TEXT_BYTES, MAX_URL_BYTES};
use crate::renderer_protocol::ProtocolError;

pub(super) fn encode_reader(
    writer: &mut WireWriter,
    document: &Document,
) -> Result<(), ProtocolError> {
    if document.title.len() > MAX_RENDERED_TEXT_BYTES
        || document.source_url.len() > MAX_URL_BYTES
        || document.blocks.len() > MAX_DOM_NODES
    {
        return Err(ProtocolError::InvalidPayload("reader document"));
    }
    writer.string(&document.title)?;
    writer.string(&document.source_url)?;
    writer.bool(document.truncated);
    writer.u32(document.blocks.len() as u32);
    let mut span_count = 0_usize;
    let mut text_bytes = 0_usize;
    for block in &document.blocks {
        encode_kind(writer, block.kind)?;
        span_count = span_count
            .checked_add(block.spans.len())
            .ok_or(ProtocolError::InvalidPayload("reader span count"))?;
        if span_count > MAX_DOM_NODES {
            return Err(ProtocolError::InvalidPayload("reader span count"));
        }
        writer.u32(block.spans.len() as u32);
        for span in &block.spans {
            text_bytes = text_bytes
                .checked_add(span.text.len())
                .ok_or(ProtocolError::InvalidPayload("reader text budget"))?;
            if text_bytes > MAX_RENDERED_TEXT_BYTES
                || span
                    .link
                    .as_ref()
                    .is_some_and(|link| link.len() > MAX_URL_BYTES)
            {
                return Err(ProtocolError::InvalidPayload("reader span"));
            }
            writer.string(&span.text)?;
            writer.bool(span.link.is_some());
            if let Some(link) = &span.link {
                writer.string(link)?;
            }
        }
    }
    Ok(())
}

pub(super) fn decode_reader(reader: &mut WireReader<'_>) -> Result<Document, ProtocolError> {
    let title = reader.string(MAX_RENDERED_TEXT_BYTES)?;
    let source_url = reader.string(MAX_URL_BYTES)?;
    let truncated = reader.bool()?;
    let block_count = bounded_count(reader.u32()?, MAX_DOM_NODES, "reader block count")?;
    let mut blocks = Vec::with_capacity(block_count);
    let mut span_count = 0_usize;
    let mut text_bytes = 0_usize;
    for _ in 0..block_count {
        let kind = decode_kind(reader)?;
        let count = bounded_count(reader.u32()?, MAX_DOM_NODES, "reader span count")?;
        span_count = span_count
            .checked_add(count)
            .ok_or(ProtocolError::InvalidPayload("reader span count"))?;
        if span_count > MAX_DOM_NODES {
            return Err(ProtocolError::InvalidPayload("reader span count"));
        }
        let mut spans = Vec::with_capacity(count);
        for _ in 0..count {
            let text = reader.string(MAX_RENDERED_TEXT_BYTES)?;
            text_bytes = text_bytes
                .checked_add(text.len())
                .ok_or(ProtocolError::InvalidPayload("reader text budget"))?;
            if text_bytes > MAX_RENDERED_TEXT_BYTES {
                return Err(ProtocolError::InvalidPayload("reader text budget"));
            }
            let link = reader
                .bool()?
                .then(|| reader.string(MAX_URL_BYTES))
                .transpose()?;
            spans.push(Span { text, link });
        }
        blocks.push(Block { kind, spans });
    }
    Ok(Document {
        title,
        source_url,
        blocks,
        truncated,
    })
}

fn encode_kind(writer: &mut WireWriter, kind: BlockKind) -> Result<(), ProtocolError> {
    match kind {
        BlockKind::Paragraph => writer.u8(1),
        BlockKind::Heading(level) if (1..=6).contains(&level) => {
            writer.u8(2);
            writer.u8(level);
        }
        BlockKind::Heading(_) => return Err(ProtocolError::InvalidPayload("reader heading")),
        BlockKind::ListItem => writer.u8(3),
        BlockKind::Quote => writer.u8(4),
        BlockKind::Preformatted => writer.u8(5),
    }
    Ok(())
}

fn decode_kind(reader: &mut WireReader<'_>) -> Result<BlockKind, ProtocolError> {
    match reader.u8()? {
        1 => Ok(BlockKind::Paragraph),
        2 => match reader.u8()? {
            level @ 1..=6 => Ok(BlockKind::Heading(level)),
            _ => Err(ProtocolError::InvalidPayload("reader heading")),
        },
        3 => Ok(BlockKind::ListItem),
        4 => Ok(BlockKind::Quote),
        5 => Ok(BlockKind::Preformatted),
        _ => Err(ProtocolError::InvalidPayload("reader block kind")),
    }
}

fn bounded_count(value: u32, maximum: usize, field: &'static str) -> Result<usize, ProtocolError> {
    let value = value as usize;
    (value <= maximum)
        .then_some(value)
        .ok_or(ProtocolError::InvalidPayload(field))
}
