//! UA subtitle boxes are painted in the video's retained display-list slot, after its frame
//! but before following page content. This keeps captions clipped and avoids DOM injection.

use super::*;
use crate::engine::TextMeasurer;
use crate::engine::css::Color;
use crate::engine::layout::{DisplayItem, RectF};
use crate::engine::script::ScriptCaptionCue;

impl DocumentRuntime {
    pub(in crate::renderer_process::child::document) fn compose_media_captions(&mut self) {
        if self.media_captions.is_empty() {
            return;
        }
        let mut insertions = Vec::new();
        for (node, cues) in &self.media_captions {
            if cues.is_empty()
                || self
                    .page
                    .dom
                    .find_node(*node)
                    .is_none_or(|node| node.tag_name() != Some("video"))
            {
                continue;
            }
            let image_key = format!("breeze-internal:media-frame:{:032x}", node.to_wire());
            if let Some((index, rect)) =
                self.layout
                    .items
                    .iter()
                    .enumerate()
                    .find_map(|(index, item)| match item {
                        DisplayItem::Image { rect, url, .. } if *url == image_key => {
                            Some((index, *rect))
                        }
                        _ => None,
                    })
            {
                insertions.push((index, rect, cues.clone()));
            }
        }
        // Insert in reverse order to leave earlier display-list offsets stable.
        insertions.sort_by_key(|(index, _, _)| *index);
        for (index, rect, cues) in insertions.into_iter().rev() {
            let items = caption_items(&mut self.text.borrow_mut(), rect, &cues);
            self.layout.items.splice(index + 1..index + 1, items);
        }
    }
}

fn caption_items(
    text_system: &mut super::super::RendererTextSystem,
    rect: RectF,
    cues: &[ScriptCaptionCue],
) -> Vec<DisplayItem> {
    if rect.width < 80.0 || rect.height < 40.0 {
        return Vec::new();
    }
    let font = crate::engine::FontSpec {
        family: "Arial".into(),
        size: (rect.height * 0.052).clamp(14.0, 32.0),
        weight: 600,
        italic: false,
        underline: false,
        letter_spacing: 0.0,
        word_spacing: 0.0,
    };
    let line_height = font.size * 1.25;
    let mut items = Vec::new();
    items.push(DisplayItem::BeginClip { bounds: rect });
    let mut stack_height = (rect.height * 0.04).max(4.0);
    for cue in cues.iter().take(4) {
        let max_width =
            (rect.width * cue.size_percent as f32 / 100.0).clamp(24.0, rect.width - 24.0);
        let mut lines = wrap_caption(text_system, &cue.text, &font, max_width - 16.0);
        lines.truncate((((rect.height - 12.0) / line_height).floor() as usize).max(1));
        if lines.is_empty() {
            continue;
        }
        let width = (lines
            .iter()
            .map(|line| text_system.measure(line, &font).0)
            .fold(0.0f32, f32::max)
            + 16.0)
            .min(max_width);
        let height = line_height * lines.len() as f32 + 12.0;
        let anchor = rect.x + rect.width * cue.position_percent as f32 / 100.0;
        let position_align = if cue.position_align == "auto" {
            match cue.align.as_str() {
                "start" | "left" => "line-left",
                "end" | "right" => "line-right",
                _ => "center",
            }
        } else {
            cue.position_align.as_str()
        };
        let x = match position_align {
            "line-left" => anchor,
            "line-right" => anchor - width,
            _ => anchor - width / 2.0,
        }
        .clamp(rect.x, rect.right() - width);
        let y = if let Some(line) = cue.line {
            let step = line_height + 4.0;
            if line >= 0 {
                rect.y + line as f32 * step
            } else {
                rect.bottom() + (line as f32 + 1.0) * step - height
            }
        } else if let Some(line) = cue.line_percent {
            rect.y + (rect.height - height) * line as f32 / 100.0
        } else {
            rect.bottom() - height - stack_height
        }
        .clamp(rect.y, rect.bottom() - height);
        let box_rect = RectF {
            x,
            y,
            width,
            height,
        };
        items.push(DisplayItem::SolidRect {
            rect: box_rect,
            color: Color {
                red: 0,
                green: 0,
                blue: 0,
                alpha: 190,
            },
            radius: 4.0,
        });
        for (number, line) in lines.into_iter().enumerate() {
            let shaped = text_system.shape(&line, &font);
            let text_x = match cue.align.as_str() {
                "start" | "left" => x + 8.0,
                "end" | "right" => x + width - shaped.width - 8.0,
                _ => x + (width - shaped.width) / 2.0,
            };
            items.push(DisplayItem::Text {
                rect: RectF {
                    x: text_x,
                    y: y + 6.0 + line_height * number as f32,
                    width: shaped.width,
                    height: line_height,
                },
                text: line,
                font: font.clone(),
                color: Color::WHITE,
                link: None,
                node_id: None,
                raster_run_id: shaped.raster_run_id,
                glyphs: shaped.glyphs,
            });
        }
        if cue.line.is_none() && cue.line_percent.is_none() {
            stack_height += height + 4.0;
        }
    }
    items.push(DisplayItem::EndClip { bounds: rect });
    items
}

fn wrap_caption(
    text_system: &mut super::super::RendererTextSystem,
    input: &str,
    font: &crate::engine::FontSpec,
    max_width: f32,
) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in input.lines() {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let proposed = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if text_system.measure(&proposed, font).0 <= max_width {
                line = proposed;
            } else {
                if !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                }
                for character in word.chars() {
                    let mut proposed = line.clone();
                    proposed.push(character);
                    if !line.is_empty() && text_system.measure(&proposed, font).0 > max_width {
                        lines.push(std::mem::take(&mut line));
                    }
                    line.push(character);
                }
            }
            if lines.len() >= 3 {
                break;
            }
        }
        if !line.is_empty() {
            lines.push(line);
        }
        if lines.len() >= 4 {
            break;
        }
    }
    lines.truncate(4);
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cue(text: &str) -> ScriptCaptionCue {
        ScriptCaptionCue {
            text: text.into(),
            line: None,
            line_percent: None,
            position_percent: 50,
            size_percent: 100,
            align: "center".into(),
            position_align: "auto".into(),
        }
    }

    #[test]
    fn caption_is_shaped_clipped_and_centered_over_video() {
        let mut text = super::super::super::RendererTextSystem::new(96);
        let video = RectF {
            x: 20.0,
            y: 40.0,
            width: 320.0,
            height: 180.0,
        };
        let items = caption_items(&mut text, video, &[cue("An accessible subtitle")]);
        assert!(
            matches!(items.first(), Some(DisplayItem::BeginClip { bounds }) if *bounds == video)
        );
        assert!(matches!(items.last(), Some(DisplayItem::EndClip { bounds }) if *bounds == video));
        let box_rect = items
            .iter()
            .find_map(|item| match item {
                DisplayItem::SolidRect { rect, color, .. } if color.alpha > 0 => Some(*rect),
                _ => None,
            })
            .expect("caption backdrop");
        assert!(box_rect.x >= video.x && box_rect.right() <= video.right());
        assert!(box_rect.y > video.y && box_rect.bottom() <= video.bottom());
        assert!(items.iter().any(|item| matches!(item,
            DisplayItem::Text { text, glyphs, .. } if text == "An accessible subtitle" && !glyphs.is_empty())));
    }

    #[test]
    fn explicit_cue_position_and_multiple_cues_stay_inside_video() {
        let mut text = super::super::super::RendererTextSystem::new(96);
        let video = RectF {
            x: 20.0,
            y: 40.0,
            width: 500.0,
            height: 300.0,
        };
        let mut first = cue("left aligned");
        first.line_percent = Some(25);
        first.position_percent = 10;
        first.position_align = "line-left".into();
        first.align = "start".into();
        first.size_percent = 40;
        let mut second = cue("bottom aligned");
        second.line = Some(-1);
        let items = caption_items(&mut text, video, &[first, second]);
        let boxes: Vec<_> = items
            .iter()
            .filter_map(|item| match item {
                DisplayItem::SolidRect { rect, .. } => Some(*rect),
                _ => None,
            })
            .collect();
        assert_eq!(boxes.len(), 2);
        assert!(boxes[0].x < boxes[1].x && boxes[0].y < boxes[1].y);
        assert!(boxes.iter().all(|rect| rect.x >= video.x
            && rect.right() <= video.right()
            && rect.y >= video.y
            && rect.bottom() <= video.bottom()));
    }
}
