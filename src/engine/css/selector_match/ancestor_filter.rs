//! Per-matching-pass necessary ancestor keys; no cache survives a DOM mutation.
use super::*;

#[derive(Default)]
pub(in crate::engine::css) struct AncestorFilter {
    tags: HashSet<String>,
    ids: HashSet<String>,
    classes: HashSet<String>,
    attributes: HashSet<String>,
}

impl AncestorFilter {
    pub(in crate::engine::css) fn needed(selector: &Selector) -> bool {
        selector.compounds.len() > 1
            && selector
                .combinators
                .iter()
                .all(|combinator| matches!(combinator, Combinator::Child | Combinator::Descendant))
    }

    pub(in crate::engine::css) fn new(node: &NodeRef) -> Self {
        let mut result = Self::default();
        let mut parent = node.parent();
        while let Some(node) = parent {
            parent = node.parent();
            let Some(element) = node.element() else {
                continue;
            };
            result.tags.insert(element.name.local.to_string());
            if let Some(id) = node.attr_ref("id") {
                result.ids.insert(id.to_string());
            }
            if let Some(classes) = node.attr_ref("class") {
                result
                    .classes
                    .extend(classes.split_ascii_whitespace().map(str::to_string));
            }
            result.attributes.extend(
                element
                    .attrs
                    .borrow()
                    .iter()
                    .map(|attribute| attribute.name.local.as_ref().to_ascii_lowercase()),
            );
        }
        result
    }

    pub(in crate::engine::css) fn may_match(&self, selector: &Selector) -> bool {
        // Sibling chains can leave the ancestor path. Alternatives and negations supply no
        // necessary key. Ignore those conditions and leave them to the complete matcher.
        if selector.combinators.iter().any(|combinator| {
            matches!(
                combinator,
                Combinator::AdjacentSibling | Combinator::GeneralSibling
            )
        }) {
            return true;
        }
        selector.compounds.iter().rev().skip(1).all(|compound| {
            compound
                .tag
                .as_ref()
                .is_none_or(|tag| self.tags.contains(tag))
                && compound.id.as_ref().is_none_or(|id| self.ids.contains(id))
                && compound
                    .classes
                    .iter()
                    .all(|class| self.classes.contains(class))
                && compound
                    .attributes
                    .iter()
                    .all(|attribute| self.attributes.contains(&attribute.name))
                && (!compound.requires_link || self.tags.contains("a"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
