//! Per-declaration reuse of `var()` resolution for an unchanged custom-property map.
//
// Substitution and its computed-value-time validation depend only on the declaration and the
// element's custom-property map. Siblings normally share their inherited map, so remembering the
// last outcome per declaration avoids re-tokenizing and re-validating identical substitutions
// during style recalculation. A different map, even with equal contents, simply recomputes.
// https://drafts.csswg.org/css-variables-1/#substitute-a-var
use super::*;
use std::cell::{OnceCell, RefCell};
use std::rc::Rc;
use std::sync::Weak;

type CustomProperties = HashMap<String, String>;

#[derive(Clone, Debug, Default)]
pub(in crate::engine::css) struct SubstitutionMemo {
    has_variable_reference: OnceCell<bool>,
    last: RefCell<Option<Remembered>>,
}

#[derive(Clone, Debug)]
struct Remembered {
    // A weak handle keeps the map's allocation reserved, so its address cannot identify a
    // different map while this entry exists.
    properties: Weak<CustomProperties>,
    outcome: Substitution,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Substitution {
    /// A reference had no value and no fallback.
    Unresolved,
    /// The substituted value does not match the property's grammar.
    Invalid,
    Valid(Rc<str>),
}

impl Declaration {
    pub(super) fn has_variable_reference(&self) -> bool {
        *self
            .substitution
            .has_variable_reference
            .get_or_init(|| self.may_use_var && contains_valid_variable_reference(&self.value))
    }

    pub(super) fn substitute(&self, properties: &Arc<CustomProperties>) -> Substitution {
        if let Some(remembered) = self.substitution.last.borrow().as_ref()
            && std::ptr::eq(remembered.properties.as_ptr(), Arc::as_ptr(properties))
        {
            return remembered.outcome.clone();
        }
        let outcome = match substitute_variables(&self.value, properties) {
            None => Substitution::Unresolved,
            // Substitution happens after the cascade chose this declaration. An invalid result
            // cannot resurrect a lower-priority authored value. Validate declarations, not
            // feature-query admission: partially implemented visual properties may deliberately
            // decline CSS.supports without being invalid CSS.
            // https://drafts.csswg.org/css-variables-1/#invalid-at-computed-value-time
            Some(value)
                if self.has_variable_reference()
                    && !supports::cssom_declaration_value(&self.name, &value) =>
            {
                Substitution::Invalid
            }
            Some(value) => Substitution::Valid(value.into()),
        };
        *self.substitution.last.borrow_mut() = Some(Remembered {
            properties: Arc::downgrade(properties),
            outcome: outcome.clone(),
        });
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declaration(value: &str) -> Declaration {
        parse_declarations(&format!("width:{value}")).remove(0)
    }

    fn properties(width: &str) -> Arc<CustomProperties> {
        Arc::new(HashMap::from([("--w".into(), width.into())]))
    }

    #[test]
    fn remembered_outcomes_follow_map_identity_not_stale_contents() {
        let declaration = declaration("var(--w)");
        let wide = properties("20px");
        assert_eq!(
            declaration.substitute(&wide),
            Substitution::Valid("20px".into())
        );
        assert_eq!(
            declaration.substitute(&wide),
            Substitution::Valid("20px".into())
        );
        let invalid = properties("red");
        assert_eq!(declaration.substitute(&invalid), Substitution::Invalid);
        let mut shared = Arc::clone(&wide);
        Arc::make_mut(&mut shared).insert("--w".into(), "30px".into());
        assert_eq!(
            declaration.substitute(&shared),
            Substitution::Valid("30px".into())
        );
        assert_eq!(
            declaration.substitute(&Arc::new(HashMap::new())),
            Substitution::Unresolved
        );
        assert_eq!(
            declaration.substitute(&wide),
            Substitution::Valid("20px".into())
        );
    }

    #[test]
    fn a_dropped_map_cannot_alias_a_new_map_at_the_same_address() {
        let declaration = declaration("var(--w)");
        for width in ["1px", "2px", "3px", "4px"] {
            let map = properties(width);
            assert_eq!(
                declaration.substitute(&map),
                Substitution::Valid(width.into())
            );
        }
    }

    #[test]
    fn escaped_values_without_references_are_never_validated_as_substitutions() {
        let declaration = declaration("\\31 0px");
        assert!(!declaration.has_variable_reference());
        assert!(matches!(
            declaration.substitute(&Arc::new(HashMap::new())),
            Substitution::Valid(_)
        ));
    }
}
