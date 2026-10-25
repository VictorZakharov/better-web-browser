//! Conservative ancestor-key rejection, rebuilt whenever the owning DOM version changes.
use super::*;

mod keys;
use keys::Keys;

#[derive(Clone, Copy, Default)]
pub(in crate::engine::css) struct AncestorFilter {
    tags: Keys,
    ids: Keys,
    classes: Keys,
    attributes: Keys,
}

#[derive(Debug, Default)]
pub(in crate::engine::css) struct AncestorFilterCache {
    version: Option<(NodeId, u64)>,
    filters: HashMap<NodeId, AncestorFilter>,
}

impl std::fmt::Debug for AncestorFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AncestorFilter")
    }
}

impl AncestorFilterCache {
    pub(in crate::engine::css) fn for_node(&mut self, node: &NodeRef) -> AncestorFilter {
        // Allocation identity survives adoption. The current shadow-including root owns the
        // mutation epoch; detached and adopted subtrees must not reuse their old ancestor keys.
        let root = Node::shadow_including_root(node);
        let version = (root.id(), root.subtree_mutation_version());
        if self.version != Some(version)
            || (self.filters.len() >= crate::limits::MAX_DOM_NODES
                && !self.filters.contains_key(&node.id()))
        {
            self.filters.clear();
            self.version = Some(version);
        }
        if let Some(filter) = self.filters.get(&node.id()) {
            return *filter;
        }
        let mut chain = Vec::new();
        let mut cursor = Some(node.clone());
        let mut filter = AncestorFilter::default();
        while let Some(current) = cursor {
            if let Some(cached) = self.filters.get(&current.id()) {
                filter = *cached;
                filter.insert(&current);
                break;
            }
            cursor = current.parent();
            chain.push(current);
        }
        if self.filters.len() + chain.len() > crate::limits::MAX_DOM_NODES {
            self.filters.clear();
        }
        for current in chain.into_iter().rev() {
            self.filters.insert(current.id(), filter);
            filter.insert(&current);
        }
        self.filters.get(&node.id()).copied().unwrap_or_default()
    }
}

impl AncestorFilter {
    pub(in crate::engine::css) fn needed(selector: &Selector) -> bool {
        selector.ancestor_keys.is_some()
    }

    /// The keys every ancestor-side compound of a descendant/child-only selector requires,
    /// hashed once at parse time so matching tests whole bit masks. `None` when the filter
    /// cannot reject: a single compound, a sibling combinator, or no positive ancestor key.
    pub(in crate::engine::css) fn required_keys(
        compounds: &[CompoundSelector],
        combinators: &[Combinator],
    ) -> Option<std::rc::Rc<Self>> {
        // Sibling chains can leave the ancestor path. Alternatives and negations supply no
        // necessary key. Ignore those conditions and leave them to the complete matcher.
        if compounds.len() < 2
            || !combinators
                .iter()
                .all(|combinator| matches!(combinator, Combinator::Child | Combinator::Descendant))
        {
            return None;
        }
        let mut keys = Self::default();
        for compound in &compounds[..compounds.len() - 1] {
            if let Some(tag) = &compound.tag {
                keys.tags.insert(tag, false);
            }
            if let Some(id) = &compound.id {
                keys.ids.insert(id, false);
            }
            for class in &compound.classes {
                keys.classes.insert(class, false);
            }
            for attribute in &compound.attributes {
                keys.attributes.insert(&attribute.name, true);
            }
            if compound.requires_link {
                keys.tags.insert("a", false);
            }
        }
        let empty = keys.tags.is_empty()
            && keys.ids.is_empty()
            && keys.classes.is_empty()
            && keys.attributes.is_empty();
        (!empty).then(|| std::rc::Rc::new(keys))
    }

    #[cfg(test)]
    fn new(node: &NodeRef) -> Self {
        let mut result = Self::default();
        let mut parent = node.parent();
        while let Some(node) = parent {
            parent = node.parent();
            result.insert(&node);
        }
        result
    }

    fn insert(&mut self, node: &NodeRef) {
        let Some(element) = node.element() else {
            return;
        };
        self.tags.insert(element.name.local.as_ref(), false);
        if let Some(id) = node.attr_ref("id") {
            self.ids.insert(&id, false);
        }
        node.for_each_class(|class| self.classes.insert(class, false));
        for attribute in element.attrs.borrow().iter() {
            self.attributes.insert(attribute.name.local.as_ref(), true);
        }
    }

    pub(in crate::engine::css) fn may_match(&self, selector: &Selector) -> bool {
        selector.ancestor_keys.as_deref().is_none_or(|required| {
            self.tags.contains_all(&required.tags)
                && self.ids.contains_all(&required.ids)
                && self.classes.contains_all(&required.classes)
                && self.attributes.contains_all(&required.attributes)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod cache;

    #[test]
    fn rejects_absent_ancestors_without_losing_full_matcher_results() {
        let dom = dom::parse(
            "<main id=root class='a' data-scope><section class=b><i class=before></i><p class=target>x</p></section></main>",
        );
        let node = dom.elements_named("p").next().unwrap();
        for input in [
            ".a .target",
            "#root > .b > p",
            "[data-scope] p",
            ".absent p",
            "aside p",
            "[missing] p",
            ".before + p",
            ".before ~ p",
            ":is(main,aside) p",
            ":not(aside) p",
            "p",
            ".b .a p",
        ] {
            let selector = parse_selector(input).unwrap();
            let filter = AncestorFilter::new(&node);
            assert_eq!(
                filter.may_match(&selector) && selector_matches(&selector, &node),
                selector_matches(&selector, &node),
                "{input}"
            );
        }
        let missing = parse_selector(".absent p").unwrap();
        assert!(!AncestorFilter::new(&node).may_match(&missing));
        dom.elements_named("main")
            .next()
            .unwrap()
            .set_attr("class", "absent");
        assert!(AncestorFilter::new(&node).may_match(&missing));
        assert!(selector_matches(&missing, &node));
    }
}
