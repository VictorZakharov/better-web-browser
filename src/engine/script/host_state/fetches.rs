//! Request identifiers are unique within an agent, with an explicit owning document.
use super::*;

pub(in crate::engine::script) struct FetchIdentifiers {
    next: u32,
    owners: HashMap<u32, NodeId>,
}

impl Default for FetchIdentifiers {
    fn default() -> Self {
        Self {
            next: 1,
            owners: HashMap::new(),
        }
    }
}

impl FetchIdentifiers {
    pub(in crate::engine::script) fn allocate(&mut self, owner: NodeId) -> JsResult<u32> {
        let id = self.next;
        self.next = id.checked_add(1).ok_or_else(|| {
            JsNativeError::range().with_message("Fetch request identifiers were exhausted")
        })?;
        self.owners.insert(id, owner);
        Ok(id)
    }

    pub(in crate::engine::script) fn owner(&self, id: u32) -> Option<NodeId> {
        self.owners.get(&id).copied()
    }

    pub(in crate::engine::script) fn reassign(&mut self, id: u32, document: NodeId) {
        if let Some(owner) = self.owners.get_mut(&id) {
            *owner = document;
        }
    }

    pub(in crate::engine::script) fn finish(&mut self, id: u32) {
        self.owners.remove(&id);
    }

    pub(in crate::engine::script) fn cancel_document(&mut self, document: NodeId) -> Vec<u32> {
        let ids: Vec<_> = self
            .owners
            .iter()
            .filter_map(|(id, owner)| (*owner == document).then_some(*id))
            .collect();
        for id in &ids {
            self.owners.remove(id);
        }
        ids
    }
}
