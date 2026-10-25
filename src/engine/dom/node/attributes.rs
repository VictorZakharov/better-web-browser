//! Borrowed attribute reads for read-only DOM and selector operations.
use super::{ElementData, Node};
use html5ever::Attribute;
use std::cell::{Ref, RefCell, RefMut};

/// An element's `class` tokens, split once for selector matching. Selector-heavy work
/// (style matching, `querySelectorAll`) tests classes far more often than they change, and
/// re-finding and re-splitting the attribute dominated those tests.
#[derive(Debug, Default)]
pub(crate) struct ClassTokens(RefCell<Option<Box<[Box<str>]>>>);

impl ElementData {
    /// Mutable attributes. Every attribute change goes through here so derived caches reset.
    pub(crate) fn attrs_mut(&self) -> RefMut<'_, Vec<Attribute>> {
        self.class_tokens.0.borrow_mut().take();
        self.attrs.borrow_mut()
    }

    fn with_class_tokens<T>(&self, read: impl FnOnce(&[Box<str>]) -> T) -> T {
        if let Some(tokens) = self.class_tokens.0.borrow().as_deref() {
            return read(tokens);
        }
        // Match `attr_ref`: the first attribute whose local name is ASCII-case-insensitively
        // `class`, split on ASCII whitespace.
        let tokens = self
            .attrs
            .borrow()
            .iter()
            .find(|attribute| attribute.name.local.as_ref().eq_ignore_ascii_case("class"))
            .map(|attribute| {
                attribute
                    .value
                    .split_ascii_whitespace()
                    .map(Box::from)
                    .collect::<Box<[_]>>()
            })
            .unwrap_or_default();
        let result = read(&tokens);
        *self.class_tokens.0.borrow_mut() = Some(tokens);
        result
    }
}

impl Node {
    pub(crate) fn attr_ref(&self, wanted: &str) -> Option<Ref<'_, str>> {
        let element = self.element()?;
        Ref::filter_map(element.attrs.borrow(), |attributes| {
            attributes
                .iter()
                .find(|attribute| attribute.name.local.as_ref().eq_ignore_ascii_case(wanted))
                .map(|attribute| attribute.value.as_ref())
        })
        .ok()
    }

    pub fn attr(&self, wanted: &str) -> Option<String> {
        self.attr_ref(wanted).map(|value| value.to_string())
    }

    pub fn has_class(&self, wanted: &str) -> bool {
        self.element().is_some_and(|element| {
            element.with_class_tokens(|tokens| tokens.iter().any(|class| **class == *wanted))
        })
    }

    /// Calls `visit` with each `class` token, in attribute order.
    pub(crate) fn for_each_class(&self, mut visit: impl FnMut(&str)) {
        if let Some(element) = self.element() {
            element.with_class_tokens(|tokens| tokens.iter().for_each(|class| visit(class)));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::dom;

    #[test]
    fn class_tokens_follow_every_attribute_change() {
        let dom = dom::parse("<p class='a  b'></p>");
        let node = dom.elements_named("p").next().unwrap();
        assert!(node.has_class("a") && node.has_class("b") && !node.has_class("c"));
        node.set_attr("class", "c");
        assert!(node.has_class("c") && !node.has_class("a"));
        node.remove_attr("class");
        assert!(!node.has_class("c"));
        node.set_attr("CLASS", "d");
        let mut seen = Vec::new();
        node.for_each_class(|class| seen.push(class.to_string()));
        assert_eq!(seen, ["d"]);
    }
}
