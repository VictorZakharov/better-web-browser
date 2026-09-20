//! Document ownership, including the inert document used by HTML template contents.

use super::*;

#[derive(Default)]
pub(in crate::engine::script) struct DocumentRegistry {
    pub(in crate::engine::script) owner_documents: HashMap<NodeId, u64>,
    pub(in crate::engine::script) document_roots: HashMap<u64, std::rc::Weak<Node>>,
    pub(in crate::engine::script) html_documents: HashSet<u64>,
    pub(in crate::engine::script) document_metadata:
        HashMap<NodeId, super::super::dom_host::DocumentMetadata>,
    pub(in crate::engine::script) template_contents_documents: HashMap<u64, u64>,
}

impl DocumentRegistry {
    pub(in crate::engine::script) fn collect_dead_documents(&mut self) {
        let dead: HashSet<_> = self
            .document_roots
            .iter()
            .filter_map(|(id, root)| (root.strong_count() == 0).then_some(*id))
            .collect();
        self.document_roots
            .retain(|_, root| root.strong_count() > 0);
        self.owner_documents
            .retain(|_, owner| !dead.contains(owner));
        self.html_documents.retain(|owner| !dead.contains(owner));
        self.document_metadata
            .retain(|node, _| !dead.contains(&node.document()));
        self.template_contents_documents
            .retain(|owner, _| !dead.contains(owner));
    }
}

impl HostState {
    pub(in crate::engine::script) fn share_document_registry(&mut self, parent: &Self) {
        let mut old = self.documents.borrow_mut();
        let mut shared = parent.documents.borrow_mut();
        shared.owner_documents.extend(old.owner_documents.drain());
        shared.document_roots.extend(old.document_roots.drain());
        shared.html_documents.extend(old.html_documents.drain());
        shared
            .document_metadata
            .extend(old.document_metadata.drain());
        shared
            .template_contents_documents
            .extend(old.template_contents_documents.drain());
        drop(shared);
        drop(old);
        self.documents = Rc::clone(&parent.documents);
    }
    pub(in crate::engine::script) fn document_for(&self, node: &NodeRef) -> Option<NodeRef> {
        self.documents
            .borrow()
            .document_roots
            .get(&self.owner_document_identity(node))
            .and_then(std::rc::Weak::upgrade)
    }

    pub(in crate::engine::script) fn is_html_document_for(&self, node: &NodeRef) -> bool {
        self.documents
            .borrow()
            .html_documents
            .contains(&self.owner_document_identity(node))
    }

    pub(in crate::engine::script) fn register_document(
        &mut self,
        document: NodeRef,
        html: bool,
    ) -> u32 {
        let identity = document.id().document();
        // HTML's appropriate-template-contents-owner-document algorithm shares one inert
        // document per owner, and reuses that document for nested templates. Allocate it
        // alongside the owner; callers reserve both nodes against the DOM node budget.
        // https://html.spec.whatwg.org/multipage/scripting.html#appropriate-template-contents-owner-document
        let inert = Node::create_document();
        let inert_identity = inert.id().document();
        self.documents.borrow_mut().collect_dead_documents();
        self.documents
            .borrow_mut()
            .document_roots
            .insert(identity, Rc::downgrade(&document));
        self.documents
            .borrow_mut()
            .document_roots
            .insert(inert_identity, Rc::downgrade(&inert));
        self.documents
            .borrow_mut()
            .template_contents_documents
            .insert(identity, inert_identity);
        self.documents
            .borrow_mut()
            .template_contents_documents
            .insert(inert_identity, inert_identity);
        if html {
            self.documents
                .borrow_mut()
                .html_documents
                .extend([identity, inert_identity]);
        }
        self.id_for(&inert);
        self.register_subtree(&document);
        self.id_for(&document)
    }

    pub(in crate::engine::script) fn adopt_subtree(&mut self, parent: &NodeRef, child: &NodeRef) {
        self.assign_subtree_owner(child, self.owner_document_identity(parent), false);
    }

    pub(in crate::engine::script) fn register_subtree(&mut self, root: &NodeRef) {
        let owner = root
            .parent()
            .map(|parent| self.owner_document_identity(&parent))
            .unwrap_or_else(|| self.owner_document_identity(root));
        self.assign_subtree_owner(root, owner, true);
    }

    fn assign_subtree_owner(&mut self, root: &NodeRef, owner: u64, register: bool) {
        let mut stack = vec![(root.clone(), owner)];
        while let Some((node, owner)) = stack.pop() {
            self.documents
                .borrow_mut()
                .owner_documents
                .insert(node.id(), owner);
            if register {
                self.id_for(&node);
            }
            stack.extend(
                node.children
                    .borrow()
                    .iter()
                    .rev()
                    .cloned()
                    .map(|child| (child, owner)),
            );
            stack.extend(node.shadow_root().map(|shadow| (shadow, owner)));
            if let Some(contents) = node
                .element()
                .and_then(|element| element.template_contents.borrow().clone())
            {
                let inert = self
                    .documents
                    .borrow()
                    .template_contents_documents
                    .get(&owner)
                    .copied()
                    .unwrap_or(owner);
                stack.push((contents, inert));
            }
        }
    }

    pub(super) fn owner_document_identity(&self, node: &NodeRef) -> u64 {
        self.documents
            .borrow()
            .owner_documents
            .get(&node.id())
            .copied()
            .unwrap_or_else(|| node.id().document())
    }
}
