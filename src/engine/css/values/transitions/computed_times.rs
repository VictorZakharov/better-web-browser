//! Copy unresolved time expressions in cascade-layer snapshots, but already
//! computed seconds during explicit inheritance.
use super::*;

impl TransitionSettings {
    pub(crate) fn copy_property(&mut self, source: &Self, property: &str) -> bool {
        match property {
            "transition" => self.clone_from(source),
            "transition-property" => self.properties.clone_from(&source.properties),
            "transition-duration" => {
                self.durations.clone_from(&source.durations);
                self.calculated_times
                    .durations
                    .clone_from(&source.calculated_times.durations);
            }
            "transition-delay" => {
                self.delays.clone_from(&source.delays);
                self.calculated_times
                    .delays
                    .clone_from(&source.calculated_times.delays);
            }
            "transition-timing-function" => {
                self.easings.clone_from(&source.easings);
                self.calculated_easings
                    .clone_from(&source.calculated_easings);
            }
            _ => return false,
        }
        true
    }
}
