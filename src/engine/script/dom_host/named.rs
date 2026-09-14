//! Window named access in tree order, indexed per document mutation generation.
//! https://html.spec.whatwg.org/multipage/nav-history-apis.html#named-access-on-the-window-object
use super::*;

#[derive(Default)]
pub(in crate::engine::script) struct NamedPropertyIndex {
    version: Option<u64>,
    names: Vec<String>,
    nodes: HashMap<String, Vec<NodeRef>>,
}

impl NamedPropertyIndex {
    fn refresh(&mut self, document: &NodeRef) {
        let version = document.subtree_mutation_version();
        if self.version == Some(version) {
            return;
        }
        self.names.clear();
        self.nodes.clear();
        for node in Node::descendants(document) {
            for name in named_values(&node) {
                let entries = self.nodes.entry(name.clone()).or_insert_with(|| {
                    self.names.push(name);
                    Vec::new()
                });
                // An element with equal id and name participates only once.
                if entries
                    .last()
                    .is_none_or(|previous| previous.id() != node.id())
                {
                    entries.push(node.clone());
                }
            }
        }
        self.version = Some(version);
    }
}

pub(super) fn named_property_names(state: &mut HostState) -> String {
    state.named_property_index.refresh(&state.document);
    serde_json::to_string(&state.named_property_index.names).unwrap_or_else(|_| "[]".into())
}

pub(super) fn named_property_nodes(state: &mut HostState, wanted: &str) -> Vec<NodeRef> {
    state.named_property_index.refresh(&state.document);
    state
        .named_property_index
        .nodes
        .get(wanted)
        .cloned()
        .unwrap_or_default()
}

pub(super) fn named_property_candidates(state: &HostState, root_id: u32) -> String {
    let Some(root) = state.node(root_id) else {
        return "[]".into();
    };
    let mut seen = HashSet::new();
    let names = Node::descendants(&root)
        .flat_map(|node| named_values(&node))
        .filter(|name| seen.insert(name.clone()))
        .collect::<Vec<_>>();
    serde_json::to_string(&names).unwrap_or_else(|_| "[]".into())
}

fn named_values(node: &NodeRef) -> Vec<String> {
    if node.namespace_uri() != Some(HTML_NAMESPACE) {
        return Vec::new();
    }
    let mut names = Vec::with_capacity(2);
    if let Some(id) = node.attr("id").filter(|id| !id.is_empty()) {
        names.push(id);
    }
    if matches!(node.tag_name(), Some("embed" | "form" | "img" | "object"))
        && let Some(name) = node.attr("name").filter(|name| !name.is_empty())
    {
        names.push(name);
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn index_is_reused_then_rebuilt_for_attribute_and_tree_changes() {
        let dom = crate::engine::dom::parse(
            "<div id=a></div><form id=b name=a></form><img id=a name=a><svg id=excluded></svg>",
        );
        let mut index = NamedPropertyIndex::default();
        index.refresh(&dom.document);
        assert_eq!(index.names, ["a", "b"]);
        assert_eq!(index.nodes["a"].len(), 3);
        let allocation = index.nodes["a"].as_ptr();
        for _ in 0..100 {
            index.refresh(&dom.document);
        }
        assert_eq!(index.nodes["a"].as_ptr(), allocation);
        let form = dom.elements_named("form").next().unwrap();
        form.set_attr("name", "renamed");
        index.refresh(&dom.document);
        assert_eq!(index.nodes["a"].len(), 2);
        assert_eq!(index.nodes["renamed"][0].id(), form.id());
        Node::remove_from_parent(&form);
        index.refresh(&dom.document);
        assert!(!index.nodes.contains_key("renamed"));
        assert!(!index.nodes.contains_key("b"));
    }
}
