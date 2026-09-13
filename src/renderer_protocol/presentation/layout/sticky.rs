//! Bounded, pointer-free retained sticky constraints; never trust renderer range indices.
use super::*;
use crate::engine::layout::StickyLayer;

pub(super) fn encode(
    writer: &mut WireWriter,
    layers: &[StickyLayer],
    items: usize,
) -> Result<(), ProtocolError> {
    validate(layers, items)?;
    writer.u32(layers.len() as u32);
    for layer in layers {
        writer.u128(layer.node_id.to_wire());
        writer.u32(layer.items.start as u32);
        writer.u32(layer.items.end as u32);
        for rect in [layer.normal, layer.containing, layer.port] {
            encode_rect(writer, rect);
        }
        for value in layer.insets {
            writer.bool(value.is_some());
            if let Some(value) = value {
                writer.f32(value);
            }
        }
        for value in layer.margins {
            writer.f32(value);
        }
        for parent in [layer.parent, layer.port_parent] {
            writer.u32(parent.map_or(0, |i| i as u32 + 1));
        }
        writer.bool(layer.viewport_port);
        writer.f32(layer.offset.0);
        writer.f32(layer.offset.1);
    }
    Ok(())
}

pub(super) fn decode(
    reader: &mut WireReader<'_>,
    items: usize,
) -> Result<Vec<StickyLayer>, ProtocolError> {
    let count = bounded_count(reader.u32()?, MAX_DOM_NODES, "sticky layers")?;
    let mut layers = Vec::with_capacity(count);
    for _ in 0..count {
        let node_id = NodeId::from_wire(reader.u128()?)
            .ok_or(ProtocolError::InvalidPayload("sticky node"))?;
        let start = reader.u32()? as usize;
        let end = reader.u32()? as usize;
        let normal = decode_rect(reader)?;
        let containing = decode_rect(reader)?;
        let port = decode_rect(reader)?;
        let mut insets = [None; 4];
        for inset in &mut insets {
            if reader.bool()? {
                *inset = Some(reader.f32()?);
            }
        }
        let mut margins = [0.0; 4];
        for margin in &mut margins {
            *margin = reader.f32()?;
        }
        let parent = (reader.u32()? as usize).checked_sub(1);
        let port_parent = (reader.u32()? as usize).checked_sub(1);
        let viewport_port = reader.bool()?;
        let offset = (reader.f32()?, reader.f32()?);
        layers.push(StickyLayer {
            node_id,
            items: start..end,
            normal,
            containing,
            port,
            insets,
            margins,
            parent,
            port_parent,
            viewport_port,
            offset,
        });
    }
    validate(&layers, items)?;
    Ok(layers)
}

pub(in crate::renderer_protocol::presentation) fn validate(
    layers: &[StickyLayer],
    items: usize,
) -> Result<(), ProtocolError> {
    if layers.len() > MAX_DOM_NODES {
        return Err(ProtocolError::InvalidPayload("sticky layers"));
    }
    let mut depths = Vec::with_capacity(layers.len());
    for (index, layer) in layers.iter().enumerate() {
        if layer.items.start > layer.items.end
            || layer.items.end > items
            || [layer.parent, layer.port_parent]
                .into_iter()
                .flatten()
                .any(|parent| parent >= index)
        {
            return Err(ProtocolError::InvalidPayload("sticky range or parent"));
        }
        let depth = layer.parent.map_or(1, |parent| depths[parent] + 1);
        if depth > 256 {
            return Err(ProtocolError::InvalidPayload("sticky depth"));
        }
        depths.push(depth);
        for value in layer
            .insets
            .into_iter()
            .flatten()
            .chain(layer.margins)
            .chain([layer.offset.0, layer.offset.1])
        {
            finite(
                value,
                -MAX_PRESENTATION_COORDINATE,
                MAX_PRESENTATION_COORDINATE,
                "sticky coordinate",
            )?;
        }
        for rect in [layer.normal, layer.containing, layer.port] {
            for value in [rect.x, rect.y] {
                finite(
                    value,
                    -MAX_PRESENTATION_COORDINATE,
                    MAX_PRESENTATION_COORDINATE,
                    "sticky rect",
                )?;
            }
            for value in [rect.width, rect.height] {
                finite(value, 0.0, MAX_PRESENTATION_COORDINATE, "sticky size")?;
            }
        }
    }
    // Prevent overlapping sibling ranges from multiplying per-frame work. Nested ranges
    // are allowed, but their nesting is bounded independently of claimed parent indices.
    let mut ranges = layers
        .iter()
        .filter(|layer| !layer.items.is_empty())
        .map(|layer| layer.items.clone())
        .collect::<Vec<_>>();
    ranges.sort_by_key(|r| (r.start, std::cmp::Reverse(r.end)));
    let mut ends = Vec::new();
    for range in ranges {
        while ends.last().is_some_and(|end| *end <= range.start) {
            ends.pop();
        }
        if ends.len() >= 256 || ends.last().is_some_and(|end| range.end > *end) {
            return Err(ProtocolError::InvalidPayload("sticky range nesting"));
        }
        ends.push(range.end);
    }
    Ok(())
}
