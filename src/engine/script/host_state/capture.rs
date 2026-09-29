//! Renderer-local binding of granted stream IDs to video elements.

use crate::engine::dom::NodeId;
use std::collections::{HashMap, HashSet};

const MAX_CAPTURE_ELEMENTS: usize = 4;

#[derive(Default)]
pub(in crate::engine::script) struct CaptureBindings {
    elements: HashMap<u64, HashSet<NodeId>>,
    enabled: HashMap<(u64, u8), bool>,
    ended_video: HashSet<u64>,
}

impl CaptureBindings {
    pub(in crate::engine::script) fn attach(
        &mut self,
        request_id: u64,
        node: NodeId,
    ) -> Result<(), &'static str> {
        if self.elements.values().map(HashSet::len).sum::<usize>() >= MAX_CAPTURE_ELEMENTS
            && !self
                .elements
                .get(&request_id)
                .is_some_and(|nodes| nodes.contains(&node))
        {
            return Err("capture element budget exceeded");
        }
        self.elements.entry(request_id).or_default().insert(node);
        Ok(())
    }

    pub(in crate::engine::script) fn detach(&mut self, request_id: u64, node: NodeId) {
        if let Some(nodes) = self.elements.get_mut(&request_id) {
            nodes.remove(&node);
            if nodes.is_empty() {
                self.elements.remove(&request_id);
            }
        }
    }

    pub(in crate::engine::script) fn set_enabled(
        &mut self,
        request_id: u64,
        track_id: u8,
        enabled: bool,
    ) {
        self.enabled.insert((request_id, track_id), enabled);
    }

    pub(in crate::engine::script) fn video_nodes(&self, request_id: u64) -> Vec<(NodeId, bool)> {
        if self.ended_video.contains(&request_id) {
            return Vec::new();
        }
        let enabled = self.enabled.get(&(request_id, 1)) != Some(&false);
        self.elements
            .get(&request_id)
            .map_or_else(Vec::new, |nodes| {
                nodes.iter().copied().map(|node| (node, enabled)).collect()
            })
    }

    pub(in crate::engine::script) fn retire(&mut self, request_id: u64) {
        self.elements.remove(&request_id);
        self.enabled.retain(|(id, _), _| *id != request_id);
        self.ended_video.remove(&request_id);
    }

    pub(in crate::engine::script) fn end_track(&mut self, request_id: u64, track_id: u8) {
        if track_id == 1 {
            self.ended_video.insert(request_id);
        }
    }
}
