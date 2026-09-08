//! Document ownership, including the inert document used by HTML template contents.

use super::*;

impl HostState {
    pub(in crate::engine::script) fn document_for(&self, node: &NodeRef) -> Option<NodeRef> {
        self.document_roots
            .get(&self.owner_document_identity(node))
            .cloned()
    }

    pub(in crate::engine::script) fn is_html_document_for(&self, node: &NodeRef) -> bool {
        self.html_documents
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
        self.document_roots.insert(identity, document.clone());
        self.document_roots.insert(inert_identity, inert.clone());
        self.template_contents_documents
            .insert(identity, inert_identity);
        self.template_contents_documents
            .insert(inert_identity, inert_identity);
        if html {
            self.html_documents.extend([identity, inert_identity]);
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
            self.owner_documents.insert(node.id(), owner);
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
                    .template_contents_documents
                    .get(&owner)
                    .copied()
                    .unwrap_or(owner);
                stack.push((contents, inert));
            }
        }
    }

    pub(super) fn owner_document_identity(&self, node: &NodeRef) -> u64 {
        self.owner_documents
            .get(&node.id())
            .copied()
            .unwrap_or_else(|| node.id().document())
    }
}
