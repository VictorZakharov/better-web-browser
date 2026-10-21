//! Aggregate inline fragments after final flex/grid/positioned translations.
//! Bounding client rectangles include non-atomic inline boxes, not just block boxes.
//! https://drafts.csswg.org/cssom-view/#dom-element-getboundingclientrect
use super::*;

pub(in crate::engine::layout) fn include(
    bounds: &mut HashMap<NodeId, RectF>,
    id: NodeId,
    rect: RectF,
) {
    bounds
        .entry(id)
        .and_modify(|old| {
            let right = old.right().max(rect.right());
            let bottom = old.bottom().max(rect.bottom());
            old.x = old.x.min(rect.x);
            old.y = old.y.min(rect.y);
            old.width = right - old.x;
            old.height = bottom - old.y;
        })
        .or_insert(rect);
}

pub(in crate::engine::layout) fn finish(
    root: &NodeRef,
    styles: &StyleSet,
    output: &mut LayoutOutput,
) {
    // Children finish before their parents, so a plain inline's bounds include its nested plain
    // inlines.
    struct Open {
        node: NodeRef,
        children: Vec<NodeRef>,
        next: usize,
        /// A plain inline without bounds of its own takes the union of its children's.
        aggregates: bool,
        union: Option<RectF>,
    }
    let open = |node: NodeRef, output: &mut LayoutOutput| {
        let inline = node.element().is_some()
            && styles
                .styles
                .get(&node.id())
                .is_some_and(|style| style.display == Display::Inline);
        let own = output.node_bounds.get(&node.id()).copied();
        // Existing atomic/decorated inline boxes already own their border geometry. Their
        // descendants' font bands must not replace that border area, and they are never
        // enlarged by overflowing descendants. Empty, undecorated inlines retain a caret band.
        if inline && own.is_some_and(|rect| rect.width > 0.0) {
            std::sync::Arc::make_mut(&mut output.fragments)
                .elements
                .remove(&node.id());
        }
        Open {
            children: Node::composed_children(&node),
            node,
            next: 0,
            aggregates: inline && own.is_none(),
            union: None,
        }
    };
    let mut stack = vec![open(root.clone(), output)];
    while let Some(top) = stack.last_mut() {
        if let Some(child) = top.children.get(top.next).cloned() {
            top.next += 1;
            let child = open(child, output);
            stack.push(child);
            continue;
        }
        let done = stack.pop().expect("the loop holds an open node");
        let rect = match done.union {
            Some(rect) => {
                output.node_bounds.insert(done.node.id(), rect);
                Some(rect)
            }
            None => output.node_bounds.get(&done.node.id()).copied(),
        };
        if let (Some(rect), Some(parent)) = (rect, stack.last_mut())
            && parent.aggregates
        {
            parent.union = Some(
                parent
                    .union
                    .map_or(rect, |union| fragments::union(union, rect)),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::layout::test_support::FixedMeasurer;

    #[test]
    fn plain_inline_bounds_survive_wrapping_and_translated_ancestors() {
        let page = Page::parse(
            r#"<!doctype html><style>
            body{margin:0} main{width:130px;transform:translate(20px,30px)}
            h2{display:inline;font:16px Arial} p{margin:0}
        </style><main><h2 id=section>alpha <b>beta gamma delta</b> epsilon</h2><p>end</p></main>"#,
            "https://example.test/",
        );
        let painted = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        let geometry =
            layout_geometry_with_style_viewport(&page, 800.0, 600.0, 800.0, &mut FixedMeasurer);
        assert_eq!(painted.node_bounds, geometry.node_bounds);
        let heading = page.dom.elements_named("h2").next().unwrap();
        let bold = page.dom.elements_named("b").next().unwrap();
        let bounds = painted.node_bounds[&heading.id()];
        assert!(bounds.x >= 20.0 && bounds.y >= 30.0, "{bounds:?}");
        assert!(bounds.height > 30.0, "wrapped inline union: {bounds:?}");
        let child = painted.node_bounds[&bold.id()];
        assert!(child.x >= bounds.x && child.right() <= bounds.right());
        assert!(child.y >= bounds.y && child.bottom() <= bounds.bottom());
        assert!(geometry.items.is_empty());
    }
}
