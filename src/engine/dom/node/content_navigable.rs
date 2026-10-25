//! Which navigable containers render their content navigable's document, and at what size.
use super::Node;

/// The natural size of an embedded document that does not define one (HTML §15.4.1).
pub(crate) const DEFAULT_NAVIGABLE_SIZE: (f32, f32) = (300.0, 150.0);

impl Node {
    /// An `iframe` always renders its content navigable. An `object` does so only once a
    /// document response was committed; otherwise (while loading, after an HTTP error
    /// status or for an unsupported type) it represents its fallback content.
    /// https://html.spec.whatwg.org/multipage/iframe-embed-object.html#the-object-element
    pub(crate) fn represents_content_navigable(&self) -> bool {
        self.content_navigable_size().is_some()
    }

    /// The natural size of the content navigable this element renders, if any.
    pub(crate) fn content_navigable_size(&self) -> Option<(f32, f32)> {
        match self.tag_name() {
            Some("iframe") => Some(DEFAULT_NAVIGABLE_SIZE),
            Some("object") => self.element()?.object_navigable.get(),
            _ => None,
        }
    }

    /// Records whether an `object` represents its content navigable; returns whether it changed.
    pub(crate) fn set_object_represents_navigable(&self, value: bool) -> bool {
        self.set_object_navigable_size(value.then_some(DEFAULT_NAVIGABLE_SIZE))
    }

    /// Records the represented navigable's natural size (`None`: the fallback content).
    /// Returns whether it changed.
    pub(crate) fn set_object_navigable_size(&self, size: Option<(f32, f32)>) -> bool {
        self.element()
            .filter(|_| self.tag_name() == Some("object"))
            .is_some_and(|element| element.object_navigable.replace(size) != size)
    }
}
