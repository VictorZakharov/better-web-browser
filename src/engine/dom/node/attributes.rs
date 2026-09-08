//! Borrowed attribute reads for read-only DOM and selector operations.
use super::Node;
use std::cell::Ref;

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
        self.attr_ref("class").is_some_and(|classes| {
            classes
                .split_ascii_whitespace()
                .any(|class| class == wanted)
        })
    }
}
