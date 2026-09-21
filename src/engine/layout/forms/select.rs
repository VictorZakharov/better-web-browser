use super::*;

pub(in crate::engine::layout) struct SelectData {
    pub options: Vec<SelectOption>,
    pub selected_index: i64,
}

impl SelectData {
    pub fn label(&self) -> String {
        self.selected_option()
            .map(|option| option.label.clone())
            .unwrap_or_default()
    }

    pub fn value(&self) -> String {
        self.selected_option()
            .map(|option| option.value.clone())
            .unwrap_or_default()
    }

    fn selected_option(&self) -> Option<&SelectOption> {
        usize::try_from(self.selected_index)
            .ok()
            .and_then(|index| self.options.get(index))
    }

    pub fn preferred_width(&self, font_size: f32) -> f32 {
        let characters = self
            .options
            .iter()
            .map(|option| option.label.chars().count())
            .max()
            .unwrap_or(0);
        (characters as f32 * font_size * 0.58 + 38.0).max(90.0)
    }
}

pub(in crate::engine::layout) fn select_data(node: &NodeRef) -> SelectData {
    // The select's list of options owns membership; selectedness owns state.
    let options: Vec<_> = node
        .select_options()
        .iter()
        .map(|option| SelectOption {
            value: option.option_value(),
            label: option.option_label(),
        })
        .collect();
    SelectData {
        options,
        selected_index: node.selected_index(),
    }
}
