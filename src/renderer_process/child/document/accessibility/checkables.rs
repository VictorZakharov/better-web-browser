//! HTML checkable roles survive replacement of control ink by cached raster images.
//! https://www.w3.org/TR/html-aam-1.0/#el-input-checkbox
use crate::engine::Page;
use crate::engine::dom::{Node, NodeRef};
use crate::renderer_protocol::{SemanticChecked, SemanticRole};

pub(super) fn checked_state(node: &NodeRef, role: SemanticRole) -> Option<SemanticChecked> {
    if !matches!(role, SemanticRole::Checkbox | SemanticRole::Radio) {
        return None;
    }
    let native = node.tag_name() == Some("input")
        && node.attr("type").is_some_and(|value| {
            value.eq_ignore_ascii_case("checkbox") || value.eq_ignore_ascii_case("radio")
        });
    let (checked, mixed) = if native {
        (node.checked(), node.indeterminate())
    } else {
        let value = node.attr("aria-checked").unwrap_or_default();
        (
            value.eq_ignore_ascii_case("true"),
            value.eq_ignore_ascii_case("mixed"),
        )
    };
    Some(if mixed && role == SemanticRole::Checkbox {
        SemanticChecked::Mixed
    } else if checked {
        SemanticChecked::True
    } else {
        SemanticChecked::False
    })
}

pub(super) fn name(node: &NodeRef, page: &Page) -> String {
    if let Some(ids) = node.attr("aria-labelledby") {
        let labels = ids
            .split_whitespace()
            .filter_map(|id| {
                Node::descendants(&page.dom.document)
                    .find(|target| target.attr("id").as_deref() == Some(id))
            })
            .map(|target| target.text_content())
            .collect::<Vec<_>>()
            .join(" ");
        if !labels.trim().is_empty() {
            return super::semantics::bounded_text(
                &labels.split_whitespace().collect::<Vec<_>>().join(" "),
            );
        }
    }
    if let Some(label) = node
        .attr("aria-label")
        .filter(|label| !label.trim().is_empty())
    {
        return super::semantics::bounded_text(&label);
    }
    if node.tag_name() != Some("input") {
        return super::semantics::bounded_text(
            &node
                .text_content()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
        );
    }
    let id = node.attr("id");
    let labels = Node::descendants(&page.dom.document)
        .filter(|label| label.tag_name() == Some("label"))
        .filter(|label| {
            if let Some(target) = label.attr("for") {
                id.as_deref() == Some(target.as_str())
            } else {
                Node::descendants(label).any(|child| child.id() == node.id())
            }
        })
        .map(|label| label.text_content())
        .collect::<Vec<_>>()
        .join(" ");
    super::semantics::bounded_text(&labels.split_whitespace().collect::<Vec<_>>().join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn renderer_raster_checkables_still_have_names_roles_and_live_checked_state() {
        let page = Page::parse(
            "<label for=c>First label</label><label><input id=c type=checkbox checked>Second label</label><fieldset disabled><input type=radio checked></fieldset>",
            "https://example.test/",
        );
        let checkbox = page.dom.elements_named("input").next().unwrap();
        assert_eq!(
            super::super::semantics::role_for(&checkbox, None),
            Some(SemanticRole::Checkbox)
        );
        assert_eq!(name(&checkbox, &page), "First label Second label");
        assert_eq!(
            checked_state(&checkbox, SemanticRole::Checkbox),
            Some(SemanticChecked::True)
        );
        checkbox.set_indeterminate(true);
        assert_eq!(
            checked_state(&checkbox, SemanticRole::Checkbox),
            Some(SemanticChecked::Mixed)
        );
        let actions = super::super::semantics::actions_for(SemanticRole::Checkbox, &checkbox, None);
        assert!(actions.focus && actions.invoke && !actions.set_value);
        let radio = page.dom.elements_named("input").nth(1).unwrap();
        assert!(super::super::semantics::is_disabled(&radio));
        assert!(!super::super::semantics::actions_for(SemanticRole::Radio, &radio, None).invoke);
        radio.set_indeterminate(true);
        assert_eq!(
            checked_state(&radio, SemanticRole::Radio),
            Some(SemanticChecked::True)
        );
    }

    #[test]
    fn author_checkables_keep_content_names_and_labelledby_precedes_aria_label() {
        let page = Page::parse(
            "<span id=name>Referenced choice</span><div role=checkbox aria-checked=mixed>Content choice</div><input type=checkbox aria-label=Ignored aria-labelledby=name>",
            "https://example.test/",
        );
        let author = page.dom.elements_named("div").next().unwrap();
        assert_eq!(name(&author, &page), "Content choice");
        assert_eq!(
            checked_state(&author, SemanticRole::Checkbox),
            Some(SemanticChecked::Mixed)
        );
        let input = page.dom.elements_named("input").next().unwrap();
        assert_eq!(name(&input, &page), "Referenced choice");
    }
}
