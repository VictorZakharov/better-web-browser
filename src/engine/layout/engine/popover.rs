//! Open popovers are separate top-layer roots, outside ancestor clips and stacking contexts.
//! https://www.w3.org/TR/css-position-4/#top-layer

use super::*;

pub(super) fn append_top_layer<M: TextMeasurer>(
    page: &Page,
    styles: &StyleSet,
    output: &mut LayoutOutput,
    width: f32,
    height: f32,
    measurer: &mut M,
    emit_paint: bool,
) {
    let mut popovers = Node::shadow_including_descendants(&page.dom.document)
        .filter(|node| node.is_popover_open() && node.attr("popover").is_some())
        .filter(|node| {
            // A real display:none ancestor suppresses even a top-layer element.
            std::iter::successors(Some(node.clone()), |node| node.shadow_including_parent())
                .filter(|node| node.element().is_some())
                .all(|node| {
                    styles
                        .styles
                        .get(&node.id())
                        .is_some_and(|style| style.display != Display::None)
                })
        })
        .collect::<Vec<_>>();
    popovers.sort_by_key(|node| node.popover_order());
    for root in popovers {
        let tree = box_tree::BoxTree::new(&root, styles);
        let mut engine = LayoutEngine {
            page,
            styles: &tree,
            measurer,
            emit_paint,
            retain_fragments: true,
            scroll_gutters: HashMap::new(),
            measurement_cache: HashMap::new(),
            intrinsic_block_heights: HashMap::new(),
            intrinsic_widths: Default::default(),
            margin_profiles: Default::default(),
            inline_box_cache: HashMap::new(),
            positioned_flow_scopes: Vec::new(),
            floats: Default::default(),
            viewport: RectF {
                x: 0.0,
                y: 0.0,
                width: width.max(1.0),
                height: height.max(1.0),
            },
            output: LayoutOutput {
                background: Color::TRANSPARENT,
                content_height: height,
                ..LayoutOutput::default()
            },
        };
        engine.layout_block(&root, 0.0, 0.0, width.max(1.0), Some(height.max(1.0)), None);
        inline_layout::geometry::finish(&root, styles, &mut engine.output);
        block::paint_order::finalize(&mut engine.output);
        tree.remove_anonymous_geometry(&mut engine.output);
        let layer = engine.output;
        let fragments = std::sync::Arc::make_mut(&mut output.fragments);
        let line_offset = fragments.next_line;
        for (&id, value) in &layer.fragments.elements {
            let mut value = value.clone();
            for fragment in &mut value {
                fragment.line += line_offset;
            }
            fragments.elements.insert(id, value);
        }
        for (&id, value) in &layer.fragments.text {
            let mut value = value.clone();
            for fragment in &mut value {
                fragment.line += line_offset;
            }
            fragments.text.insert(id, value);
        }
        fragments.next_line += layer.fragments.next_line;
        output.sticky_offsets.extend(layer.sticky_offsets);
        output.sticky_layers.extend(layer.sticky_layers);
        output.scroll_boxes.extend(layer.scroll_boxes);
        output.clip_paths.extend(layer.clip_paths);
        output.hit_excluded.extend(layer.hit_excluded);
        output.items.extend(layer.items);
        output.node_bounds.extend(layer.node_bounds);
        output.resize_boxes.extend(layer.resize_boxes);
        output.node_paint_order.extend(layer.node_paint_order);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixedMeasurer;

    impl TextMeasurer for FixedMeasurer {
        fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
            (text.chars().count() as f32 * font.size * 0.5, font.size)
        }
    }

    #[test]
    fn open_popovers_paint_after_page_content_in_top_layer_order() {
        let page = Page::parse(
            "<div style='overflow:hidden;width:40px;height:20px'>\
               <div id=first popover style='width:80px;height:40px'>First</div>\
               <div id=second popover style='width:80px;height:40px'>Second</div>\
             </div><p id=outside>Outside</p>",
            "https://example.test/",
        );
        let first = page.dom.elements_named("div").nth(1).unwrap();
        let second = page.dom.elements_named("div").nth(2).unwrap();
        let outside = page.dom.elements_named("p").next().unwrap();
        first.set_popover_order(1);
        second.set_popover_order(2);
        let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        // The UA border and padding add to the author's 80px content width.
        assert_eq!(output.node_bounds[&first.id()].width, 92.0);
        assert_eq!(output.node_bounds[&second.id()].width, 92.0);
        let second_rect = output.node_bounds[&second.id()];
        assert!(output.point_in_scroll_clips(&second, second_rect.x + 5.0, second_rect.y + 5.0));
        let hits = HitTestSnapshot::from_layout(&output, &page.dom.document)
            .elements_at(second_rect.x + 5.0, second_rect.y + 5.0);
        assert_eq!(hits.first().map(|node| node.id()), Some(second.id()));
        let order = &output.node_paint_order;
        let index = |node: &NodeRef| order.iter().rposition(|id| *id == node.id()).unwrap();
        assert!(index(&outside) < index(&first));
        assert!(index(&first) < index(&second));
    }

    #[test]
    fn unsized_popover_shrink_wraps_and_centers_in_viewport() {
        let page = Page::parse("<div popover>Short menu</div>", "https://example.test/");
        let popover = page.dom.elements_named("div").next().unwrap();
        popover.set_popover_order(1);

        let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        let bounds = output.node_bounds[&popover.id()];
        assert!(bounds.width < 200.0, "unexpected popover width: {bounds:?}");
        assert!(
            bounds.height < 100.0,
            "unexpected popover height: {bounds:?}"
        );
        assert!(
            (bounds.x - (800.0 - bounds.width) / 2.0).abs() < 1.0,
            "{bounds:?}"
        );
        assert!(
            (bounds.y - (600.0 - bounds.height) / 2.0).abs() < 1.0,
            "{bounds:?}"
        );
    }
}
