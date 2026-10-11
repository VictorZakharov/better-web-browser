//! Retain natural dimensions, not pixels, across unrelated resource consumers.
//! A style-only edit to an SVG definition can change another SVG's natural size.
//! Stable URL identity alone is not an intrinsic-size proof for retained siblings.
use super::*;

#[derive(Clone, Debug, Default)]
pub(super) struct ImageInputs(Vec<(String, Option<(u32, u32)>)>);
impl ImageInputs {
    pub fn matches(&self, page: &Page) -> bool {
        self.0
            .iter()
            .all(|(key, dimensions)| *dimensions == page.layout_image_dimensions(key))
    }
}

pub(in crate::engine::layout) fn retain(page: &Page, output: &mut LayoutOutput) {
    if output.fixed_layers.layers.is_empty() {
        return;
    }
    let mut inputs = Vec::new();
    let mut seen = HashSet::new();
    let mut bytes = 0usize;
    for item in &output.items {
        let key = match item {
            DisplayItem::Image { url, .. } | DisplayItem::BackgroundImage { url, .. } => url,
            _ => continue,
        };
        if seen.contains(key.as_str()) {
            continue;
        }
        bytes = bytes.saturating_add(key.len());
        if key.len() > 4096 || bytes > 128 * 1024 || inputs.len() >= 512 {
            // Exhaustion declines this optional retained path, never image rendering.
            output.fixed_layers.layers.clear();
            output.fixed_layers.image_inputs = Default::default();
            return;
        }
        seen.insert(key.as_str());
        inputs.push((key.clone(), page.layout_image_dimensions(key)));
    }
    output.fixed_layers.image_inputs = ImageInputs(inputs);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::DecodedImage;
    #[test]
    fn same_key_new_dimensions_cannot_reuse_unrelated_boxes() {
        let mut page = Page::parse("", "https://example.test/");
        let key = "inline-svg:example".to_string();
        let inputs = ImageInputs(vec![(key.clone(), Some((2, 3)))]);
        page.images.insert(
            key.clone(),
            DecodedImage {
                width: 2,
                height: 3,
                bgra: vec![0; 24].into(),
            },
        );
        assert!(inputs.matches(&page));
        page.images.insert(
            key.clone(),
            DecodedImage {
                width: 3,
                height: 2,
                bgra: vec![0; 24].into(),
            },
        );
        assert!(!inputs.matches(&page));
        page.images.remove(&key);
        assert!(!inputs.matches(&page));
    }
}
