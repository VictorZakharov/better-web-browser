//! Immutable compiled-rule sharing between a document's independent style consumers.
use super::*;
use crate::engine::css::media::MediaEnvironment;
use crate::engine::css::rule_index::RuleIndex;
use std::rc::Rc;

mod parsed;
#[cfg(test)]
mod rebuild_tests;
mod registry;

#[derive(Debug, PartialEq, Eq, Hash)]
struct SheetInput {
    source: String,
    base_url: String,
    scope: RuleScope,
}

#[derive(Debug, Default)]
pub(super) struct CompiledRules {
    pub(super) rules: Vec<Rule>,
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
        external_stylesheets: &[(String, String)],
        environment: MediaEnvironment,
        removed_nodes: &[NodeId],
    ) -> StyleRefreshStats {
        let viewport_changed = self.viewport_width != environment.viewport_width
            || self.viewport_height != environment.viewport_height;
        // Full rule changes also invalidate entries which are no longer in the composed tree.
        // Keeping them would make later explicit queries reuse stale unassigned light-DOM styles.
        // A full refresh need not carry an incremental removal log, so prune detached entries too.
        let composed = Node::composed_descendants(&dom.document)
            .map(|node| node.id())
            .collect::<HashSet<_>>();
        let previous_count = self.styles.len();
        let previous_generated_count = self.generated_nodes.len();
        self.styles.retain(|node, _| composed.contains(node));
        self.pseudo_styles
            .retain(|(origin, _), _| composed.contains(origin));
        self.generated_nodes
            .retain(|(origin, _), _| composed.contains(origin));
        let generated = self
            .generated_nodes
            .values()
            .flat_map(Node::descendants)
            .map(|node| node.id())
            .collect::<HashSet<_>>();
        self.generated_styles
            .retain(|node, _| generated.contains(node));
        let removed_styles = previous_count - self.styles.len();
        let removed_generated = previous_generated_count != self.generated_nodes.len();
        self.compiled = collect(&dom.document, base_url, external_stylesheets, environment);
        self.document_base_url = base_url.to_string();
        self.viewport_width = environment.viewport_width;
        self.viewport_height = environment.viewport_height;
        let mut stats = self.refresh_subtrees(
            &dom.document,
            std::slice::from_ref(&dom.document),
            removed_nodes,
        );
        stats.removed_styles += removed_styles;
        // Auto/percentage used sizes can change with the containing viewport even when every
        // computed style remains equal. A stylesheet equality check cannot suppress that layout.
        stats.layout_changed |= viewport_changed || removed_styles != 0 || removed_generated;
        stats.full_rebuild = true;
        stats
    }
}

pub(super) fn collect(
    document: &NodeRef,
    document_base_url: &str,
    external_stylesheets: &[(String, String)],
    environment: MediaEnvironment,
) -> Rc<CompiledRules> {
    let mut inputs = Vec::new();
    append_inline(
        document,
        document_base_url,
        &mut inputs,
        RuleScope::Document,
    );
    inputs.extend(
        external_stylesheets
            .iter()
            .map(|(base_url, source)| SheetInput {
                source: source.clone(),
                base_url: base_url.clone(),
                scope: RuleScope::Document,
            }),
    );
    append_adopted(document, environment, &mut inputs, RuleScope::Document);
    for shadow in Node::shadow_including_descendants(document)
        .filter(|node| matches!(node.data, NodeData::ShadowRoot(_)))
    {
        append_inline(
            &shadow,
            document_base_url,
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
    if let Some(cached) = candidates
        .iter()
        .find(|cached| cached.environment == Some(environment) && cached.inputs == inputs)
    {
        registry::remember(document.id(), cached);
        return Rc::clone(cached);
    }
    let previous = candidates
        .iter()
        .find(|set| set.environment == Some(environment));
    let (rules, parsed) = parsed::assemble(&mut inputs, environment, previous.map(Rc::as_ref));
    let compiled = Rc::new(CompiledRules {
        index: RuleIndex::new(&rules),
        rules,
        inputs,
        parsed,
        environment: Some(environment),
    });
    registry::remember(document.id(), &compiled);
    compiled
}

fn append_inline(root: &NodeRef, base_url: &str, inputs: &mut Vec<SheetInput>, scope: RuleScope) {
    inputs.extend(
        Node::descendants(root)
            .filter(|node| node.tag_name() == Some("style"))
            .map(|node| SheetInput {
                source: node.text_content(),
                base_url: base_url.to_string(),
                scope,
            }),
    );
}

fn append_adopted(
    root: &NodeRef,
    environment: MediaEnvironment,
    inputs: &mut Vec<SheetInput>,
    scope: RuleScope,
) {
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
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shares_only_identical_rule_inputs_and_never_computed_styles() {
        let dom = dom::parse("<style>.a {color:red}</style><p class=a>x</p>");
        let first = StyleSet::from_dom(&dom, &[], 800.0);
        let mut second = StyleSet::from_dom(&dom, &[], 800.0);
        assert!(Rc::ptr_eq(&first.compiled, &second.compiled));
        second.clear_computed_styles();
        assert!(!first.styles.is_empty());
        assert!(second.styles.is_empty());
        assert!(!Rc::ptr_eq(
            &first.compiled,
            &StyleSet::from_dom(&dom, &[], 900.0).compiled
        ));
        let other = dom::parse("<style>.a {color:red}</style><p class=a>x</p>");
        assert!(!Rc::ptr_eq(
            &first.compiled,
            &StyleSet::from_dom(&other, &[], 800.0).compiled
        ));
        Node::set_text_content(
            &dom.elements_named("style").next().unwrap(),
            ".a {color:blue}",
        );
        let updated = StyleSet::from_dom(&dom, &[], 800.0);
        assert!(!Rc::ptr_eq(&first.compiled, &updated.compiled));
        assert_eq!(
            updated.get(&dom.elements_named("p").next().unwrap()).color,
            Color::rgb(0, 0, 255)
        );
    }

    #[test]
    fn rebuilding_rules_reconciles_values_and_generated_boxes_against_a_fresh_cascade() {
        let dom = dom::parse(
            "<style>:root{--tone:red} p{color:var(--tone);width:10vw} p::before{content:'old'}</style><p>x</p>",
        );
        let target = dom.elements_named("p").next().unwrap();
        let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
        let variables = styles.get(&target).custom_properties.clone();
        let before = styles
            .generated_pseudo(&target, PseudoElement::Before)
            .unwrap();
        let environment = MediaEnvironment::new(800.0, 800.0, 1.0, false);
        let unchanged = styles.rebuild_rules_for_media_environment(&dom, "", &[], environment, &[]);
        assert!(!unchanged.layout_changed);
        assert!(std::sync::Arc::ptr_eq(
            &variables,
            &styles.get(&target).custom_properties
        ));
        assert_eq!(
            styles
                .generated_pseudo(&target, PseudoElement::Before)
                .unwrap()
                .id(),
            before.id()
        );
        Node::set_text_content(
            &dom.elements_named("style").next().unwrap(),
            ":root{--tone:blue} p{color:var(--tone);width:20vw} p::after{content:'new'}",
        );
        let changed = styles.rebuild_rules_for_media_environment(&dom, "", &[], environment, &[]);
        assert!(changed.layout_changed);
        let fresh = StyleSet::from_dom(&dom, &[], 800.0);
        assert_eq!(styles.styles, fresh.styles);
        assert!(
            styles
                .generated_pseudo(&target, PseudoElement::Before)
                .is_none()
        );
        assert_eq!(
            styles
                .generated_pseudo(&target, PseudoElement::After)
                .unwrap()
                .text_content(),
            "new"
        );
    }

    #[test]
    fn shadow_scopes_adopted_sources_and_media_invalidate_sharing() {
        let dom = dom::parse("<x-one></x-one><x-two></x-two>");
        let roots = ["x-one", "x-two"].map(|tag| {
            Node::attach_shadow(
                &dom.elements_named(tag).next().unwrap(),
                crate::engine::dom::ShadowRootMode::Open,
                false,
                false,
                false,
            )
            .unwrap()
        });
        let sheet = |source: &str, media: &str| crate::engine::AdoptedStyleSheet {
            source: source.into(),
            media: media.into(),
            base_url: "https://example.test/component.css".into(),
        };
        roots[0].set_adopted_stylesheets(vec![sheet(":host{color:red}", "")]);
        let first = StyleSet::from_dom(&dom, &[], 800.0);
        assert!(Rc::ptr_eq(
            &first.compiled,
            &StyleSet::from_dom(&dom, &[], 800.0).compiled
        ));
        roots[0].set_adopted_stylesheets(vec![]);
        roots[1].set_adopted_stylesheets(vec![sheet(":host{color:red}", "")]);
        let moved = StyleSet::from_dom(&dom, &[], 800.0);
        assert!(!Rc::ptr_eq(&first.compiled, &moved.compiled));
        assert_eq!(
            moved
                .get(&dom.elements_named("x-two").next().unwrap())
                .color,
            Color::rgb(255, 0, 0)
        );
        roots[1].set_adopted_stylesheets(vec![sheet(":host{color:blue}", "")]);
        let edited = StyleSet::from_dom(&dom, &[], 800.0);
        assert!(!Rc::ptr_eq(&moved.compiled, &edited.compiled));
        assert_eq!(
            edited
                .get(&dom.elements_named("x-two").next().unwrap())
                .color,
            Color::rgb(0, 0, 255)
        );
        roots[1].set_adopted_stylesheets(vec![sheet(":host{color:blue}", "(min-width:900px)")]);
        let hidden = StyleSet::from_dom(&dom, &[], 800.0);
        assert!(!Rc::ptr_eq(&edited.compiled, &hidden.compiled));
        assert!(hidden.compiled.rules.is_empty());
        let visible = StyleSet::from_dom(&dom, &[], 1000.0);
        assert!(!visible.compiled.rules.is_empty());
        let weak = Rc::downgrade(&visible.compiled);
        drop(visible);
        assert!(
            weak.upgrade().is_none(),
            "the global lookup must not retain compiled sources"
        );
    }

    #[test]
    fn source_urls_and_source_order_are_part_of_compiled_rule_identity() {
        let dom = dom::parse("<p>x</p>");
        let environment = MediaEnvironment::new(800.0, 600.0, 1.0, false);
        let first = collect(
            &dom.document,
            "",
            &[
                ("https://example.com/a.css".into(), "p{color:red}".into()),
                ("https://example.com/b.css".into(), "p{color:blue}".into()),
            ],
            environment,
        );
        let swapped = collect(
            &dom.document,
            "",
            &[
                ("https://example.com/b.css".into(), "p{color:blue}".into()),
                ("https://example.com/a.css".into(), "p{color:red}".into()),
            ],
            environment,
        );
        assert!(!Rc::ptr_eq(&first, &swapped));
        let base_changed = collect(
            &dom.document,
            "",
            &[
                ("https://other.example/a.css".into(), "p{color:red}".into()),
                ("https://example.com/b.css".into(), "p{color:blue}".into()),
            ],
            environment,
        );
        assert!(!Rc::ptr_eq(&first, &base_changed));
    }
}
