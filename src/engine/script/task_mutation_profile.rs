//! Bounded per-task mutation accounting used by the script security budget.

use super::MutationKind;
use std::collections::BTreeMap;

const MAX_PROFILED_ATTRIBUTE_NAMES: usize = 32;
const MAX_PROFILED_ATTRIBUTE_NAME_BYTES: usize = 64;

#[derive(Default)]
pub(super) struct TaskMutationProfile {
    attributes: usize,
    unchanged_attributes: usize,
    attribute_names: BTreeMap<String, usize>,
    character_data: usize,
    child_list: usize,
    stylesheets: usize,
}

impl TaskMutationProfile {
    pub(super) fn record(&mut self, kind: MutationKind<'_>) {
        let counter = match kind {
            MutationKind::Attribute(name) => {
                let name = name
                    .chars()
                    .take(MAX_PROFILED_ATTRIBUTE_NAME_BYTES)
                    .collect::<String>()
                    .to_ascii_lowercase();
                if let Some(count) = self.attribute_names.get_mut(&name) {
                    *count = count.saturating_add(1);
                } else if self.attribute_names.len() < MAX_PROFILED_ATTRIBUTE_NAMES {
                    self.attribute_names.insert(name, 1);
                }
                &mut self.attributes
            }
            MutationKind::CharacterData => &mut self.character_data,
            MutationKind::ChildList => &mut self.child_list,
            MutationKind::Stylesheet => &mut self.stylesheets,
            MutationKind::Viewport => return,
        };
        *counter = counter.saturating_add(1);
    }

    pub(super) fn total(&self) -> usize {
        self.attributes
            .saturating_add(self.character_data)
            .saturating_add(self.child_list)
            .saturating_add(self.stylesheets)
    }

    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(super) fn record_unchanged_attribute(&mut self) {
        self.unchanged_attributes = self.unchanged_attributes.saturating_add(1);
    }

    pub(super) fn summary(&self) -> String {
        let mut attributes = self.attribute_names.iter().collect::<Vec<_>>();
        attributes.sort_by_key(|(name, count)| (std::cmp::Reverse(**count), name.as_str()));
        let top_attributes = attributes
            .into_iter()
            .take(8)
            .map(|(name, count)| format!("{name}:{count}"))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "attributes={}, unchanged_attributes={}, top_attributes=[{}], character_data={}, child_list={}, stylesheets={}",
            self.attributes,
            self.unchanged_attributes,
            top_attributes,
            self.character_data,
            self.child_list,
            self.stylesheets
        )
    }
}
