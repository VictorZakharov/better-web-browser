//! Geometric image visibility after retained clips; later occlusion is not inferred.
use crate::engine::{DisplayItem, RectF};

pub(super) fn rects(items: &[DisplayItem], url: &str) -> Vec<RectF> {
    let mut clips = Vec::new();
    let mut result = Vec::new();
    for item in items {
        match item {
            DisplayItem::BeginClip { bounds } => clips.push(*bounds),
            DisplayItem::EndClip { .. } => {
                clips.pop();
            }
            DisplayItem::Image { rect, url: key, .. } if key == url => {
                let clipped = clips
                    .iter()
                    .fold(*rect, |rect, clip| intersect(rect, *clip));
                if clipped.width > 0.0 && clipped.height > 0.0 {
                    result.push(clipped);
                    if result.len() == 8 {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    result
}

fn intersect(a: RectF, b: RectF) -> RectF {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    RectF {
        x,
        y,
        width: (a.right().min(b.right()) - x).max(0.0),
        height: (a.bottom().min(b.bottom()) - y).max(0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_clips_hide_images_and_restore_the_outer_region() {
        let outer = RectF {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        };
        let empty = RectF {
            height: 0.0,
            ..outer
        };
        let image = DisplayItem::Image {
            rect: outer,
            url: "frame".into(),
            alt: String::new(),
            tint: None,
        };
        let items = vec![
            DisplayItem::BeginClip { bounds: outer },
            DisplayItem::BeginClip { bounds: empty },
            image.clone(),
            DisplayItem::EndClip { bounds: empty },
            image,
            DisplayItem::EndClip { bounds: outer },
        ];
        assert_eq!(rects(&items, "frame"), vec![outer]);
    }
}
