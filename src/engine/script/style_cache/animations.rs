//! Bounded animation discovery from the same compiled cascade used for layout.
use super::*;
use crate::engine::css::stylesheet::keyframes::KeyframeDefinition;

#[derive(Default)]
struct SnapshotBudget {
    bytes: usize,
    values: usize,
}

impl SnapshotBudget {
    fn reserve(&mut self, bytes: usize, values: usize) -> bool {
        let bytes = self.bytes.saturating_add(bytes);
        let values = self.values.saturating_add(values);
        // Keep the bridge bounded independently of each stylesheet/target's own limits.
        if bytes > 1_048_576 || values > 20_000 {
            return false;
        }
        self.bytes = bytes;
        self.values = values;
        true
    }
}

fn frames(
    definition: &KeyframeDefinition,
    styles: &mut StyleSet,
    node: &NodeRef,
    budget: &mut SnapshotBudget,
) -> Option<JsValue> {
    let mut output = Vec::new();
    for block in definition.merged_blocks() {
        let declarations = styles.resolved_keyframe_values(node, &block.declarations);
        let bytes = declarations
            .iter()
            .map(|(name, value)| name.len() + value.len())
            .sum();
        if !budget.reserve(bytes, 3 + declarations.len() * 3) {
            return None;
        }
        output.push(JsValue::Array(vec![
            JsValue::from(f64::from(block.offsets[0])),
            JsValue::Array(
                declarations
                    .into_iter()
                    .map(|(name, value)| {
                        JsValue::Array(vec![JsValue::from(name), JsValue::from(value)])
                    })
                    .collect(),
            ),
        ]));
    }
    Some(JsValue::Array(output))
}

impl HostState {
    pub(in crate::engine::script) fn css_animation_rendered(&mut self, node: &NodeRef) -> bool {
        let (version, mut styles) = self.take_computed_styles();
        let mut current = Some(node.clone());
        let mut rendered = self.is_connected(node);
        while let Some(ancestor) = current {
            if styles
                .computed_style_for_node(&ancestor)
                .is_some_and(|style| style.display == crate::engine::css::Display::None)
            {
                rendered = false;
                break;
            }
            current = Node::composed_parent(&ancestor);
        }
        self.computed_styles = Some((version, styles));
        rendered
    }
    pub(in crate::engine::script) fn transition_affected_targets(
        &mut self,
        root: &NodeRef,
        name: &str,
        next: &str,
    ) -> JsValue {
        let (version, styles) = self.take_computed_styles();
        let mut nodes = Vec::new();
        if styles.may_transition_on_attribute(root, name, next) {
            nodes.push(root.clone());
        }
        if styles.has_transition_rules() {
            for node in Node::composed_descendants(root).skip(1).take(100_000) {
                if node.element().is_some() && styles.may_transition_on_attribute(&node, "", "") {
                    nodes.push(node);
                    if nodes.len() == 64 {
                        break;
                    }
                }
            }
        }
        for node in self.inline_transitions.descendants(root) {
            if nodes.len() == 64 {
                break;
            }
            if !nodes.iter().any(|previous| previous.id() == node.id()) {
                nodes.push(node);
            }
        }
        self.computed_styles = Some((version, styles));
        JsValue::Array(
            nodes
                .into_iter()
                .map(|node| JsValue::from(self.id_for(&node)))
                .collect(),
        )
    }
    pub(in crate::engine::script) fn css_animation_underlying(
        &mut self,
        node: &NodeRef,
        properties: &[String],
    ) -> JsValue {
        let (version, mut styles) = self.take_computed_styles();
        let underlying = styles.underlying_style_for_node(node);
        let values = JsValue::Array(
            properties
                .iter()
                .map(|name| {
                    JsValue::from(resolved_property_value(&underlying, name).unwrap_or_default())
                })
                .collect(),
        );
        self.computed_styles = Some((version, styles));
        values
    }
    pub(in crate::engine::script) fn css_animation_snapshot(&mut self) -> JsValue {
        let (version, mut styles) = self.take_computed_styles();
        let mut output = Vec::new();
        let mut budget = SnapshotBudget::default();
        let mut exhausted = false;
        if !styles.has_keyframe_definitions() {
            self.computed_styles = Some((version, styles));
            return JsValue::Array(output);
        }
        for node in Node::composed_descendants(&self.document).take(100_000) {
            if node.element().is_none() || !styles.may_have_css_animation(&node) {
                continue;
            }
            let Some(style) = styles.computed_style_for_node(&node).cloned() else {
                continue;
            };
            let mut current = Some(node.clone());
            let mut rendered = true;
            while let Some(ancestor) = current {
                if styles
                    .computed_style_for_node(&ancestor)
                    .is_some_and(|style| style.display == crate::engine::css::Display::None)
                {
                    rendered = false;
                    break;
                }
                current = Node::composed_parent(&ancestor);
            }
            if !rendered {
                continue;
            }
            let settings = &style.animation;
            let mut animations = Vec::new();
            for (index, name) in settings.names.iter().enumerate().take(16) {
                let Some(name) = name.named() else {
                    continue;
                };
                let Some(definition) =
                    styles.animation_keyframes_for_scope(settings.name_scope, name)
                else {
                    continue;
                };
                let definition = definition.clone();
                if !budget.reserve(name.len() + 256, 12) {
                    exhausted = true;
                    break;
                }
                let Some(frames) = frames(&definition, &mut styles, &node, &mut budget) else {
                    exhausted = true;
                    break;
                };
                animations.push(JsValue::Array(vec![
                    JsValue::from(name.to_owned()),
                    JsValue::from(
                        f64::from(settings.durations[index % settings.durations.len()]) * 1000.0,
                    ),
                    JsValue::from(
                        f64::from(settings.delays[index % settings.delays.len()]) * 1000.0,
                    ),
                    JsValue::from(settings.easings[index % settings.easings.len()].clone()),
                    JsValue::from(settings.iterations[index % settings.iterations.len()]),
                    JsValue::from(settings.directions[index % settings.directions.len()].clone()),
                    JsValue::from(settings.fills[index % settings.fills.len()].clone()),
                    JsValue::from(settings.states[index % settings.states.len()].clone()),
                    frames,
                ]));
            }
            if !animations.is_empty() {
                output.push(JsValue::Array(vec![
                    JsValue::from(self.id_for(&node)),
                    JsValue::Array(animations),
                ]));
                if output.len() == 64 {
                    break;
                }
            }
            if exhausted {
                break;
            }
        }
        self.computed_styles = Some((version, styles));
        JsValue::Array(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_limits_are_atomic_and_saturating() {
        let mut budget = SnapshotBudget::default();
        assert!(budget.reserve(1_048_575, 19_999));
        assert!(!budget.reserve(usize::MAX, 1));
        assert_eq!((budget.bytes, budget.values), (1_048_575, 19_999));
        assert!(budget.reserve(1, 1));
        assert!(!budget.reserve(0, 1));
        assert!(!budget.reserve(1, 0));
    }

    fn state() -> HostState {
        let dom = crate::engine::dom::parse(
            "<style>@keyframes fade{from{opacity:0}to{opacity:1}} #target{animation:fade 1s linear both paused}</style><main><div id=target></div></main>",
        );
        HostState::new(
            dom.document,
            "https://example.test/",
            "UTF-8",
            Rc::new(module_loader::WebModuleLoader::new()),
        )
    }

    #[test]
    fn snapshot_discovery_does_not_mutate_author_styles_or_its_own_revision() {
        let mut state = state();
        let target = Node::descendants(&state.document)
            .find(|node| node.attr("id").as_deref() == Some("target"))
            .unwrap();
        let version = state.document.document_mutation_version();
        let revision = state.css_animation_revision;
        for _ in 0..5 {
            let JsValue::Array(snapshot) = state.css_animation_snapshot() else {
                panic!("snapshot array");
            };
            assert_eq!(snapshot.len(), 1);
        }
        assert_eq!(state.css_animation_revision, revision);
        assert_eq!(state.document.document_mutation_version(), version);
        assert!(target.attr("style").is_none());
    }

    #[test]
    fn animation_and_transition_overlay_writes_do_not_trigger_rediscovery() {
        let mut state = state();
        let target = Node::descendants(&state.document)
            .find(|node| node.attr("id").as_deref() == Some("target"))
            .unwrap();
        let id = state.id_for(&target);
        let revision = state.css_animation_revision;
        for operation in ["setAnimationStyle", "setTransitionStyle"] {
            super::super::super::style_host::style_host_call(
                operation,
                &[
                    JsValue::from(operation.to_owned()),
                    JsValue::from(id),
                    JsValue::from("opacity:.5".to_owned()),
                ],
                &mut state,
            )
            .unwrap();
            assert_eq!(state.css_animation_revision, revision);
        }
        assert!(target.attr("style").is_none());
        assert!(!state.pending_invalidation.snapshot(0).is_empty());
    }

    #[test]
    fn authored_changes_and_loaded_stylesheets_advance_discovery_revision() {
        let mut state = state();
        let target = Node::descendants(&state.document)
            .find(|node| node.attr("id").as_deref() == Some("target"))
            .unwrap();
        let revision = state.css_animation_revision;
        target.set_attr("class", "changed");
        state.record_mutation(Some(&target), MutationKind::Attribute("class"));
        assert_ne!(state.css_animation_revision, revision);
        let revision = state.css_animation_revision;
        state.replace_document_stylesheets(&[crate::engine::css::StylesheetSource::injected(
            "https://example.test/extra.css",
            "@keyframes fade{to{opacity:.4}}".into(),
        )]);
        assert_ne!(state.css_animation_revision, revision);
        let revision = state.css_animation_revision;
        let sheets = state.stylesheet_sources.clone();
        state.replace_document_stylesheets(&sheets);
        assert_eq!(state.css_animation_revision, revision);
    }
}
