//! Keyframe lookup starts at the winning name declaration's scope, then enclosing scopes.
use super::*;
use crate::engine::css::layers::{LayerRegistry, occurrence_path};
use crate::engine::css::stylesheet::keyframes::{KeyframeDefinition, MAX_KEYFRAME_DEFINITIONS};

pub(super) fn collect(
    inputs: &[Rc<SheetInput>],
    environment: MediaEnvironment,
) -> Vec<KeyframeDefinition> {
    let mut output = Vec::new();
    let mut registries = std::collections::HashMap::<RuleScope, LayerRegistry>::new();
    let mut assignments = Vec::new();
    for (occurrence, input) in inputs.iter().enumerate() {
        let registry = registries.entry(input.scope).or_default();
        if let Some(path) = &input.declared_layer {
            registry.declare(path);
        }
        let collection = crate::engine::css::stylesheet::keyframes::collect_with_layers(
            &input.source,
            environment,
            input.scope,
        );
        for path in collection.layers {
            let mut full = input.layer_prefix.clone();
            full.extend(occurrence_path(&path, occurrence as u32));
            registry.declare(&full);
        }
        for mut definition in collection.definitions {
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
        // A cheap filter prevents computing every element's style on static pages.
        if node.attr("style").is_some_and(|style| {
            let style = style.to_ascii_lowercase();
            style.contains("animation") || style.contains("all")
        }) {
            return true;
        }
        self.compiled.animation_rule_indices.iter().any(|&index| {
            let rule = &self.compiled.rules[index];
            if rule.pseudo.is_some() {
                return false;
            }
            let Some(compound) = rule.selector.compounds.last() else {
                return true;
            };
            compound
                .tag
                .as_deref()
                .is_none_or(|tag| node.tag_name() == Some(tag))
                && compound
                    .id
                    .as_deref()
                    .is_none_or(|id| node.attr("id").as_deref() == Some(id))
                && compound.classes.iter().all(|class| {
                    node.attr("class").is_some_and(|value| {
                        value.split_ascii_whitespace().any(|part| part == class)
                    })
                })
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
