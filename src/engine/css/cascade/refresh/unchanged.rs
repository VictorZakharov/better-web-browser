//! Whether a restyled node's style equals its previous one, compared once per pair of styles.
//
// Elements that shared a style before a restyle usually share one after it too, so many nodes
// compare the same pair of allocations. Comparing two equal ~3 KB styles walks every field;
// remembering each pair's result makes that once per pair. Both styles are kept alive while
// remembered, so neither address can be reused by another style during the traversal.
use super::*;

/// Both compared styles, kept alive, and whether they are equal.
type Comparison = (Arc<ComputedStyle>, Arc<ComputedStyle>, bool);

#[derive(Default)]
pub(super) struct UnchangedStyles {
    results: NodeMap<(usize, usize), Comparison>,
}

impl UnchangedStyles {
    pub(super) fn equal(
        &mut self,
        previous: &Arc<ComputedStyle>,
        style: &Arc<ComputedStyle>,
    ) -> bool {
        if Arc::ptr_eq(previous, style) {
            return true;
        }
        let key = (Arc::as_ptr(previous) as usize, Arc::as_ptr(style) as usize);
        self.results
            .entry(key)
            .or_insert_with(|| (Arc::clone(previous), Arc::clone(style), previous == style))
            .2
    }
}
