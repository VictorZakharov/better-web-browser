//! Download only best-style font subsets intersecting visible text. Font file
//! admission remains separate from CSS membership: an unused rule is unloaded.
use super::*;
use crate::engine::css::{ComputedStyle, Display, PseudoElement, TextTransform};
use crate::engine::dom::{Node, NodeData};
use crate::engine::font::{WebFontFace, unicode_ranges::UnicodeRanges};
use std::collections::{BTreeSet, HashSet};

struct Request {
    family: String,
    weight: u16,
    italic: bool,
    codepoints: BTreeSet<u32>,
}

impl Page {
    pub(super) fn request_visible_fonts(&mut self, styles: &mut StyleSet) {
        let available = styles.document_font_faces();
        if available.is_empty() {
            return;
        }
        let mut requests = Vec::new();
        let mut hidden = HashSet::new();
        for node in Node::composed_descendants(&self.dom.document) {
            let Some(style) = styles.computed_style_for_node(&node).cloned() else {
                continue;
            };
            if style.display == Display::None
                || Node::composed_parent(&node).is_some_and(|parent| hidden.contains(&parent.id()))
            {
                hidden.insert(node.id());
                continue;
            }
            let text = match &node.data {
                NodeData::Text(_) | NodeData::Cdata(_) => {
                    // Live textarea values are collected from their control owner.
                    if node
                        .parent()
                        .is_some_and(|parent| parent.tag_name() == Some("textarea"))
                    {
                        String::new()
                    } else {
                        node.text_content()
                    }
                }
                NodeData::Element(_) if node.tag_name() == Some("input") => {
                    node.input_display_value()
                }
                NodeData::Element(_) if node.tag_name() == Some("textarea") => {
                    node.textarea_api_value()
                }
                _ => String::new(),
            };
            collect(&mut requests, &available, &style, &text);
            for pseudo in [PseudoElement::Before, PseudoElement::After] {
                if let Some(generated) = styles.generated_pseudo(&node, pseudo) {
                    collect(
                        &mut requests,
                        &available,
                        styles.get(&generated),
                        &generated.text_content(),
                    );
                }
            }
        }
        let mut selected = Vec::new();
        for request in requests {
            let rank = |face: &WebFontFace| {
                let weight = face.weight_match_rank(request.weight);
                (u8::from(face.italic != request.italic), weight.0, weight.1)
            };
            let family_faces = available
                .iter()
                .filter(|face| face.family.eq_ignore_ascii_case(&request.family));
            let Some(best) = family_faces
                .clone()
                .map(rank)
                .min_by(|a, b| a.partial_cmp(b).unwrap())
            else {
                continue;
            };
            for face in family_faces.filter(|face| rank(face) == best) {
                let Some(ranges) = UnicodeRanges::parse(&face.unicode_range) else {
                    continue;
                };
                if !request
                    .codepoints
                    .iter()
                    .any(|point| ranges.contains(*point))
                {
                    continue;
                }
                let weight = face.registered_weight(request.weight);
                if !selected
                    .iter()
                    .any(|(old, old_weight)| old == face && *old_weight == weight)
                {
                    selected.push((face.clone(), weight));
                }
            }
        }
        for (face, weight) in selected.into_iter().take(MAX_WEB_FONTS) {
            let resource = PageResource::Font {
                source_url: face.url.clone(),
                fallback_urls: face.fallback_urls,
                url: face.url,
                family: face.family,
                weight,
                italic: face.italic,
                unicode_range: face.unicode_range,
                font_feature_settings: face.features.css_text(),
            };
            if !self.resources.contains(&resource) {
                self.resources.push(resource);
            }
        }
    }
}

fn collect(
    requests: &mut Vec<Request>,
    available: &[WebFontFace],
    style: &ComputedStyle,
    text: &str,
) {
    if !style.visibility || style.display == Display::None || text.is_empty() {
        return;
    }
    for family in crate::engine::css::font_family::parse(&style.font_family).unwrap_or_default() {
        let crate::engine::css::font_family::Family::Named(family) = family else {
            continue;
        };
        if !available
            .iter()
            .any(|face| face.family.eq_ignore_ascii_case(&family))
        {
            continue;
        }
        let index = requests
            .iter()
            .position(|request| {
                request.family.eq_ignore_ascii_case(&family)
                    && request.weight == style.font_weight
                    && request.italic == style.italic
            })
            .unwrap_or_else(|| {
                requests.push(Request {
                    family,
                    weight: style.font_weight,
                    italic: style.italic,
                    codepoints: BTreeSet::new(),
                });
                requests.len() - 1
            });
        let codepoints = &mut requests[index].codepoints;
        for character in text.chars() {
            match style.text_transform {
                TextTransform::Uppercase => {
                    codepoints.extend(character.to_uppercase().map(u32::from))
                }
                TextTransform::Lowercase => {
                    codepoints.extend(character.to_lowercase().map(u32::from))
                }
                TextTransform::Capitalize => {
                    // Word starts can span adjacent inline nodes. Conservatively
                    // admit both cases until the loader shares formatting-run boundaries.
                    codepoints.insert(character as u32);
                    codepoints.extend(character.to_uppercase().map(u32::from));
                }
                TextTransform::None => {
                    codepoints.insert(character as u32);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
