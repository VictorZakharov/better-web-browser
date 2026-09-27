//! Resource-aware presentation compaction and queue backpressure.

use super::*;
use crate::limits::MAX_RENDERER_PRESENTATION_BYTES;
use crate::renderer_protocol::{RendererPresentation, RendererRuntimeUpdate};

pub(super) fn coalesce(
    events: &mut VecDeque<RendererEvent>,
    next: Box<RendererPresentation>,
) -> Result<Box<RendererPresentation>, ProtocolError> {
    // Only an adjacent presentation can merge. A RuntimeUpdate or diagnostic is a FIFO
    // barrier: moving the earlier presentation's console/navigation deltas past it would
    // change observable event order even if the visual snapshot is newer.
    if !matches!(events.back(), Some(RendererEvent::Presentation(previous))
        if previous.document == next.document)
    {
        return Ok(next);
    }
    let Some(RendererEvent::Presentation(previous)) = events.pop_back() else {
        unreachable!("adjacent presentation changed while queue was locked");
    };
    let (presentation, remaining) = previous.coalesce(*next)?;
    if let Some(next) = remaining {
        events.push_back(RendererEvent::Presentation(Box::new(presentation)));
        Ok(Box::new(next))
    } else {
        Ok(Box::new(presentation))
    }
}

pub(super) fn exceeds_resource_budget(
    events: &VecDeque<RendererEvent>,
    next: &RendererEvent,
) -> bool {
    let bytes = events
        .iter()
        .fold(event_resource_bytes(next), |bytes, event| {
            bytes.saturating_add(event_resource_bytes(event))
        });
    bytes > MAX_RENDERER_PRESENTATION_BYTES
}

fn event_resource_bytes(event: &RendererEvent) -> usize {
    match event {
        RendererEvent::Presentation(value) => value.one_shot_resource_bytes(),
        RendererEvent::RuntimeUpdate(value) => value.one_shot_resource_bytes(),
        _ => 0,
    }
}

pub(super) fn coalesce_runtime(
    events: &mut VecDeque<RendererEvent>,
    next: Box<RendererRuntimeUpdate>,
) -> Result<Box<RendererRuntimeUpdate>, ProtocolError> {
    if !matches!(events.back(), Some(RendererEvent::RuntimeUpdate(previous))
        if previous.document == next.document)
    {
        return Ok(next);
    }
    let Some(RendererEvent::RuntimeUpdate(previous)) = events.pop_back() else {
        unreachable!("runtime-update position changed while queue was locked");
    };
    let (update, remaining) = previous.coalesce(*next)?;
    if let Some(next) = remaining {
        events.push_back(RendererEvent::RuntimeUpdate(Box::new(update)));
        Ok(Box::new(next))
    } else {
        Ok(Box::new(update))
    }
}
