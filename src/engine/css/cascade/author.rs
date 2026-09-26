//! Author-origin cascade ordering, including CSS Cascade Level 5 layers.
use super::*;
use std::collections::HashSet;

#[derive(Clone, Copy, PartialEq, Eq)]
struct LayerKey {
    band: u8,
    context: u8,
    rank: u32,
}

struct Cascaded<'a> {
    declaration: &'a Declaration,
    base_url: &'a str,
    layer: LayerKey,
}

pub(super) struct AuthorCascadeInput<'a> {
    pub(super) node: &'a NodeRef,
    pub(super) parent: Option<&'a ComputedStyle>,
    pub(super) lower_origin: &'a ComputedStyle,
    pub(super) matching: &'a [&'a Rule],
    pub(super) inline_declarations: &'a [Declaration],
    pub(super) animation_declarations: &'a [Declaration],
}

impl StyleSet {
    pub(super) fn apply_author_cascade(
        &self,
        style: &mut ComputedStyle,
        input: AuthorCascadeInput<'_>,
    ) {
        let AuthorCascadeInput {
            node,
            parent,
            lower_origin,
            matching,
            inline_declarations,
            animation_declarations,
        } = input;
        let mut cascaded = Vec::new();
        let scoped = matching
            .iter()
            .any(|rule| !matches!(rule.scope, RuleScope::Document));
        let outer_context = if scoped {
            shadow_context_depth(&Node::tree_root(node))
        } else {
            0
        };
        let slot_context = if matching
            .iter()
            .any(|rule| matches!(rule.scope, RuleScope::Slotted(_)))
        {
            Node::assigned_slot(node)
                .map(|slot| shadow_context_depth(&Node::tree_root(&slot)))
                .unwrap_or(outer_context)
        } else {
            outer_context
        };
        let mut rules = matching
            .iter()
            .map(|rule| {
                let context = match rule.scope {
                    RuleScope::Document => 0,
                    RuleScope::Shadow(_) => outer_context,
                    RuleScope::Host(_) => outer_context.saturating_add(1),
                    RuleScope::Slotted(_) => slot_context,
                };
                let proximity = scope::proximity(rule, node).unwrap_or(usize::MAX);
                (*rule, context, proximity)
            })
            .collect::<Vec<_>>();
        let layered = rules.iter().any(|(rule, _, _)| rule.layer_rank != u32::MAX);
        let mixed_context = rules
            .first()
            .is_some_and(|first| rules.iter().any(|rule| rule.1 != first.1));
        rules.sort_by(
            |(left, left_context, left_proximity), (right, right_context, right_proximity)| {
                right_context
                    .cmp(left_context)
                    .then_with(|| left.layer_rank.cmp(&right.layer_rank))
                    .then_with(|| left.selector.specificity.cmp(&right.selector.specificity))
                    .then_with(|| right_proximity.cmp(left_proximity))
                    .then_with(|| left.order.cmp(&right.order))
            },
        );
        for important in [false, true] {
            if important && (layered || mixed_context) {
                rules.sort_by(
                    |(left, left_context, left_proximity),
                     (right, right_context, right_proximity)| {
                        left_context
                            .cmp(right_context)
                            .then_with(|| right.layer_rank.cmp(&left.layer_rank))
                            .then_with(|| {
                                left.selector.specificity.cmp(&right.selector.specificity)
                            })
                            .then_with(|| right_proximity.cmp(left_proximity))
                            .then_with(|| left.order.cmp(&right.order))
                    },
                );
            }
            let mut inline = inline_declarations
                .iter()
                .filter(|declaration| declaration.important == important)
                .map(|declaration| Cascaded {
                    declaration,
                    base_url: &self.document_base_url,
                    layer: LayerKey {
                        band: if important { 4 } else { 1 },
                        context: outer_context,
                        rank: 0,
                    },
                });
            let mut inserted_inline = false;
            for (rule, context, _) in &rules {
                if important && !inserted_inline && *context > outer_context {
                    cascaded.extend(inline.by_ref());
                    inserted_inline = true;
                }
                cascaded.extend(
                    rule.declarations
                        .iter()
                        .filter(|declaration| declaration.important == important)
                        .map(|declaration| Cascaded {
                            declaration,
                            base_url: &rule.base_url,
                            layer: LayerKey {
                                band: if important { 3 } else { 0 },
                                context: *context,
                                rank: rule.layer_rank,
                            },
                        }),
                );
            }
            cascaded.extend(inline);
            if !important {
                // Animation origin is above normal author declarations, below important author.
                cascaded.extend(
                    animation_declarations
                        .iter()
                        .filter(|declaration| !declaration.important)
                        .map(|declaration| Cascaded {
                            declaration,
                            base_url: &self.document_base_url,
                            layer: LayerKey {
                                band: 2,
                                context: outer_context,
                                rank: 0,
                            },
                        }),
                );
            }
        }

        // Most pages never use revert-layer. Do not clone a computed style at every layer
        // boundary on their hot style-recalculation path.
        let needs_snapshot = cascaded
            .iter()
            .any(|item| item.declaration.possible_revert_layer);
        // Resolve custom properties before var() substitution. For important rules the rollback
        // baseline is the normal cascade *before* that layer, plus earlier important layers;
        // the normal declaration in the same layer and all intervening normal layers are ignored.
        let custom_origin = style.clone();
        let mut normal_history = Vec::new();
        let mut normal_author_end = custom_origin.clone();
        let mut important_custom_names: HashSet<String> = HashSet::new();
        let mut previous = None;
        let mut layer_start = None;
        for item in &cascaded {
            if previous != Some(item.layer) {
                if needs_snapshot
                    && let Some(prior) = previous
                    && prior.band == 0
                {
                    normal_history.push((prior, style.clone()));
                    normal_author_end = style.clone();
                }
                if needs_snapshot {
                    let mut baseline = match item.layer.band {
                        3 => normal_prefix(&normal_history, &custom_origin, item.layer),
                        4 => normal_author_end.clone(),
                        _ => style.clone(),
                    };
                    if item.layer.band >= 3 {
                        for name in &important_custom_names {
                            if let Some(value) = style.custom_properties.get(name).cloned() {
                                Arc::make_mut(&mut baseline.custom_properties)
                                    .insert(name.clone(), value);
                            } else {
                                Arc::make_mut(&mut baseline.custom_properties).remove(name);
                            }
                        }
                    }
                    layer_start = Some(baseline);
                }
                previous = Some(item.layer);
            }
            apply_custom_properties(
                style,
                std::slice::from_ref(item.declaration),
                parent,
                layer_start.as_ref().unwrap_or(lower_origin),
            );
            if item.layer.band >= 3 && item.declaration.name.starts_with("--") {
                important_custom_names.insert(item.declaration.name.clone());
            }
        }

        let needs_snapshot = needs_snapshot
            || (cascaded.iter().any(|item| item.declaration.may_use_var)
                && style
                    .custom_properties
                    .values()
                    .any(|value| might_revert_layer(value)));
        let property_origin = style.clone();
        normal_history.clear();
        normal_author_end = property_origin.clone();
        let mut important_starts: Vec<(LayerKey, ComputedStyle)> = Vec::new();
        let mut prior_important = Vec::new();
        previous = None;
        // Font-relative line height resolves after the cascade. Do not move declarations past
        // later font shorthands or change importance ordering.
        for item in &cascaded {
            if previous != Some(item.layer) {
                if needs_snapshot {
                    if let Some(prior) = previous
                        && prior.band == 0
                    {
                        normal_history.push((prior, style.clone()));
                        normal_author_end = style.clone();
                    }
                    let mut baseline = match item.layer.band {
                        3 => normal_prefix(&normal_history, &property_origin, item.layer),
                        4 => normal_author_end.clone(),
                        _ => style.clone(),
                    };
                    if item.layer.band >= 3 {
                        for earlier in &prior_important {
                            let earlier: &&Cascaded<'_> = earlier;
                            let earlier_start = important_starts
                                .iter()
                                .rev()
                                .find(|(key, _)| *key == earlier.layer)
                                .map(|(_, start)| start)
                                .unwrap_or(lower_origin);
                            apply_resolved_declaration(
                                &mut baseline,
                                earlier.declaration,
                                DeclarationContext {
                                    parent,
                                    lower_origin,
                                    layer_start: earlier_start,
                                    base_url: earlier.base_url,
                                    viewport_width: self.viewport_width,
                                    viewport_height: self.viewport_height,
                                },
                            );
                        }
                        important_starts.push((item.layer, baseline.clone()));
                    }
                    layer_start = Some(baseline);
                }
                previous = Some(item.layer);
            }
            apply_resolved_declaration(
                style,
                item.declaration,
                DeclarationContext {
                    parent,
                    lower_origin,
                    layer_start: layer_start.as_ref().unwrap_or(lower_origin),
                    base_url: item.base_url,
                    viewport_width: self.viewport_width,
                    viewport_height: self.viewport_height,
                },
            );
            if needs_snapshot && item.layer.band >= 3 {
                prior_important.push(item);
            }
        }
    }
}

fn normal_prefix(
    history: &[(LayerKey, ComputedStyle)],
    origin: &ComputedStyle,
    before: LayerKey,
) -> ComputedStyle {
    history
        .iter()
        .rev()
        .find(|(key, _)| {
            key.context > before.context
                || (key.context == before.context && key.rank < before.rank)
        })
        .map_or_else(|| origin.clone(), |(_, style)| style.clone())
}

fn shadow_context_depth(root: &NodeRef) -> u8 {
    let mut depth = 0_u8;
    let mut current = root.clone();
    while let Some(host) = current.shadow_host() {
        depth = depth.saturating_add(1);
        current = Node::tree_root(&host);
    }
    depth
}

fn might_revert_layer(value: &str) -> bool {
    value.as_bytes().contains(&b'\\')
        || value
            .as_bytes()
            .windows(b"revert-layer".len())
            .any(|part| part.eq_ignore_ascii_case(b"revert-layer"))
}
