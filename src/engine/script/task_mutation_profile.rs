//! Per-task tree mutation accounting used by the script security budget.

use super::MutationKind;

#[derive(Default)]
pub(super) struct TaskMutationProfile {
    child_list: usize,
    stylesheets: usize,
}

impl TaskMutationProfile {
    pub(super) fn record(&mut self, kind: MutationKind<'_>) {
        let counter = match kind {
            MutationKind::ChildList => &mut self.child_list,
            MutationKind::Stylesheet => &mut self.stylesheets,
            MutationKind::Attribute(_) | MutationKind::CharacterData | MutationKind::Viewport => {
                return;
            }
        };
        *counter = counter.saturating_add(1);
    }

    pub(super) fn tree_total(&self) -> usize {
        self.child_list.saturating_add(self.stylesheets)
    }

    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(super) fn summary(&self) -> String {
        format!(
            "child_list={}, stylesheets={}",
            self.child_list, self.stylesheets
        )
    }
}
