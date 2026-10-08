//! Keyframe values resolve variables, inheritance, shorthands, and relative units natively.
//! This is the same declaration pipeline as normal style, without the target's effect overlay.
use super::*;

impl StyleSet {
    pub(crate) fn resolved_keyframe_values(
        &mut self,
        node: &NodeRef,
        declarations: &[(String, String)],
    ) -> Vec<(String, String)> {
        let underlying = self.underlying_style_for_node(node);
        let parent = Node::composed_parent(node)
            .and_then(|parent| self.computed_style_for_node(&parent).cloned());
        let mut style = underlying.clone();
        let mut properties = Vec::new();
        for (name, value) in declarations {
            if name.len() > 128
                || value.len() > 1024
                || !supports::supports_declaration_value(name, value)
            {
                continue;
            }
            if name.starts_with("animation") && name != "animation-timing-function" {
                continue;
            }
            for declaration in parse_declarations(&format!("{name}:{value}")) {
                apply_resolved_declaration(
                    &mut style,
                    &declaration,
                    DeclarationContext {
                        parent: parent.as_ref(),
                        lower_origin: &underlying,
                        layer_start: &underlying,
                        base_url: &self.document_base_url,
                        viewport_width: self.viewport_width,
                        viewport_height: self.viewport_height,
                    },
                );
            }
            for property in longhands(name) {
                if !properties.contains(&property) && properties.len() < 64 {
                    properties.push(property);
                }
            }
        }
        style.resolve_relative_units(
            self.viewport_width,
            self.viewport_height,
            style.root_font_size,
        );
        style.resolve_line_height(self.viewport_width, self.viewport_height);
        properties
            .into_iter()
            .filter_map(|name| {
                resolved_property_value(&style, name).map(|value| (name.to_owned(), value))
            })
            .collect()
    }
}

fn longhands(name: &str) -> Vec<&str> {
    match name {
        "margin" => vec!["margin-top", "margin-right", "margin-bottom", "margin-left"],
        "padding" => vec![
            "padding-top",
            "padding-right",
            "padding-bottom",
            "padding-left",
        ],
        "border-width" => vec![
            "border-top-width",
            "border-right-width",
            "border-bottom-width",
            "border-left-width",
        ],
        "border-color" => vec![
            "border-top-color",
            "border-right-color",
            "border-bottom-color",
            "border-left-color",
        ],
        "border" => [longhands("border-width"), longhands("border-color")].concat(),
        "border-top" => vec!["border-top-width", "border-top-color"],
        "border-right" => vec!["border-right-width", "border-right-color"],
        "border-bottom" => vec!["border-bottom-width", "border-bottom-color"],
        "border-left" => vec!["border-left-width", "border-left-color"],
        "background" => vec!["background-color"],
        "font" => vec![
            "font-size",
            "font-family",
            "font-style",
            "font-weight",
            "line-height",
        ],
        _ => vec![name],
    }
}

#[cfg(test)]
mod tests;
