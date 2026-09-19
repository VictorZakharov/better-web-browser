//! Navigation keeps the WindowProxy but replaces its Document and realm.
use super::*;

pub(in crate::engine::script) struct FrameNavigation {
    pub element: NodeId,
    pub document: NodeId,
    pub epoch: u64,
    pub url: String,
    pub source: Option<String>,
    pub scripts: bool,
    pub initiator_url: String,
    pub initiator_origin: crate::fetch::Origin,
}

pub(in crate::engine::script) struct Replacement<'s> {
    pub document: NodeRef,
    pub url: String,
    pub proxy: v8::Local<'s, v8::Object>,
}

impl FrameTree {
    pub(in crate::engine::script::engine) fn navigations(&self) -> Vec<FrameNavigation> {
        let mut requests = Vec::new();
        for child in self.children.borrow_mut().values_mut() {
            let attributes = (child.element.attr("srcdoc"), child.element.attr("src"));
            let initial = child.attributes.is_none();
            if child.attributes.as_ref() == Some(&attributes) {
                continue;
            }
            child.attributes = Some(attributes.clone());
            let state = child.host.borrow();
            let Some(parent) = child.parent_host.upgrade() else {
                continue;
            };
            let parent = parent.borrow();
            let source = attributes.0;
            let url = if source.is_some() {
                "about:srcdoc".into()
            } else if attributes
                .1
                .as_deref()
                .is_none_or(|src| src.trim().is_empty())
            {
                "about:blank".into()
            } else {
                crate::navigation::resolve_url(
                    &parent.script_base_url(),
                    attributes.1.as_deref().unwrap(),
                )
                .unwrap_or_else(|| "about:blank".into())
            };
            if initial && url == "about:blank" {
                continue;
            }
            child.load_notified = false;
            child.navigation_epoch += 1;
            requests.push(FrameNavigation {
                element: child.element.id(),
                document: state.document.id(),
                epoch: child.navigation_epoch,
                url,
                source,
                scripts: child.element.attr("sandbox").is_none_or(|flags| {
                    flags
                        .split_ascii_whitespace()
                        .any(|flag| flag.eq_ignore_ascii_case("allow-scripts"))
                }),
                initiator_url: parent
                    .inherited_url
                    .as_ref()
                    .unwrap_or(&parent.document_url)
                    .clone(),
                initiator_origin: parent.document_origin.clone(),
            });
        }
        requests
    }

    pub(in crate::engine::script::engine) fn replace<'s>(
        self: &Rc<Self>,
        scope: &mut v8::PinScope<'s, '_>,
        element: NodeId,
        document: NodeRef,
        url: String,
    ) -> Option<NodeId> {
        let (old, parent_document, node, attributes, epoch) = {
            let children = self.children.borrow();
            let child = children.get(&element)?;
            (
                child.context.clone(),
                child.parent_document,
                child.element.clone(),
                child.attributes.clone(),
                child.navigation_epoch,
            )
        };
        let parent = self
            .documents
            .borrow()
            .get(&parent_document)?
            .to_local(scope)?;
        if !active(parent) {
            return None;
        }
        let parent_scope = &mut v8::ContextScope::new(scope, parent);
        let old = v8::Local::new(parent_scope, old);
        let proxy = old.global(parent_scope);
        let element_object = self.wrappers.get(parent_scope, node.id())?.into();
        discard(self, element);
        // DetachGlobal is V8's supported WindowProxy reuse boundary. Retained old Documents
        // remain associated with their original inactive context, never the replacement host.
        super::super::v8_api::detach(old);
        let document_id = document.id();
        let replacement = Replacement {
            document,
            url,
            proxy,
        };
        let mut child = create(
            parent_scope,
            parent,
            &node,
            element_object,
            self,
            parent_document,
            Some(replacement),
        )?;
        child.attributes = attributes;
        child.navigation_epoch = epoch;
        self.children.borrow_mut().insert(element, child);
        Some(document_id)
    }
}
