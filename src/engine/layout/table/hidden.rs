use super::*;
use crate::engine::invalidation::RenderInvalidation;
use crate::engine::layout::test_support::FixedMeasurer;

fn layout(markup: &str, sparse: bool) -> (Page, LayoutOutput) {
    let page = Page::parse(
        &format!(
            "<style>body{{margin:0}}table{{width:300px}}td,th{{padding:0;border:0}}\
             .hidden{{display:none}}.box{{height:20px}}</style>{markup}"
        ),
        "https://example.com/",
    );
    let mut page = if sparse { page.layout_snapshot() } else { page };
    if sparse {
        let invalidation = RenderInvalidation::full(page.dom.document.id());
        page.refresh_layout_styles_after_invalidation_for_viewport(300.0, 200.0, &invalidation);
    }
    let output = layout_page(&page, 300.0, 200.0, &mut FixedMeasurer);
    (page, output)
}

fn by_id(page: &Page, id: &str) -> NodeRef {
    Node::descendants(&page.dom.document)
        .find(|node| node.attr("id").as_deref() == Some(id))
        .unwrap()
}

#[test]
fn hidden_table_sections_and_rows_do_not_visit_deferred_descendants() {
    let markup = "<table>
        <caption class=hidden><div id=caption-content>hidden caption</div></caption>
        <thead class=hidden><tr><th id=head-cell>hidden header</th></tr></thead>
        <tbody class=hidden><tr><td id=body-cell>hidden body</td></tr></tbody>
        <tbody><tr class=hidden><td id=row-cell>hidden row</td></tr>
          <tr id=visible-row><td><div class=box id=visible>visible</div></td></tr></tbody>
        <tfoot class=hidden><tr><td id=foot-cell>hidden footer</td></tr></tfoot>
        </table>";
    for sparse in [false, true] {
        let (page, output) = layout(markup, sparse);
        let visible = by_id(&page, "visible");
        assert_eq!(output.node_bounds[&visible.id()].y, 0.0);
        assert_eq!(output.node_bounds[&visible.id()].width, 300.0);
        for id in [
            "caption-content",
            "head-cell",
            "body-cell",
            "row-cell",
            "foot-cell",
        ] {
            let hidden = by_id(&page, id);
            assert!(!output.node_bounds.contains_key(&hidden.id()), "{id}");
            if sparse {
                assert!(
                    !page
                        .cached_style_for_viewport(300.0, 200.0)
                        .unwrap()
                        .styles
                        .contains_key(&hidden.id()),
                    "{id}"
                );
            }
        }
    }
}

#[test]
fn hidden_cells_do_not_consume_column_width_or_row_height() {
    let markup = "<table><tbody><tr>
        <td class=hidden style='width:290px'><div id=hidden-content style='height:500px'>hidden</div></td>
        <td><div class=box id=left>left</div></td>
        <th><div class=box id=right>right</div></th>
        </tr><tr><td><div class=box id=next>next</div></td></tr></tbody></table>";
    for sparse in [false, true] {
        let (page, output) = layout(markup, sparse);
        let left = output.node_bounds[&by_id(&page, "left").id()];
        let right = output.node_bounds[&by_id(&page, "right").id()];
        let next = output.node_bounds[&by_id(&page, "next").id()];
        assert_eq!((left.x, left.width), (0.0, 150.0));
        assert_eq!((right.x, right.width), (150.0, 150.0));
        assert_eq!(next.y, 20.0);
        assert!(
            !output
                .node_bounds
                .contains_key(&by_id(&page, "hidden-content").id())
        );
    }
}
