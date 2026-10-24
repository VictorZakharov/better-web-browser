//! Whether an element's cascade can read its `revert` origin.
//!
//! `revert` rolls a property back to the style before author origins (user-agent defaults and
//! inheritance), so the cascade keeps a full copy of that style for each element. Copying a
//! computed style costs about as much as inheriting one, and almost no element needs it: on a
//! large Wikipedia article this copy was about a fifth of full-document style time. An element
//! needs it only if one of its declarations may resolve to `revert` or `revert-layer`: directly,
//! escaped, through a `var()` fallback, or through an inherited custom property. SVG elements
//! always keep it, because their presentation attributes are not declarations.
use super::*;

/// Stands in for the revert origin of elements whose cascade cannot read it.
#[derive(Clone, Debug)]
pub(super) struct Placeholder(std::rc::Rc<ComputedStyle>);

impl Default for Placeholder {
    fn default() -> Self {
        Self(std::rc::Rc::new(ComputedStyle::initial()))
    }
}

impl std::ops::Deref for Placeholder {
    type Target = ComputedStyle;

    fn deref(&self) -> &ComputedStyle {
        &self.0
    }
}

impl StyleSet {
    pub(super) fn may_read_revert_origin(
        &self,
        node: &NodeRef,
        inherited: &ComputedStyle,
        matching: &[&Rule],
        declaration_lists: [&[Declaration]; 3],
    ) -> bool {
        if node.namespace_uri() == Some("http://www.w3.org/2000/svg") {
            return true;
        }
        let mut uses_var = false;
        let declarations = matching
            .iter()
            .flat_map(|rule| rule.declarations.iter())
            .chain(declaration_lists.into_iter().flatten());
        for declaration in declarations {
            if declaration.possible_revert {
                return true;
            }
            uses_var |= declaration.may_use_var;
        }
        uses_var && self.inherited_variables_may_revert(&inherited.custom_properties)
    }

    fn inherited_variables_may_revert(
        &self,
        custom_properties: &Arc<HashMap<String, String>>,
    ) -> bool {
        if let Some((cached, result)) = self.revert_variables.borrow().as_ref()
            && Arc::ptr_eq(cached, custom_properties)
        {
            return *result;
        }
        let result = custom_properties.values().any(|value| {
            value.contains('\\')
                || value
                    .as_bytes()
                    .windows(b"revert".len())
                    .any(|part| part.eq_ignore_ascii_case(b"revert"))
        });
        *self.revert_variables.borrow_mut() = Some((Arc::clone(custom_properties), result));
        result
    }
}
