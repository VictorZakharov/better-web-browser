//! Keyframe lookup starts at the winning name declaration's scope, then enclosing scopes.
use super::*;
use crate::engine::css::layers::{LayerRegistry, occurrence_path};
use crate::engine::css::stylesheet::keyframes::{KeyframeDefinition, MAX_KEYFRAME_DEFINITIONS};
#[cfg(test)]
mod tests;

pub(super) fn collect(
    inputs: &[Rc<SheetInput>],
    parsed: &[Rc<parsed::ParsedSheet>],
    environment: MediaEnvironment,
) -> Vec<KeyframeDefinition> {
    let mut output = Vec::new();
    let mut registries = std::collections::HashMap::<RuleScope, LayerRegistry>::new();
    let mut assignments = Vec::new();
    let parsed_by_input = parsed
        .iter()
        .map(|sheet| (Rc::as_ptr(&sheet.input), sheet.as_ref()))
        .collect::<std::collections::HashMap<_, _>>();
    for (occurrence, input) in inputs.iter().enumerate() {
        let registry = registries.entry(input.scope).or_default();
        if let Some(path) = &input.declared_layer {
            registry.declare(path);
        }
        let cached = parsed_by_input
            .get(&Rc::as_ptr(input))
            .map(|sheet| sheet.keyframes(environment));
        let fallback = cached.is_none().then(|| {
            crate::engine::css::stylesheet::keyframes::collect_with_layers(
                &input.source,
                environment,
                input.scope,
            )
        });
        let (definitions, layers) = if let Some(cached) = cached {
            (&cached.0, &cached.1)
        } else {
            let fallback = fallback.as_ref().expect("uncached keyframe collection");
            (&fallback.definitions, &fallback.layers)
        };
        for path in layers {
            let mut full = input.layer_prefix.clone();
            full.extend(occurrence_path(path, occurrence as u32));
            registry.declare(&full);
        }
        for mut definition in definitions.iter().cloned() {
            if output.len() == MAX_KEYFRAME_DEFINITIONS {
                break;
            }
            let mut path = input.layer_prefix.clone();
            path.extend(occurrence_path(&definition.layer, occurrence as u32));
            let assignment = (!path.is_empty())
                .then(|| registry.declare(&path))
                .flatten();
            assignments.push(assignment.map(|id| (input.scope, id)));
            definition.layer = path;
            output.push(definition);
        }
    }
    let ranks = registries
        .into_iter()
        .map(|(scope, registry)| (scope, registry.ranks()))
        .collect::<std::collections::HashMap<_, _>>();
    for (definition, assignment) in output.iter_mut().zip(assignments) {
        if let Some((scope, id)) = assignment {
            definition.layer_rank = ranks[&scope][id];
        }
    }
    output
}

impl StyleSet {
    pub(crate) fn has_keyframe_definitions(&self) -> bool {
        !self.compiled.keyframes.is_empty()
    }
    #[cfg(test)]
    pub(crate) fn animation_keyframes(
        &self,
        node: &NodeRef,
        name: &str,
    ) -> Option<&KeyframeDefinition> {
        let root = Node::tree_root(node);
        self.animation_keyframes_for_scope(root.shadow_host().map(|_| root.id()), name)
    }

    pub(crate) fn animation_keyframes_for_scope(
        &self,
        mut name_scope: Option<NodeId>,
        name: &str,
    ) -> Option<&KeyframeDefinition> {
        loop {
            let scope = name_scope.map_or(RuleScope::Document, RuleScope::Shadow);
            if let Some(definition) = self
                .compiled
                .keyframes
                .iter()
                .enumerate()
                .filter(|(_, definition)| definition.scope == scope && definition.name == name)
                // Source order resolves equal ranks; layer precedence resolves collisions.
                .max_by_key(|(index, definition)| (definition.layer_rank, *index))
                .map(|(_, definition)| definition)
            {
                return Some(definition);
            }
            name_scope = self
                .compiled
                .scope_parents
                .get(&name_scope?)
                .copied()
                .flatten();
        }
    }

    pub(crate) fn may_have_css_animation(&self, node: &NodeRef) -> bool {
        if node.element().is_none() {
            return false;
        }
        // A cheap filter prevents computing every element's style on static pages.
        // Scanned in place: the snapshot visits every element after each DOM change.
        let mentions = |style: &str, word: &str| {
            style
                .as_bytes()
                .windows(word.len())
                .any(|window| window.eq_ignore_ascii_case(word.as_bytes()))
        };
        if node
            .attr_ref("style")
            .is_some_and(|style| mentions(&style, "animation") || mentions(&style, "all"))
        {
            return true;
        }
        // A descendant universal such as `.dialog *` is not a document-wide candidate.
        // Reuse cascade matching, including @scope, :has(), shadow hosts and slot/part
        // exposure. No match cache survives a DOM version change; pending style changes
        // must be reflected before getAnimations() (CSS Animations 2 §6.2).
        let tree_root = std::cell::OnceCell::new();
        let ancestors = std::cell::OnceCell::new();
        // The snapshot asks this of every element, so candidates are tested without collecting.
        self.compiled
            .animation_index
            .any_candidate(node, None, |index| {
                let rule = &self.compiled.rules[index];
                let tree_root = tree_root.get_or_init(|| Node::tree_root(node));
                self.rule_matches(rule, node, None, tree_root, &ancestors)
            })
    }
}

pub(super) fn rule_indices(rules: &[Rule]) -> Vec<usize> {
    rules
        .iter()
        .enumerate()
        .filter(|(_, rule)| {
            rule.declarations.iter().any(|declaration| {
                declaration.name == "all"
                    || crate::engine::css::values::animations::supported_property(&declaration.name)
            })
        })
        .map(|(index, _)| index)
        .collect()
}
