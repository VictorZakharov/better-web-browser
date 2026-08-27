//! Style-set construction and cascade ordering.

mod presentational;
mod sheets;
#[cfg(test)]
mod tests;

use super::rule_index::RuleIndex;
use super::*;
use presentational::apply_presentational_hints;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StyleRefreshStats {
    pub invalidated_nodes: usize,
    pub total_styles: usize,
    pub recomputed_styles: usize,
    pub changed_styles: usize,
    pub removed_styles: usize,
    pub layout_changed: bool,
    pub full_rebuild: bool,
}

#[derive(Debug, Default)]
pub struct StyleSet {
    pub styles: HashMap<NodeId, ComputedStyle>,
    rules: Vec<Rule>,
    rule_index: RuleIndex,
    document_base_url: String,
    viewport_width: f32,
    viewport_height: f32,
}

impl StyleSet {
    pub fn from_dom(dom: &Dom, external_stylesheets: &[String], viewport_width: f32) -> Self {
        let sources = external_stylesheets
            .iter()
            .map(|stylesheet| (String::new(), stylesheet.clone()))
            .collect::<Vec<_>>();
        Self::from_document(&dom.document, "", &sources, viewport_width)
    }

    pub(crate) fn from_sources_for_viewport(
        dom: &Dom,
        document_base_url: &str,
        external_stylesheets: &[(String, String)],
        viewport_width: f32,
        viewport_height: f32,
    ) -> Self {
        Self::from_document_for_viewport(
            &dom.document,
            document_base_url,
            external_stylesheets,
            viewport_width,
            viewport_height,
        )
    }

    pub(crate) fn from_document(
        document: &NodeRef,
        document_base_url: &str,
        external_stylesheets: &[(String, String)],
        viewport_width: f32,
    ) -> Self {
        Self::from_document_for_viewport(
            document,
            document_base_url,
            external_stylesheets,
            viewport_width,
            viewport_width,
        )
    }

    pub(crate) fn from_document_for_viewport(
        document: &NodeRef,
        document_base_url: &str,
        external_stylesheets: &[(String, String)],
        viewport_width: f32,
        viewport_height: f32,
    ) -> Self {
        let mut set = Self::for_computed_style_for_viewport(
            document,
            document_base_url,
            external_stylesheets,
            viewport_width,
            viewport_height,
        );
        set.compute_subtree(document, None);
        set
    }

    pub(crate) fn for_computed_style(
        document: &NodeRef,
        document_base_url: &str,
        external_stylesheets: &[(String, String)],
        viewport_width: f32,
    ) -> Self {
        Self::for_computed_style_for_viewport(
            document,
            document_base_url,
            external_stylesheets,
            viewport_width,
            viewport_width,
        )
    }

    pub(crate) fn for_computed_style_for_viewport(
        document: &NodeRef,
        document_base_url: &str,
        external_stylesheets: &[(String, String)],
        viewport_width: f32,
        viewport_height: f32,
    ) -> Self {
        let viewport_width = viewport_width.max(1.0);
        let viewport_height = viewport_height.max(1.0);
        let rules = sheets::collect(
            document,
            document_base_url,
            external_stylesheets,
            viewport_width,
        );
        let rule_index = RuleIndex::new(&rules);
        Self {
            styles: HashMap::new(),
            rules,
            rule_index,
            document_base_url: document_base_url.to_string(),
            viewport_width,
            viewport_height,
        }
    }

    pub(crate) fn clear_computed_styles(&mut self) {
        self.styles.clear();
    }

    pub(crate) fn computed_style_for_node(&mut self, node: &NodeRef) -> Option<&ComputedStyle> {
        if !self.styles.contains_key(&node_id(node)) {
            let mut ancestors = std::iter::successors(Some(node.clone()), Node::composed_parent)
                .collect::<Vec<_>>();
            ancestors.reverse();
            let mut parent_style = None;
            for ancestor in ancestors {
                let style = self
                    .styles
                    .get(&node_id(&ancestor))
                    .cloned()
                    .unwrap_or_else(|| self.compute_style(&ancestor, parent_style.as_ref()));
                self.styles.insert(node_id(&ancestor), style.clone());
                parent_style = Some(style);
            }
        }
        self.styles.get(&node_id(node))
    }

    pub fn get(&self, node: &NodeRef) -> &ComputedStyle {
        self.styles
            .get(&node_id(node))
            .expect("style should exist for every DOM node")
    }

    /// Uses the engine selector parser for opt-in inspection and future DOM query APIs.
    pub fn query_selector_all(&self, dom: &Dom, input: &str) -> Option<Vec<NodeRef>> {
        let selector = parse_selector(input.trim())?;
        Some(
            dom::Node::descendants(&dom.document)
                .filter(|node| selector_matches(&selector, node))
                .collect(),
        )
    }

    pub(crate) fn refresh_subtree(
        &mut self,
        document: &NodeRef,
        requested_root: &NodeRef,
        removed_nodes: &[NodeId],
    ) -> StyleRefreshStats {
        let requested_root = requested_root
            .shadow_host()
            .unwrap_or_else(|| requested_root.clone());
        let root = if is_descendant_of(&requested_root, document) {
            requested_root
        } else {
            document.clone()
        };
        let parent_style = Node::composed_parent(&root)
            .and_then(|parent| self.styles.get(&node_id(&parent)).cloned());
        let mut stats = StyleRefreshStats::default();
        self.recompute_subtree(&root, parent_style.as_ref(), &mut stats);

        // A node may be removed and reinserted before the rendering checkpoint. Its identifier
        // remains in the removal log, but its newly recomputed style must remain available.
        let connected_nodes = Node::shadow_including_descendants(document)
            .map(|node| node_id(&node))
            .collect::<HashSet<_>>();
        stats.removed_styles = removed_nodes
            .iter()
            .filter(|node| !connected_nodes.contains(node))
            .filter(|node| self.styles.remove(node).is_some())
            .count();
        stats.total_styles = self.styles.len();
        stats
    }

    fn compute_subtree(&mut self, node: &NodeRef, parent: Option<&ComputedStyle>) {
        let root = node_id(node);
        let mut pending = vec![node.clone()];
        while let Some(node) = pending.pop() {
            let style = if node_id(&node) == root {
                self.compute_style(&node, parent)
            } else {
                let parent = Node::composed_parent(&node)
                    .expect("connected composed-tree child has a parent");
                let parent_style = self
                    .styles
                    .get(&node_id(&parent))
                    .expect("parent style is computed before its children");
                self.compute_style(&node, Some(parent_style))
            };
            self.styles.insert(node_id(&node), style);
            pending.extend(Node::composed_children(&node).into_iter().rev());
        }
    }

    fn recompute_subtree(
        &mut self,
        node: &NodeRef,
        parent: Option<&ComputedStyle>,
        stats: &mut StyleRefreshStats,
    ) {
        let root = node_id(node);
        let mut pending = vec![node.clone()];
        while let Some(node) = pending.pop() {
            let style = if node_id(&node) == root {
                self.compute_style(&node, parent)
            } else {
                let parent = Node::composed_parent(&node)
                    .expect("connected composed-tree child has a parent");
                let parent_style = self
                    .styles
                    .get(&node_id(&parent))
                    .expect("parent style is recomputed before its children");
                self.compute_style(&node, Some(parent_style))
            };
            stats.invalidated_nodes += 1;
            stats.recomputed_styles += 1;
            match self.styles.get(&node_id(&node)) {
                Some(previous) if previous != &style => {
                    stats.changed_styles += 1;
                    stats.layout_changed |= !previous.layout_equivalent(&style);
                }
                None => {
                    stats.changed_styles += 1;
                    stats.layout_changed = true;
                }
                _ => {}
            }
            self.styles.insert(node_id(&node), style);
            pending.extend(Node::composed_children(&node).into_iter().rev());
        }
    }

    fn compute_style(&self, node: &NodeRef, parent: Option<&ComputedStyle>) -> ComputedStyle {
        let mut style = ComputedStyle::inherit_from(parent);
        apply_user_agent_defaults(node, &mut style);

        let mut matching = self
            .rule_index
            .candidates(node)
            .into_iter()
            .filter_map(|index| self.rules.get(index))
            .filter(|rule| rule_applies_to(rule, node))
            .filter(|rule| selector_matches(&rule.selector, node))
            .collect::<Vec<_>>();
        matching.sort_by(|left, right| {
            left.selector
                .specificity
                .cmp(&right.selector.specificity)
                .then_with(|| left.order.cmp(&right.order))
        });
        let inline_declarations = node
            .attr("style")
            .map(|inline| parse_declarations(&inline))
            .unwrap_or_default();

        // CSS Cascade places every important author declaration above every normal author
        // declaration. Inline declarations retain their higher specificity within each group.
        // https://drafts.csswg.org/css-cascade/#importance
        let mut cascaded = Vec::new();
        for important in [false, true] {
            for rule in &matching {
                cascaded.extend(
                    rule.declarations
                        .iter()
                        .filter(|declaration| declaration.important == important)
                        .map(|declaration| (declaration, rule.base_url.as_str())),
                );
            }
            cascaded.extend(
                inline_declarations
                    .iter()
                    .filter(|declaration| declaration.important == important)
                    .map(|declaration| (declaration, self.document_base_url.as_str())),
            );
        }

        for &(declaration, _) in &cascaded {
            apply_custom_properties(&mut style, std::slice::from_ref(declaration), parent);
        }
        for &(declaration, base_url) in &cascaded {
            if declaration.name != "line-height" {
                apply_resolved_declaration(
                    &mut style,
                    declaration,
                    parent,
                    base_url,
                    self.viewport_width,
                    self.viewport_height,
                );
            }
        }
        // line-height depends on the winning font-size, independent of declaration source order.
        for &(declaration, base_url) in &cascaded {
            if declaration.name == "line-height" {
                apply_resolved_declaration(
                    &mut style,
                    declaration,
                    parent,
                    base_url,
                    self.viewport_width,
                    self.viewport_height,
                );
            }
        }
        apply_presentational_hints(node, &mut style);
        style.resolve_viewport_units(self.viewport_width, self.viewport_height);
        if node.attr("hidden").is_some() || is_hidden_by_html_rendering(node) {
            style.display = Display::None;
        }
        style.line_height = style.line_height.max(style.font_size);
        style
    }
}

fn is_descendant_of(node: &NodeRef, ancestor: &NodeRef) -> bool {
    std::iter::successors(Some(node.clone()), |current| {
        current.shadow_including_parent()
    })
    .any(|current| current.id() == ancestor.id())
}

fn rule_applies_to(rule: &Rule, node: &NodeRef) -> bool {
    match rule.scope {
        RuleScope::Document => !matches!(Node::tree_root(node).data, NodeData::ShadowRoot(_)),
        RuleScope::Shadow(root) => Node::tree_root(node).id() == root,
        RuleScope::Host(root) => node.shadow_root().is_some_and(|shadow| shadow.id() == root),
        RuleScope::Slotted(root) => {
            Node::assigned_slot(node).is_some_and(|slot| Node::tree_root(&slot).id() == root)
        }
    }
}

fn node_id(node: &NodeRef) -> NodeId {
    node.id()
}
