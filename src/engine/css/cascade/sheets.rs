//! Immutable compiled-rule sharing between a document's independent style consumers.
use super::*;
use crate::engine::css::layers::{LayerPath, import_owner_path};
use crate::engine::css::media::MediaEnvironment;
use crate::engine::css::rule_index::RuleIndex;
use std::rc::Rc;

mod animations;
mod fonts;
mod invalidation;
mod owners;
mod parsed;
#[cfg(test)]
mod rebuild_tests;
mod registry;
mod transitions;

#[derive(Debug, PartialEq, Eq, Hash)]
struct SheetInput {
    source: String,
    base_url: String,
    scope: RuleScope,
    implicit_scope_root: Option<NodeId>,
    layer_prefix: LayerPath,
    import_scope_prefixes: Vec<String>,
    declared_layer: Option<LayerPath>,
}

#[derive(Debug, Default)]
pub(super) struct CompiledRules {
    pub(super) rules: Vec<Rule>,
    transition_rule_indices: Vec<usize>,
    animation_rule_indices: Vec<usize>,
    keyframes: Vec<crate::engine::css::stylesheet::keyframes::KeyframeDefinition>,
    font_faces: Vec<crate::engine::font::WebFontFace>,
    scope_parents: std::collections::HashMap<NodeId, Option<NodeId>>,
    pub(super) index: RuleIndex,
    inputs: Vec<Rc<SheetInput>>,
    parsed: Vec<Rc<parsed::ParsedSheet>>,
    environment: Option<MediaEnvironment>,
}

impl StyleSet {
    /// Recompute every element after rule changes while retaining previous values for exact
    /// equality checks and generated-box reconciliation. No old rule matches are reused.
    pub(crate) fn rebuild_rules_for_media_environment(
        &mut self,
        dom: &Dom,
        base_url: &str,
        external_stylesheets: &[crate::engine::css::StylesheetSource],
        environment: MediaEnvironment,
        removed_nodes: &[NodeId],
    ) -> StyleRefreshStats {
        let viewport_changed = self.viewport_width != environment.viewport_width
            || self.viewport_height != environment.viewport_height
            || self.resolution_dppx != environment.resolution_dppx;
        let (removed_styles, removed_generated) = self.prune_uncomposed_styles(dom);
        self.compiled = collect(&dom.document, base_url, external_stylesheets, environment);
        self.document_base_url = base_url.to_string();
        self.viewport_width = environment.viewport_width;
        self.viewport_height = environment.viewport_height;
        self.resolution_dppx = environment.resolution_dppx;
        let mut stats = self.refresh_subtrees(
            &dom.document,
            std::slice::from_ref(&dom.document),
            removed_nodes,
        );
        stats.removed_styles += removed_styles;
        // Auto/percentage used sizes can change with the containing viewport even when every
        // computed style remains equal. A stylesheet equality check cannot suppress that layout.
        stats.layout_changed |= viewport_changed || removed_styles != 0 || removed_generated;
        stats.non_deferable_paint_changes |=
            viewport_changed || removed_styles != 0 || removed_generated;
        stats.full_rebuild = true;
        stats
    }
}

pub(super) fn collect(
    document: &NodeRef,
    document_base_url: &str,
    external_stylesheets: &[crate::engine::css::StylesheetSource],
    environment: MediaEnvironment,
) -> Rc<CompiledRules> {
    let mut inputs = Vec::new();
    owners::append(
        document,
        document_base_url,
        external_stylesheets,
        environment,
        &mut inputs,
        RuleScope::Document,
    );
    for source in external_stylesheets
        .iter()
        .filter(|source| source.owner_url.is_none())
    {
        let implicit_scope_root = default_scope_root(document);
        let owner = inputs.len() as u32;
        let imports = crate::engine::css::imports::expand(
            &source.base_url,
            &source.imports,
            external_stylesheets,
            environment,
        );
        let mut declarations = imports.layer_declarations.into_iter().peekable();
        for (index, imported) in imports.sheets.into_iter().enumerate() {
            while declarations.peek().is_some_and(|(at, _)| *at == index) {
                let (_, layer) = declarations.next().unwrap();
                inputs.push(SheetInput::layer_marker(
                    import_owner_path(&layer, owner),
                    RuleScope::Document,
                ));
            }
            inputs.push(SheetInput {
                source: imported.source.clone(),
                base_url: imported.base_url.clone(),
                scope: RuleScope::Document,
                implicit_scope_root,
                layer_prefix: import_owner_path(&imported.layer_prefix, owner),
                import_scope_prefixes: imported.scope_prefixes,
                declared_layer: None,
            });
        }
        for (_, layer) in declarations {
            inputs.push(SheetInput::layer_marker(
                import_owner_path(&layer, owner),
                RuleScope::Document,
            ));
        }
        inputs.push(SheetInput {
            source: source.source.clone(),
            base_url: source.base_url.clone(),
            scope: RuleScope::Document,
            implicit_scope_root,
            layer_prefix: Vec::new(),
            import_scope_prefixes: Vec::new(),
            declared_layer: None,
        });
    }
    append_adopted(document, environment, &mut inputs, RuleScope::Document);
    let mut scope_parents = std::collections::HashMap::new();
    for shadow in Node::shadow_including_descendants(document)
        .filter(|node| matches!(node.data, NodeData::ShadowRoot(_)))
    {
        if let Some(host) = shadow.shadow_host() {
            let parent = Node::tree_root(&host);
            scope_parents.insert(shadow.id(), parent.shadow_host().map(|_| parent.id()));
        }
        owners::append(
            &shadow,
            document_base_url,
            external_stylesheets,
            environment,
            &mut inputs,
            RuleScope::Shadow(shadow.id()),
        );
        append_adopted(
            &shadow,
            environment,
            &mut inputs,
            RuleScope::Shadow(shadow.id()),
        );
    }
    let mut inputs = inputs.into_iter().map(Rc::new).collect::<Vec<_>>();
    let candidates = registry::candidates(document.id());
    if let Some(cached) = candidates.iter().find(|cached| {
        cached.environment == Some(environment)
            && cached.inputs == inputs
            // Empty shadow roots must not force static pages into full rule rebuilds.
            // Scope ancestry is only observed when a keyframe definition can be referenced.
            && (cached.keyframes.is_empty() || cached.scope_parents == scope_parents)
    }) {
        registry::remember(document.id(), cached);
        return Rc::clone(cached);
    }
    let previous = candidates
        .iter()
        .find(|set| set.environment == Some(environment));
    let (rules, parsed) = parsed::assemble(&mut inputs, environment, previous.map(Rc::as_ref));
    let transition_rule_indices = transitions::rule_indices(&rules);
    let animation_rule_indices = animations::rule_indices(&rules);
    let keyframes = animations::collect(&inputs, environment);
    let font_faces = fonts::collect(&inputs, environment);
    let compiled = Rc::new(CompiledRules {
        index: RuleIndex::new(&rules),
        rules,
        transition_rule_indices,
        animation_rule_indices,
        keyframes,
        font_faces,
        scope_parents,
        inputs,
        parsed,
        environment: Some(environment),
    });
    registry::remember(document.id(), &compiled);
    compiled
}

fn append_adopted(
    root: &NodeRef,
    environment: MediaEnvironment,
    inputs: &mut Vec<SheetInput>,
    scope: RuleScope,
) {
    let implicit_scope_root = default_scope_root(root);
    for sheet in root.adopted_stylesheets() {
        if !sheet.media.trim().is_empty()
            && !media::media_matches_for_environment(&sheet.media, environment)
        {
            continue;
        }
        inputs.push(SheetInput {
            source: sheet.source,
            base_url: sheet.base_url,
            scope,
            implicit_scope_root,
            layer_prefix: Vec::new(),
            import_scope_prefixes: Vec::new(),
            declared_layer: None,
        });
    }
}

impl SheetInput {
    fn layer_marker(path: LayerPath, scope: RuleScope) -> Self {
        Self {
            source: String::new(),
            base_url: String::new(),
            scope,
            implicit_scope_root: None,
            layer_prefix: Vec::new(),
            import_scope_prefixes: Vec::new(),
            declared_layer: Some(path),
        }
    }
}

fn default_scope_root(root: &NodeRef) -> Option<NodeId> {
    // CSS Cascade 6 §3.5.4 uses the containing node-tree root when a sheet has no
    // owner parent. For a document-adopted sheet this is the Document node, not html.
    root.shadow_host().map(|host| host.id()).or(Some(root.id()))
}

#[cfg(test)]
mod tests;
