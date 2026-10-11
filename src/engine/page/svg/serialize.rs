//! Bounded decoder input, including referenced definitions from sibling SVGs.
use super::references::{IndexCache, References};
use crate::engine::css::StyleSet;
use crate::engine::dom::{NodeData, NodeRef};
use crate::limits::{MAX_DOM_NODES, MAX_SVG_SOURCE_BYTES};

#[cfg(test)]
pub(super) fn source(node: &NodeRef, styles: Option<&StyleSet>) -> Result<String, String> {
    source_with_index(node, styles, &mut IndexCache::default())
}

pub(super) fn source_with_index(
    node: &NodeRef,
    styles: Option<&StyleSet>,
    cache: &mut IndexCache,
) -> Result<String, String> {
    let references = References::collect_with_index(node, cache)?;
    let mut writer = Writer {
        output: String::new(),
        nodes: 0,
        references: &references,
    };
    writer.node(node, true, styles, 0)?;
    Ok(writer.output)
}

struct Writer<'a> {
    output: String,
    nodes: usize,
    references: &'a References,
}

impl Writer<'_> {
    fn push(&mut self, value: &str) -> Result<(), String> {
        if value.len() > MAX_SVG_SOURCE_BYTES.saturating_sub(self.output.len()) {
            return Err("inline SVG exceeds source byte budget".into());
        }
        self.output.push_str(value);
        Ok(())
    }

    fn escape(&mut self, value: &str) -> Result<(), String> {
        for character in value.chars() {
            let mut bytes = [0; 4];
            self.push(match character {
                '&' => "&amp;",
                '<' => "&lt;",
                '>' => "&gt;",
                '"' => "&quot;",
                '\'' => "&apos;",
                character => character.encode_utf8(&mut bytes),
            })?;
        }
        Ok(())
    }

    fn node(
        &mut self,
        node: &NodeRef,
        root: bool,
        styles: Option<&StyleSet>,
        depth: usize,
    ) -> Result<(), String> {
        self.nodes += 1;
        if depth > 256 || self.nodes > MAX_DOM_NODES {
            return Err("inline SVG exceeds structural budget".into());
        }
        match &node.data {
            NodeData::Element(element) => {
                let tag = element.name.local.as_ref();
                if tag.eq_ignore_ascii_case("script")
                    || self.references.blocked_uses.contains(&node.id())
                {
                    return Ok(());
                }
                self.push("<")?;
                self.push(tag)?;
                let attrs = element.attrs.borrow();
                let color = styles
                    .and_then(|styles| styles.styles.get(&node.id()))
                    .map(|style| style.color);
                if root {
                    self.push(" xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\"")?;
                }
                for attribute in attrs.iter() {
                    if attribute.value.len() > MAX_SVG_SOURCE_BYTES {
                        return Err("inline SVG attribute exceeds source byte budget".into());
                    }
                    let local = attribute.name.local.as_ref();
                    if (color.is_some() && local == "style")
                        || (root
                            && (local == "xmlns"
                                || attribute
                                    .name
                                    .prefix
                                    .as_ref()
                                    .is_some_and(|prefix| prefix.as_ref() == "xmlns")))
                        || (local == "id"
                            && !self.references.retain_id(&attribute.value, node.id()))
                    {
                        continue;
                    }
                    self.push(" ")?;
                    if let Some(prefix) = &attribute.name.prefix {
                        self.push(prefix.as_ref())?;
                        self.push(":")?;
                    }
                    self.push(local)?;
                    self.push("=\"")?;
                    if local == "href" {
                        self.escape(&super::urls::href(&attribute.value))?;
                    } else if super::urls::is_paint_attribute(local) {
                        self.escape(&super::urls::paint(&attribute.value)?)?;
                    } else {
                        self.escape(&attribute.value)?;
                    }
                    self.push("\"")?;
                }
                if let Some(color) = color {
                    self.push(" style=\"")?;
                    if let Some(style) = node.attr("style") {
                        // The outer CSS box paints these effects exactly once.
                        // Keep its SVG raster local and reusable while transforms,
                        // opacity and inset wipes change from frame to frame.
                        let filtered;
                        let style = if root {
                            let mut excluded = vec![
                                "opacity",
                                "transform",
                                "translate",
                                "rotate",
                                "scale",
                                "transform-origin",
                                "transform-box",
                            ];
                            // URL clips/masks still belong to the SVG decoder;
                            // native CSS clipping currently owns inset shapes.
                            if styles
                                .and_then(|styles| styles.styles.get(&node.id()))
                                .is_some_and(|style| !style.clip_path.is_none())
                            {
                                excluded.push("clip-path");
                            }
                            filtered = crate::engine::css::cssom::declarations::without_properties(
                                &style, &excluded,
                            );
                            filtered.as_str()
                        } else {
                            style.as_str()
                        };
                        let style = style.trim_end_matches([';', ' ', '\t', '\r', '\n', '\u{c}']);
                        if !style.is_empty() {
                            self.escape(&super::urls::paint(style)?)?;
                            self.push(";")?;
                        }
                    }
                    self.push(&format!(
                        "color:#{:02x}{:02x}{:02x}{:02x}!important",
                        color.red, color.green, color.blue, color.alpha
                    ))?;
                    if root {
                        // Root effects, including presentation attributes and
                        // SVG-local rules, are composed once by the CSS painter.
                        // Preserve attributes for descendant selectors and use
                        // a matrix accepted by both CSS and the SVG decoder.
                        self.push(";opacity:1!important;transform:matrix(1,0,0,1,0,0)!important")?;
                    }
                    // The document cascade can match ancestors and rules outside
                    // this SVG. Preserve instance inheritance in definitions:
                    // the isolated backend resolves their use/marker context.
                    if !std::iter::successors(Some(node.clone()), |node| node.parent())
                        .any(|node| matches!(node.tag_name(), Some("defs" | "symbol" | "marker")))
                        && let Some(style) = styles.and_then(|styles| styles.styles.get(&node.id()))
                    {
                        for (name, paint) in
                            [("fill", &style.svg_fill), ("stroke", &style.svg_stroke)]
                        {
                            self.push(";")?;
                            self.push(name)?;
                            self.push(":")?;
                            self.escape(&super::urls::paint(&paint.css_text(style.color))?)?;
                            self.push("!important")?;
                        }
                    }
                    if (root || matches!(tag, "text" | "tspan" | "textPath"))
                        && !std::iter::successors(node.parent(), |parent| parent.parent())
                            .any(|parent| matches!(parent.tag_name(), Some("defs" | "symbol")))
                        && let Some(style) = styles.and_then(|styles| styles.styles.get(&node.id()))
                    {
                        self.push(";font-family:")?;
                        self.escape(&style.font_family)?;
                        self.push(&format!("!important;font-size:{}px!important;font-weight:{}!important;font-style:{}!important;letter-spacing:{}px!important;word-spacing:{}px!important", style.font_size, style.font_weight, if style.italic { "italic" } else { "normal" }, style.letter_spacing, style.word_spacing))?;
                    }
                    self.push("\"")?;
                }
                self.push(">")?;
                drop(attrs);
                if root && !self.references.targets.is_empty() {
                    self.push("<defs>")?;
                    // Do not freeze inherited currentColor from the original definition's
                    // parent: use instances inherit from their referencing element.
                    for target in &self.references.targets {
                        self.node(target, false, None, depth + 1)?;
                    }
                    self.push("</defs>")?;
                }
                for child in node.children.borrow().iter() {
                    self.node(child, false, styles, depth + 1)?;
                }
                self.push("</")?;
                self.push(tag)?;
                self.push(">")?;
            }
            NodeData::Text(text) | NodeData::Cdata(text) => self.escape(&text.borrow())?,
            _ => {}
        }
        Ok(())
    }
}
