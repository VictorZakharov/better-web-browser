use super::*;

#[test]
fn flow_root_heading_border_avoids_external_float_without_clipping() {
    for display in ["flow-root", "block flow-root", "flow-root block"] {
        let (page, layout) = boxes(&format!(
            "<aside id=float style='float:right;width:200px;height:100px;margin-left:20px'></aside>\
             <section id=heading style='display:{display};height:30px;border-bottom:1px solid'>Heading</section>"
        ));
        let heading = rect(&page, &layout, "heading");
        assert_eq!(
            (heading.x, heading.y, heading.width, heading.height),
            (0.0, 0.0, 380.0, 31.0),
            "{display}"
        );
        assert!(
            !layout
                .items
                .iter()
                .any(|item| matches!(item, DisplayItem::BeginClip { .. }))
        );
    }
}

#[test]
fn flow_root_contains_floats_and_does_not_collapse_child_margins() {
    let (page, layout) = boxes(
        "<main id=root style='display:flow-root'><div id=child style='height:20px;margin:15px 0'></div>\
         <aside style='float:left;width:10px;height:100px'></aside></main><div id=after></div>",
    );
    assert_eq!(rect(&page, &layout, "root").y, 0.0);
    assert_eq!(rect(&page, &layout, "child").y, 15.0);
    assert_eq!(rect(&page, &layout, "root").height, 150.0);
    assert_eq!(rect(&page, &layout, "after").y, 150.0);
}
