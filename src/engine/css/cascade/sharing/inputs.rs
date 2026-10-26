//! Which elements may share styles, and when two elements' local inputs are equal.
use super::*;

impl StyleSet {
    /// Whether `node`'s style depends only on inputs the cache compares.
    pub(super) fn sharing_eligible(&self, node: &NodeRef) -> bool {
        let Some(element) = node.element() else {
            return false;
        };
        let tag = element.name.local.as_ref();
        if element.name.ns.as_ref() != "http://www.w3.org/1999/xhtml"
            // Custom elements match :defined and custom states.
            || tag.contains('-')
            || ineligible_tag(tag)
            || element.shadow_root.borrow().is_some()
            || element.animation_style.borrow().is_some()
            || element.transition_style.borrow().is_some()
            || element.fullscreen.get()
            || element.popover_order.get() != 0
        {
            return false;
        }
        {
            let attributes = element.attrs.borrow();
            if attributes.iter().any(|attribute| {
                let name = attribute.name.local.as_ref();
                // Inline declarations and popover/dir=auto UA rules are per element. An id
                // matters only when some selector names it.
                matches!(name, "style" | "popover")
                    || (name == "id"
                        && self
                            .compiled
                            .sharing
                            .referenced_ids
                            .get_or_init(|| referenced_ids(&self.compiled.rules))
                            .contains(&attribute.value.to_ascii_lowercase()))
                    || (name == "dir" && attribute.value.trim().eq_ignore_ascii_case("auto"))
            }) {
                return false;
            }
        }
        let Some(parent) = node.parent() else {
            return false;
        };
        // The root inherits nothing; children of `details` depend on their summary position.
        if !matches!(parent.data, NodeData::Element(_)) || parent.tag_name() == Some("details") {
            return false;
        }
        !matches!(Node::tree_root(node).data, NodeData::ShadowRoot(_))
            && Node::assigned_slot(node).is_none()
            && !(self.visited_link_colors
                && visited::is_visited_link(node, &self.document_base_url))
    }
}

/// Form controls carry state outside attributes; the others have per-element UA rendering.
fn ineligible_tag(tag: &str) -> bool {
    matches!(
        tag,
        "input"
            | "select"
            | "textarea"
            | "button"
            | "option"
            | "optgroup"
            | "fieldset"
            | "legend"
            | "label"
            | "output"
            | "progress"
            | "meter"
            | "details"
            | "summary"
            | "dialog"
            | "slot"
            | "template"
            | "bdi"
            | "html"
            | "body"
    )
}

/// Attributes no style code reads except through selectors, which revalidation compares.
/// Presentational hints, `lang`, `dir`, `hidden` and other attributes the cascade reads are
/// compared by value. Generated `attr()` text is resolved per element, never shared.
pub(super) fn selector_only_attribute(name: &str) -> bool {
    matches!(
        name,
        // Eligible elements' ids are named by no selector.
        "id" | "href"
            | "title"
            | "src"
            | "srcset"
            | "sizes"
            | "alt"
            | "target"
            | "loading"
            | "decoding"
            | "fetchpriority"
            | "referrerpolicy"
            | "crossorigin"
            | "integrity"
            | "nonce"
            | "download"
            | "ping"
            | "hreflang"
            | "rel"
            | "rev"
            | "name"
            | "role"
            | "tabindex"
            | "accesskey"
            | "headers"
            | "abbr"
            | "cite"
            | "datetime"
            | "usemap"
            | "ismap"
            | "draggable"
            | "spellcheck"
            | "autocomplete"
            | "translate"
            // RDFa and microdata annotate content without rendering it.
            | "about"
            | "typeof"
            | "property"
            | "resource"
            | "prefix"
            | "vocab"
            | "itemprop"
            | "itemscope"
            | "itemtype"
            | "itemid"
            | "itemref"
    ) || name.starts_with("data-")
        || name.starts_with("aria-")
}

pub(super) fn same_local_inputs(candidate: &NodeRef, node: &NodeRef) -> bool {
    let (Some(candidate), Some(element)) = (candidate.element(), node.element()) else {
        return false;
    };
    if candidate.name != element.name {
        return false;
    }
    // Attribute order never affects style, and an element has each name at most once, so
    // equal lengths and a match for every attribute make the lists equal as sets.
    let (left, right) = (candidate.attrs.borrow(), element.attrs.borrow());
    left.len() == right.len()
        && left.iter().all(|left| {
            right.iter().any(|right| {
                left.name == right.name
                    && (left.value == right.value
                        || (left.name.ns.is_empty() && selector_only_attribute(&left.name.local)))
            })
        })
}
