use super::*;
use crate::limits::{
    MAX_ACCESSIBILITY_EDGES, MAX_ACCESSIBILITY_NODE_TEXT_BYTES, MAX_ACCESSIBILITY_NODES,
    MAX_ACCESSIBILITY_TOTAL_TEXT_BYTES,
};
use crate::renderer_protocol::wire::{WireReader, WireWriter};
use std::collections::HashSet;

const MAX_COORDINATE: f32 = 10_000_000.0;

pub(super) fn encode(
    writer: &mut WireWriter,
    update: &AccessibilityUpdate,
) -> Result<(), ProtocolError> {
    validate_update(update)?;
    writer.bool(update.full);
    writer.u128(update.root.get());
    writer.u128(update.focus.get());
    writer.u32(update.nodes.len() as u32);
    for node in &update.nodes {
        writer.u128(node.id.get());
        writer.u8(role_tag(node.role));
        writer.string(&node.name)?;
        writer.string(&node.value)?;
        writer.string(&node.description)?;
        encode_rect(writer, node.bounds);
        writer.u32(node.children.len() as u32);
        for child in &node.children {
            writer.u128(child.get());
        }
        writer.bool(node.level.is_some());
        if let Some(level) = node.level {
            writer.u32(level);
        }
        writer.bool(node.disabled);
        writer.bool(node.read_only);
        writer.u8(action_bits(node.actions));
        writer.bool(node.selection.is_some());
        if let Some(selection) = node.selection {
            writer.u32(selection.start);
            writer.u32(selection.end);
        }
    }
    writer.u32(update.added.len() as u32);
    for id in &update.added {
        writer.u128(id.get());
    }
    writer.u32(update.removed.len() as u32);
    for id in &update.removed {
        writer.u128(id.get());
    }
    Ok(())
}

pub(super) fn decode(reader: &mut WireReader<'_>) -> Result<AccessibilityUpdate, ProtocolError> {
    let full = reader.bool()?;
    let root = DocumentNodeId::new(reader.u128()?)?;
    let focus = DocumentNodeId::new(reader.u128()?)?;
    let count = bounded_count(
        reader.u32()?,
        MAX_ACCESSIBILITY_NODES,
        "accessibility nodes",
    )?;
    let mut nodes = Vec::with_capacity(count);
    for _ in 0..count {
        let id = DocumentNodeId::new(reader.u128()?)?;
        let role = decode_role(reader.u8()?)?;
        let name = reader.string(MAX_ACCESSIBILITY_NODE_TEXT_BYTES)?;
        let value = reader.string(MAX_ACCESSIBILITY_NODE_TEXT_BYTES)?;
        let description = reader.string(MAX_ACCESSIBILITY_NODE_TEXT_BYTES)?;
        let bounds = decode_rect(reader)?;
        let child_count = bounded_count(
            reader.u32()?,
            MAX_ACCESSIBILITY_NODES,
            "accessibility children",
        )?;
        let mut children = Vec::with_capacity(child_count);
        for _ in 0..child_count {
            children.push(DocumentNodeId::new(reader.u128()?)?);
        }
        let level = reader.bool()?.then(|| reader.u32()).transpose()?;
        let disabled = reader.bool()?;
        let read_only = reader.bool()?;
        let actions = decode_action_bits(reader.u8()?)?;
        let selection = reader
            .bool()?
            .then(|| {
                Ok::<_, ProtocolError>(SemanticSelection {
                    start: reader.u32()?,
                    end: reader.u32()?,
                })
            })
            .transpose()?;
        nodes.push(SemanticNode {
            id,
            role,
            name,
            value,
            description,
            bounds,
            children,
            level,
            disabled,
            read_only,
            actions,
            selection,
        });
    }
    let added_count = bounded_count(
        reader.u32()?,
        MAX_ACCESSIBILITY_NODES,
        "added accessibility nodes",
    )?;
    let mut added = Vec::with_capacity(added_count);
    for _ in 0..added_count {
        added.push(DocumentNodeId::new(reader.u128()?)?);
    }
    let removed_count = bounded_count(
        reader.u32()?,
        MAX_ACCESSIBILITY_NODES,
        "removed accessibility nodes",
    )?;
    let mut removed = Vec::with_capacity(removed_count);
    for _ in 0..removed_count {
        removed.push(DocumentNodeId::new(reader.u128()?)?);
    }
    let update = AccessibilityUpdate {
        full,
        root,
        focus,
        nodes,
        added,
        removed,
    };
    validate_update(&update)?;
    Ok(update)
}

fn validate_update(update: &AccessibilityUpdate) -> Result<(), ProtocolError> {
    if update.nodes.len() > MAX_ACCESSIBILITY_NODES
        || update.added.len() > MAX_ACCESSIBILITY_NODES
        || update.removed.len() > MAX_ACCESSIBILITY_NODES
        || (update.full && (!update.added.is_empty() || !update.removed.is_empty()))
    {
        return Err(ProtocolError::InvalidPayload("accessibility update"));
    }
    let mut node_ids = HashSet::with_capacity(update.nodes.len());
    let mut edge_count = 0_usize;
    let mut text_bytes = 0_usize;
    for node in &update.nodes {
        if !node_ids.insert(node.id)
            || node.level == Some(0)
            || node
                .selection
                .is_some_and(|selection| selection.start > selection.end)
        {
            return Err(ProtocolError::InvalidPayload("accessibility node"));
        }
        validate_rect(node.bounds)?;
        let mut child_ids = HashSet::with_capacity(node.children.len());
        if node
            .children
            .iter()
            .any(|child| *child == node.id || !child_ids.insert(*child))
        {
            return Err(ProtocolError::InvalidPayload("accessibility children"));
        }
        edge_count = edge_count
            .checked_add(node.children.len())
            .ok_or(ProtocolError::InvalidPayload("accessibility edge budget"))?;
        for text in [&node.name, &node.value, &node.description] {
            if text.len() > MAX_ACCESSIBILITY_NODE_TEXT_BYTES {
                return Err(ProtocolError::InvalidPayload("accessibility node text"));
            }
            text_bytes = text_bytes
                .checked_add(text.len())
                .ok_or(ProtocolError::InvalidPayload("accessibility text budget"))?;
        }
    }
    if edge_count > MAX_ACCESSIBILITY_EDGES || text_bytes > MAX_ACCESSIBILITY_TOTAL_TEXT_BYTES {
        return Err(ProtocolError::InvalidPayload("accessibility budget"));
    }
    let mut added = HashSet::with_capacity(update.added.len());
    if update
        .added
        .iter()
        .any(|id| !added.insert(*id) || !node_ids.contains(id))
    {
        return Err(ProtocolError::InvalidPayload("added accessibility node"));
    }
    let mut removed = HashSet::with_capacity(update.removed.len());
    if update
        .removed
        .iter()
        .any(|id| !removed.insert(*id) || node_ids.contains(id) || *id == update.root)
    {
        return Err(ProtocolError::InvalidPayload("removed accessibility node"));
    }
    if update.full
        && (!node_ids.contains(&update.root)
            || !node_ids.contains(&update.focus)
            || update.nodes.len() != node_ids.len())
    {
        return Err(ProtocolError::InvalidPayload("full accessibility tree"));
    }
    Ok(())
}

fn encode_rect(writer: &mut WireWriter, rect: RectF) {
    writer.f32(rect.x);
    writer.f32(rect.y);
    writer.f32(rect.width);
    writer.f32(rect.height);
}

fn decode_rect(reader: &mut WireReader<'_>) -> Result<RectF, ProtocolError> {
    let rect = RectF {
        x: reader.f32()?,
        y: reader.f32()?,
        width: reader.f32()?,
        height: reader.f32()?,
    };
    validate_rect(rect)?;
    Ok(rect)
}

fn validate_rect(rect: RectF) -> Result<(), ProtocolError> {
    if rect.x.is_finite()
        && rect.y.is_finite()
        && rect.width.is_finite()
        && rect.height.is_finite()
        && (-MAX_COORDINATE..=MAX_COORDINATE).contains(&rect.x)
        && (-MAX_COORDINATE..=MAX_COORDINATE).contains(&rect.y)
        && (0.0..=MAX_COORDINATE).contains(&rect.width)
        && (0.0..=MAX_COORDINATE).contains(&rect.height)
    {
        Ok(())
    } else {
        Err(ProtocolError::InvalidPayload("accessibility bounds"))
    }
}

fn action_bits(actions: SemanticActions) -> u8 {
    u8::from(actions.focus) | (u8::from(actions.invoke) << 1) | (u8::from(actions.set_value) << 2)
}

fn decode_action_bits(bits: u8) -> Result<SemanticActions, ProtocolError> {
    if bits & !0b111 != 0 {
        return Err(ProtocolError::InvalidPayload("accessibility actions"));
    }
    Ok(SemanticActions {
        focus: bits & 1 != 0,
        invoke: bits & 2 != 0,
        set_value: bits & 4 != 0,
    })
}

fn role_tag(role: SemanticRole) -> u8 {
    role as u8 + 1
}

fn decode_role(tag: u8) -> Result<SemanticRole, ProtocolError> {
    const ROLES: &[SemanticRole] = &[
        SemanticRole::RootWebArea,
        SemanticRole::TextRun,
        SemanticRole::Paragraph,
        SemanticRole::Heading,
        SemanticRole::Link,
        SemanticRole::Button,
        SemanticRole::TextInput,
        SemanticRole::MultilineTextInput,
        SemanticRole::PasswordInput,
        SemanticRole::SearchInput,
        SemanticRole::ComboBox,
        SemanticRole::List,
        SemanticRole::ListItem,
        SemanticRole::Table,
        SemanticRole::RowGroup,
        SemanticRole::Row,
        SemanticRole::Cell,
        SemanticRole::RowHeader,
        SemanticRole::ColumnHeader,
        SemanticRole::Image,
        SemanticRole::Form,
        SemanticRole::Main,
        SemanticRole::Navigation,
        SemanticRole::Header,
        SemanticRole::Footer,
        SemanticRole::Article,
        SemanticRole::Section,
    ];
    tag.checked_sub(1)
        .and_then(|index| ROLES.get(index as usize))
        .copied()
        .ok_or(ProtocolError::InvalidPayload("accessibility role"))
}

fn bounded_count(value: u32, maximum: usize, field: &'static str) -> Result<usize, ProtocolError> {
    let value = value as usize;
    (value <= maximum)
        .then_some(value)
        .ok_or(ProtocolError::InvalidPayload(field))
}
