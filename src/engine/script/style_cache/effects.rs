//! Reuse unrelated CSSOM styles only across a single private effect write.
use super::*;

impl HostState {
    pub(in crate::engine::script) fn inline_style_uses_target_scope(&self) -> bool {
        self.computed_styles
            .as_ref()
            .or(self.offset_parent_styles.as_ref())
            .is_some_and(|(_, styles)| styles.can_scope_inline_style_to_target())
    }

    pub(in crate::engine::script) fn effect_style_changed(
        &mut self,
        target: &NodeRef,
        before: u64,
    ) {
        let after = self.document.document_mutation_version();
        if before.checked_add(1) != Some(after) {
            return;
        }
        for cache in [&mut self.computed_styles, &mut self.offset_parent_styles] {
            if let Some((cached_version, styles)) = cache
                && *cached_version == before
            {
                // Never acknowledge an intervening authored/structural change.
                // Those still trigger the existing full cache/rule invalidation.
                // Native samples do not modify the DOM style attribute or any
                // selector input; inherited values and pseudos need eviction.
                styles.discard_effect_styles(target);
                *cached_version = after;
            }
        }
    }
}

#[cfg(test)]
mod tests;
