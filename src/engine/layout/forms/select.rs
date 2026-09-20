use super::*;

pub(in crate::engine::layout) struct SelectData {
    pub options: Vec<SelectOption>,
    pub selected_index: usize,
}

impl SelectData {
    pub fn label(&self) -> String {
        self.options
            .get(self.selected_index)
            .map(|option| option.label.clone())
            .unwrap_or_default()
    }

    pub fn value(&self) -> String {
        self.options
            .get(self.selected_index)
            .map(|option| option.value.clone())
            .unwrap_or_default()
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
    let options: Vec<_> = Node::descendants(node)
        .skip(1)
        .filter(|option| option.tag_name() == Some("option"))
        .map(|option| {
            let text = option
                .text_content()
                .split(['\t', '\n', '\u{c}', '\r', ' '])
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            let label = option
                .attr("label")
                .filter(|label| !label.is_empty())
                .unwrap_or_else(|| text.clone());
            let value = option.attr("value").unwrap_or(text);
            (
                SelectOption { value, label },
                option.attr("selected").is_some(),
            )
        })
        .collect();
    let selected_index = options
        .iter()
        .position(|(_, selected)| *selected)
        .unwrap_or(0);
    SelectData {
        options: options.into_iter().map(|(option, _)| option).collect(),
        selected_index,
    }
}
