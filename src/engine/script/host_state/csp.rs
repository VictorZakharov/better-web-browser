//! Install meta-delivered CSP when an HTML meta element becomes connected to head.
//! Attribute changes after insertion do not reprocess the pragma.
use super::*;
use crate::engine::dom::Node;

const HTML: &str = "http://www.w3.org/1999/xhtml";

impl HostState {
    pub(crate) fn process_inserted_csp_meta(&mut self, inserted: &NodeRef) -> Result<(), String> {
        for node in Node::descendants(inserted) {
            if node.tag_name() != Some("meta") || node.namespace_uri() != Some(HTML) {
                continue;
            }
            let Some(parent) = node.parent() else {
                continue;
            };
            if parent.tag_name() != Some("head")
                || parent.namespace_uri() != Some(HTML)
                || Node::tree_root(&parent).id() != self.document.id()
                || !self.processed_csp_meta.insert(node.id())
            {
                continue;
            }
            if !node
                .attr("http-equiv")
                .is_some_and(|value| value.trim().eq_ignore_ascii_case("content-security-policy"))
            {
                continue;
            }
            let Some(content) = node.attr("content").filter(|value| !value.is_empty()) else {
                continue;
            };
            let mut policy = (*self.policy).clone();
            policy
                .append_meta(&self.document_url, &content)
                .map_err(|error| format!("install meta Content Security Policy: {error}"))?;
            self.policy = std::sync::Arc::new(policy);
            self.pending_policy_updates.push(ScriptPolicyUpdate {
                client: self.fetch_client,
                serialized: content,
            });
        }
        Ok(())
    }
}
