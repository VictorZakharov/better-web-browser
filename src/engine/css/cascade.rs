//! Style-set construction and cascade ordering.

mod presentational;
mod pseudo;
mod refresh;
mod root_units;
mod sheets;
#[cfg(test)]
mod tests;

use super::media::MediaEnvironment;
use super::rule_index::RuleIndex;
use super::selector_match::selector_matches;
use super::*;
use presentational::apply_presentational_hints;
use root_units::root_font_size_for;

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
    pseudo_styles: HashMap<(NodeId, PseudoElement), ComputedStyle>,
    generated_nodes: HashMap<(NodeId, PseudoElement), NodeRef>,
    generated_styles: HashMap<NodeId, ComputedStyle>,
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

    #[cfg(test)]
    pub(crate) fn from_sources_for_viewport(
        dom: &Dom,
        document_base_url: &str,
        external_stylesheets: &[(String, String)],
        viewport_width: f32,
        viewport_height: f32,
    ) -> Self {
        Self::from_sources_for_media_environment(
            dom,
            document_base_url,
            external_stylesheets,
            MediaEnvironment::new(viewport_width, viewport_height, 1.0, false),
        )
    }

    pub(crate) fn from_sources_for_media_environment(
        dom: &Dom,
        document_base_url: &str,
        external_stylesheets: &[(String, String)],
        environment: MediaEnvironment,
    ) -> Self {
        Self::from_document_for_media_environment(
            &dom.document,
            document_base_url,
            external_stylesheets,
            environment,
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
            false,
        )
    }

    pub(crate) fn from_document_for_viewport(
        document: &NodeRef,
        document_base_url: &str,
        external_stylesheets: &[(String, String)],
        viewport_width: f32,
        viewport_height: f32,
        prefers_dark_color_scheme: bool,
    ) -> Self {
        Self::from_document_for_media_environment(
            document,
            document_base_url,
            external_stylesheets,
            MediaEnvironment::new(
                viewport_width,
                viewport_height,
                1.0,
                prefers_dark_color_scheme,
            ),
        )
    }

    fn from_document_for_media_environment(
        document: &NodeRef,
        document_base_url: &str,
        external_stylesheets: &[(String, String)],
        environment: MediaEnvironment,
    ) -> Self {
        let mut set = Self::for_computed_style_for_media_environment(
            document,
            document_base_url,
            external_stylesheets,
            environment,
        );
        set.compute_subtree(document, None);
        set
    }

    pub(crate) fn for_computed_style_for_media_environment(
        document: &NodeRef,
        document_base_url: &str,
        external_stylesheets: &[(String, String)],
        environment: MediaEnvironment,
    ) -> Self {
        let rules = sheets::collect(
            document,
            document_base_url,
            external_stylesheets,
            environment,
        );
        let rule_index = RuleIndex::new(&rules);
        Self {
            styles: HashMap::new(),
            pseudo_styles: HashMap::new(),
            generated_nodes: HashMap::new(),
            generated_styles: HashMap::new(),
            rules,
            rule_index,
            document_base_url: document_base_url.to_string(),
            viewport_width: environment.viewport_width,
            viewport_height: environment.viewport_height,
        }
    }

    pub(crate) fn clear_computed_styles(&mut self) {
        self.styles.clear();
        self.pseudo_styles.clear();
        self.generated_styles.clear();
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
                self.sync_generated_pseudos(&ancestor, &style);
                parent_style = Some(style);
            }
        }
        self.styles.get(&node_id(node))
    }

    pub fn get(&self, node: &NodeRef) -> &ComputedStyle {
        self.styles
            .get(&node_id(node))
            .or_else(|| self.generated_styles.get(&node_id(node)))
            .expect("style should exist for every DOM node")
    }

    /// Uses the engine selector parser for opt-in composed-page inspection. This deliberately
    /// crosses shadow boundaries for browser diagnostics; DOM query APIs retain tree scoping.
    pub fn query_selector_all(&self, dom: &Dom, input: &str) -> Option<Vec<NodeRef>> {
        let selector = parse_selector(input.trim())?;
        Some(
            dom::Node::shadow_including_descendants(&dom.document)
                .filter(|node| selector_matches(&selector, node))
                .collect(),
        )
    }

    fn compute_style(&self, node: &NodeRef, parent: Option<&ComputedStyle>) -> ComputedStyle {
        let mut style = ComputedStyle::inherit_from(parent);
        style.root_font_size = root_font_size_for(&self.styles, node);
        apply_user_agent_defaults(node, &mut style);
        let lower_origin = style.clone();

        let matching = self.matching_rules(node, None);
        let inline_declarations = node
            .attr("style")
            .map(|inline| parse_declarations(&inline))
            .unwrap_or_default();

        self.apply_author_cascade(
            &mut style,
            parent,
            &lower_origin,
            &matching,
            &inline_declarations,
        );
        apply_presentational_hints(node, &mut style);
        style.resolve_relative_units(
            self.viewport_width,
            self.viewport_height,
            style.root_font_size,
        );
        if node.attr("hidden").is_some() || is_hidden_by_html_rendering(node) {
            style.display = Display::None;
        }
        super::fullscreen::apply_fullscreen_ua_style(
            node,
            &mut style,
            self.viewport_width,
            self.viewport_height,
        );
        // CSS 2 makes `float` compute to `none` for absolutely positioned boxes. Resolve this
        // after the cascade so the result is independent of declaration source order.
        if matches!(style.position, Position::Absolute | Position::Fixed) {
            style.float = Float::None;
        }
        style.line_height = style.line_height.max(style.font_size);
        style
    }

    fn matching_rules(&self, node: &NodeRef, pseudo: Option<PseudoElement>) -> Vec<&Rule> {
        let mut matching = self
            .rule_index
            .candidates(node)
            .into_iter()
            .filter_map(|index| self.rules.get(index))
            .filter(|rule| rule.pseudo == pseudo)
            .filter(|rule| rule_applies_to(rule, node))
            .filter(|rule| selector_matches(&rule.selector, node))
            .collect::<Vec<_>>();
        matching.sort_by(|left, right| {
            left.selector
                .specificity
                .cmp(&right.selector.specificity)
                .then_with(|| left.order.cmp(&right.order))
        });
        matching
    }

    fn apply_author_cascade(
        &self,
        style: &mut ComputedStyle,
        parent: Option<&ComputedStyle>,
        lower_origin: &ComputedStyle,
        matching: &[&Rule],
        inline_declarations: &[Declaration],
    ) {
        // CSS Cascade places every important author declaration above every normal author
        // declaration. Inline declarations retain their higher specificity within each group.
        // https://drafts.csswg.org/css-cascade/#importance
        let mut cascaded = Vec::new();
        for important in [false, true] {
            for rule in matching {
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
            apply_custom_properties(style, std::slice::from_ref(declaration), parent);
        }
        for line_height in [false, true] {
            for &(declaration, base_url) in &cascaded {
                if (declaration.name == "line-height") == line_height {
                    apply_resolved_declaration(
                        style,
                        declaration,
                        parent,
                        lower_origin,
                        base_url,
                        self.viewport_width,
                        self.viewport_height,
                    );
                }
            }
        }
    }
}

fn rule_applies_to(rule: &Rule, node: &NodeRef) -> bool {
    let scope_matches = match rule.scope {
        RuleScope::Document => !matches!(Node::tree_root(node).data, NodeData::ShadowRoot(_)),
        RuleScope::Shadow(root) => Node::tree_root(node).id() == root,
        RuleScope::Host(root) => node.shadow_root().is_some_and(|shadow| shadow.id() == root),
        RuleScope::Slotted(root) => {
            Node::assigned_slot(node).is_some_and(|slot| Node::tree_root(&slot).id() == root)
        }
    };
    scope_matches
        && rule.host_condition.as_ref().is_none_or(|condition| {
            Node::tree_root(node)
                .shadow_host()
                .is_some_and(|host| selector_matches(condition, &host))
        })
}

fn node_id(node: &NodeRef) -> NodeId {
    node.id()
}
