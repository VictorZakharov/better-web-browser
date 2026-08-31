//! Namespace-aware attribute access and mutation.

use super::super::node::Node;
use html5ever::tendril::StrTendril;
use html5ever::{Attribute, LocalName, Namespace, Prefix, QualName, ns};

impl Node {
    pub fn attributes(&self) -> Vec<Attribute> {
        self.element()
            .map(|element| element.attrs.borrow().clone())
            .unwrap_or_default()
    }

    pub fn attr_qualified(&self, qualified_name: &str) -> Option<String> {
        self.attribute_qualified(qualified_name)
            .map(|attribute| attribute.value.to_string())
    }

    pub fn attr_ns(&self, namespace: Option<&str>, local_name: &str) -> Option<String> {
        self.attribute_ns(namespace, local_name)
            .map(|attribute| attribute.value.to_string())
    }

    pub fn attribute_qualified(&self, qualified_name: &str) -> Option<Attribute> {
        self.element().and_then(|element| {
            element
                .attrs
                .borrow()
                .iter()
                .find(|attribute| attribute_qualified_name(attribute) == qualified_name)
                .cloned()
        })
    }

    pub fn attribute_ns(&self, namespace: Option<&str>, local_name: &str) -> Option<Attribute> {
        self.element().and_then(|element| {
            element
                .attrs
                .borrow()
                .iter()
                .find(|attribute| attribute_matches(attribute, namespace, local_name))
                .cloned()
        })
    }

    pub fn set_attr_qualified(&self, qualified_name: &str, value: &str) -> bool {
        let Some(element) = self.element() else {
            return false;
        };
        let mut attributes = element.attrs.borrow_mut();
        if let Some(attribute) = attributes
            .iter_mut()
            .find(|attribute| attribute_qualified_name(attribute) == qualified_name)
        {
            attribute.value = StrTendril::from(value);
        } else {
            attributes.push(Attribute {
                name: QualName::new(None, ns!(), LocalName::from(qualified_name)),
                value: StrTendril::from(value),
            });
        }
        drop(attributes);
        self.mark_mutated();
        true
    }

    pub fn set_attr_ns(
        &self,
        namespace: Option<&str>,
        prefix: Option<&str>,
        local_name: &str,
        value: &str,
    ) -> bool {
        let Some(element) = self.element() else {
            return false;
        };
        let mut attributes = element.attrs.borrow_mut();
        if let Some(attribute) = attributes
            .iter_mut()
            .find(|attribute| attribute_matches(attribute, namespace, local_name))
        {
            attribute.value = StrTendril::from(value);
        } else {
            attributes.push(new_attribute(namespace, prefix, local_name, value));
        }
        drop(attributes);
        self.mark_mutated();
        true
    }

    pub fn replace_attr_ns(
        &self,
        namespace: Option<&str>,
        prefix: Option<&str>,
        local_name: &str,
        value: &str,
    ) -> bool {
        let Some(element) = self.element() else {
            return false;
        };
        let mut attributes = element.attrs.borrow_mut();
        let replacement = new_attribute(namespace, prefix, local_name, value);
        if let Some(index) = attributes
            .iter()
            .position(|attribute| attribute_matches(attribute, namespace, local_name))
        {
            attributes[index] = replacement;
        } else {
            attributes.push(replacement);
        }
        drop(attributes);
        self.mark_mutated();
        true
    }

    pub fn remove_attr_qualified(&self, qualified_name: &str) -> bool {
        self.remove_matching_attr(|attribute| attribute_qualified_name(attribute) == qualified_name)
    }

    pub fn remove_attr_ns(&self, namespace: Option<&str>, local_name: &str) -> bool {
        self.remove_matching_attr(|attribute| attribute_matches(attribute, namespace, local_name))
    }

    fn remove_matching_attr(&self, matches: impl Fn(&Attribute) -> bool) -> bool {
        let Some(element) = self.element() else {
            return false;
        };
        let mut attributes = element.attrs.borrow_mut();
        let Some(index) = attributes.iter().position(matches) else {
            return false;
        };
        attributes.remove(index);
        drop(attributes);
        self.mark_mutated();
        true
    }
}

fn attribute_qualified_name(attribute: &Attribute) -> String {
    attribute.name.prefix.as_ref().map_or_else(
        || attribute.name.local.to_string(),
        |prefix| format!("{prefix}:{}", attribute.name.local),
    )
}

fn attribute_matches(attribute: &Attribute, namespace: Option<&str>, local_name: &str) -> bool {
    attribute.name.ns.as_ref() == namespace.unwrap_or_default()
        && attribute.name.local.as_ref() == local_name
}

fn new_attribute(
    namespace: Option<&str>,
    prefix: Option<&str>,
    local_name: &str,
    value: &str,
) -> Attribute {
    Attribute {
        name: QualName::new(
            prefix.map(Prefix::from),
            Namespace::from(namespace.unwrap_or_default()),
            LocalName::from(local_name),
        ),
        value: StrTendril::from(value),
    }
}
