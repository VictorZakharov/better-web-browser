//! CSS Masking's rectangular inset() clip applies to an element and descendants,
//! without changing layout geometry. Paint reordering keeps the clip balanced.
//! https://www.w3.org/TR/css-masking-1/#clipping-paths

use super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn wrap_clip_path(
        &mut self,
        node: &NodeRef,
        style: &ComputedStyle,
        border_box: RectF,
        start: usize,
    ) {
        if !self.emit_paint {
            return;
        }
        let Some(inset) =
            style
                .clip_path
                .inset(border_box.width, border_box.height, style.font_size)
        else {
            return;
        };
        let bounds = border_box.inset(inset);
        if !node.is_generated_pseudo() {
            self.output.clip_paths.insert(node.id(), bounds);
        }
        self.output
            .items
            .insert(start, DisplayItem::BeginClip { bounds });
        self.output.items.push(DisplayItem::EndClip { bounds });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::layout::test_support::FixedMeasurer;

    #[test]
    fn inset_clip_covers_own_paint_and_descendants_but_not_boxes() {
        let page = Page::parse(
            "<style>body{margin:0}main{width:100px;height:100px;background:red;clip-path:inset(10%)}div{width:50px;height:50px;background:blue}</style><main><div></div></main>",
            "https://example.test/",
        );
        let main = page.dom.elements_named("main").next().unwrap();
        let child = page.dom.elements_named("div").next().unwrap();
        let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        let box_rect = output.node_bounds[&main.id()];
        assert_eq!((box_rect.width, box_rect.height), (100.0, 100.0));
        assert_eq!(output.node_bounds[&child.id()].width, 50.0);
        let expected = RectF {
            x: box_rect.x + 10.0,
            y: box_rect.y + 10.0,
            width: 80.0,
            height: 80.0,
        };
        assert!(output.items.iter().any(|item| matches!(item,
            DisplayItem::BeginClip { bounds } if *bounds == expected)));
        assert!(output.items.iter().any(|item| matches!(item,
            DisplayItem::EndClip { bounds } if *bounds == expected)));
        assert!(!output.point_in_scroll_clips(&main, 5.0, 5.0));
        assert!(!output.point_in_scroll_clips(&child, 5.0, 5.0));
        assert!(output.point_in_scroll_clips(&child, 20.0, 20.0));
    }

    #[test]
    fn transformed_clip_and_descendant_input_share_visual_coordinates() {
        let page = Page::parse(
            "<style>body{margin:0}main{width:100px;height:100px;clip-path:inset(10px);transform:translate(30px,20px)}div{width:50px;height:50px}</style><main><div></div></main>",
            "https://example.test/",
        );
        let main = page.dom.elements_named("main").next().unwrap();
        let child = page.dom.elements_named("div").next().unwrap();
        let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        assert_eq!(output.clip_paths[&main.id()].x, 40.0);
        assert_eq!(output.clip_paths[&main.id()].y, 30.0);
        assert!(!output.point_in_scroll_clips(&child, 35.0, 25.0));
        assert!(output.point_in_scroll_clips(&child, 45.0, 35.0));
    }
}
