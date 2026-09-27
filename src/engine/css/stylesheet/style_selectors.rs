//! Selector-list parsing for ordinary, nested, scoped, and shadow-tree style rules.

use super::part_selectors::parse_part_selector;
use super::*;
use crate::engine::css::selector_parser::parse_style_rule_selector_with_parent;

#[derive(Clone)]
pub(super) struct StyleSelector {
    pub(super) selector: Selector,
    pub(super) pseudo: Option<PseudoElement>,
    pub(super) host_condition: Option<Selector>,
    pub(super) host_context: bool,
    pub(super) slotted_origin: Option<Selector>,
    pub(super) slotted_host_child: bool,
    pub(super) part: Option<PartSelector>,
    pub(super) scope: RuleScope,
}

pub(super) fn parse_style_selectors(
    prelude: &str,
    parent: Option<&[StyleSelector]>,
    scope: RuleScope,
    is_css_scoped: bool,
) -> Option<Vec<StyleSelector>> {
    let parent_selectors = parent.map(|parents| {
        parents
            .iter()
            .filter(|parent| parent.pseudo.is_none() && parent.part.is_none())
            .map(|parent| parent.selector.clone())
            .collect::<Vec<_>>()
    });
    let parent_specificity = parent.and_then(|parents| {
        parents
            .iter()
            .map(|parent| parent.selector.specificity)
            .max()
    });
    let mut selectors = Vec::new();
    for member in split_css_top_level(prelude, ',') {
        let member = member.trim();
        if member.is_empty() {
            return None;
        }
        if let Some(parents) = parent {
            // Nested host/slotted rules need a composed-tree selector model.
            // Do not let unsupported nested rules escape their shadow root.
            if parents.iter().any(|parent| {
                parent.part.is_some()
                    || matches!(
                        parent.scope,
                        RuleScope::Host(_) | RuleScope::HostChild(_) | RuleScope::Slotted(_)
                    )
            }) {
                return None;
            }
            let (selector, pseudo) = parse_style_rule_selector_with_parent(
                member,
                parent_selectors.as_deref(),
                parent_specificity,
            )?;
            selectors.push(StyleSelector {
                selector,
                pseudo,
                host_condition: None,
                host_context: false,
                slotted_origin: None,
                slotted_host_child: false,
                part: None,
                scope,
            });
        } else {
            if let Some(part) = parse_part_selector(member)? {
                // @scope matching currently uses the target element's tree. A part
                // selector's originating host needs separate scope-proximity logic.
                if is_css_scoped {
                    return None;
                }
                let mut selector = parse_selector("[part]")?;
                selector.specificity = part.origin.specificity;
                selector.specificity.tags = selector.specificity.tags.saturating_add(1);
                selectors.push(StyleSelector {
                    selector,
                    pseudo: None,
                    host_condition: None,
                    host_context: false,
                    slotted_origin: None,
                    slotted_host_child: false,
                    part: Some(part),
                    scope,
                });
                continue;
            }
            let scoped = scoped_selector(member, scope)?;
            let scoped_source = is_css_scoped.then(|| scope::scoped_selector_source(scoped.source));
            let source = scoped_source.as_deref().unwrap_or(scoped.source);
            let host_condition = scoped
                .host_condition
                .map(parse_selector)
                .transpose_option()?;
            let slotted_origin = scoped
                .slotted_origin
                .map(parse_selector)
                .transpose_option()?;
            if host_condition
                .as_ref()
                .is_some_and(|condition| condition.compounds.len() != 1)
            {
                return None;
            }
            let (mut selector, pseudo) = parse_style_rule_selector(source)?;
            if matches!(scoped.scope, RuleScope::Slotted(_))
                && (pseudo.is_some() || selector.compounds.len() != 1)
            {
                // CSS Shadow §3.2.4 requires a compound selector argument.
                return None;
            }
            if matches!(scoped.scope, RuleScope::Host(_)) && selector.compounds.len() != 1 {
                return None;
            }
            // CSS Shadow §3.2.3: :host and :host() each contribute the
            // specificity of a pseudo-class, in addition to :host()'s argument.
            if matches!(scoped.scope, RuleScope::Host(_)) || host_condition.is_some() {
                selector.specificity.classes = selector.specificity.classes.saturating_add(1);
            }
            if let Some(condition) = host_condition.as_ref() {
                selector.specificity.ids = selector
                    .specificity
                    .ids
                    .saturating_add(condition.specificity.ids);
                selector.specificity.classes = selector
                    .specificity
                    .classes
                    .saturating_add(condition.specificity.classes);
                selector.specificity.tags = selector
                    .specificity
                    .tags
                    .saturating_add(condition.specificity.tags);
            }
            if let Some(origin) = slotted_origin.as_ref() {
                // ::slotted() has pseudo-element specificity plus its compound
                // argument; the originating slot selector also contributes.
                selector.specificity.ids = selector
                    .specificity
                    .ids
                    .saturating_add(origin.specificity.ids);
                selector.specificity.classes = selector
                    .specificity
                    .classes
                    .saturating_add(origin.specificity.classes);
                selector.specificity.tags = selector
                    .specificity
                    .tags
                    .saturating_add(origin.specificity.tags)
                    .saturating_add(1);
            }
            selectors.push(StyleSelector {
                selector,
                pseudo,
                host_condition,
                host_context: scoped.host_context,
                slotted_origin,
                slotted_host_child: scoped.slotted_host_child,
                part: None,
                scope: scoped.scope,
            });
        }
    }
    (!selectors.is_empty()).then_some(selectors)
}

trait TransposeOption<T> {
    fn transpose_option(self) -> Option<Option<T>>;
}

impl<T> TransposeOption<T> for Option<Option<T>> {
    fn transpose_option(self) -> Option<Option<T>> {
        match self {
            Some(Some(value)) => Some(Some(value)),
            Some(None) => None,
            None => Some(None),
        }
    }
}
