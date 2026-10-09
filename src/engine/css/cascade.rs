//! Style-set construction and cascade ordering.

mod author;
mod computed;
mod keyframe_values;
mod layout;
mod matching;
mod parts;
mod presentational;
mod pseudo;
mod refresh;
mod root_units;
mod scope;
mod sheets;
mod sources;
mod svg_presentation;
pub use sources::StylesheetSource;
#[cfg(test)]
mod anonymous_text_tests;
#[cfg(test)]
mod tests;

use super::media::MediaEnvironment;
use super::selector_match::selector_matches;
use super::*;
use author::AuthorCascadeInput;
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
    /// Changes outside color/background/underline, including unknown or generated paint.
    pub non_deferable_paint_changes: bool,
    pub full_rebuild: bool,
    pub element_style_time: std::time::Duration,
    pub pseudo_style_time: std::time::Duration,
}

#[derive(Debug, Default)]
pub struct StyleSet {
    pub styles: HashMap<NodeId, ComputedStyle>,
    pseudo_styles: HashMap<(NodeId, PseudoElement), ComputedStyle>,
    generated_nodes: HashMap<(NodeId, PseudoElement), NodeRef>,
    generated_styles: HashMap<NodeId, ComputedStyle>,
    compiled: std::rc::Rc<sheets::CompiledRules>,
    ancestor_filters: std::cell::RefCell<super::selector_match::AncestorFilterCache>,
    defer_nonrendered_descendants: bool,
    deferred_fullscreen_roots: HashSet<NodeId>,
    document_base_url: String,
    viewport_width: f32,
    viewport_height: f32,
    resolution_dppx: f32,
}

impl StyleSet {
    pub fn from_dom(dom: &Dom, external_stylesheets: &[String], viewport_width: f32) -> Self {
        let sources = external_stylesheets
            .iter()
            .map(|stylesheet| StylesheetSource::injected("", stylesheet.clone()))
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
        let sources = external_stylesheets
            .iter()
            .map(|(url, source)| StylesheetSource::injected(url, source.clone()))
            .collect::<Vec<_>>();
        Self::from_sources_for_media_environment(
            dom,
            document_base_url,
            &sources,
            MediaEnvironment::new(viewport_width, viewport_height, 1.0, false),
        )
    }

    pub(crate) fn from_sources_for_media_environment(
        dom: &Dom,
        document_base_url: &str,
        external_stylesheets: &[crate::engine::css::StylesheetSource],
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
        external_stylesheets: &[crate::engine::css::StylesheetSource],
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
        external_stylesheets: &[crate::engine::css::StylesheetSource],
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
        external_stylesheets: &[crate::engine::css::StylesheetSource],
        environment: MediaEnvironment,
    ) -> Self {
        let mut set = Self::for_computed_style_for_media_environment(
            document,
            document_base_url,
            external_stylesheets,
            environment,
        );
        set.compute_subtree(document, None);
        set.compute_independent_fullscreen_roots(document);
        set
    }

    pub(crate) fn for_computed_style_for_media_environment(
        document: &NodeRef,
        document_base_url: &str,
        external_stylesheets: &[crate::engine::css::StylesheetSource],
        environment: MediaEnvironment,
    ) -> Self {
        let compiled = sheets::collect(
            document,
            document_base_url,
            external_stylesheets,
            environment,
        );
        Self {
            styles: HashMap::new(),
            pseudo_styles: HashMap::new(),
            generated_nodes: HashMap::new(),
            generated_styles: HashMap::new(),
            compiled,
            ancestor_filters: std::cell::RefCell::default(),
            defer_nonrendered_descendants: false,
            deferred_fullscreen_roots: HashSet::new(),
            document_base_url: document_base_url.to_string(),
            viewport_width: environment.viewport_width,
            viewport_height: environment.viewport_height,
            resolution_dppx: environment.resolution_dppx,
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
}

fn node_id(node: &NodeRef) -> NodeId {
    node.id()
}
