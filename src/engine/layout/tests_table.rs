use super::*;

fn table_layout(markup: &str) -> (LayoutOutput, NodeRef) {
    let page = Page::parse(markup, "https://example.com/");
    let table = page.dom.elements_named("table").next().unwrap();
    let mut measurer = FixedMeasurer;
    (layout_page(&page, 800.0, 600.0, &mut measurer), table)
}

#[test]
fn table_ua_sizing_and_collapsed_borders_use_the_table_grid_box() {
    let cases = [
        (
            "<table style='width:20px;height:30px;padding:1px 2px 3px 4px'></table>",
            (20.0, 30.0),
        ),
        (
            "<table style='width:20px;height:30px;padding:1px 2px 3px 4px;box-sizing:content-box'></table>",
            (26.0, 34.0),
        ),
        (
            "<table style='width:20px;height:30px;border-width:2px 4px 6px 8px;border-style:solid;border-collapse:collapse;box-sizing:content-box'><tr><td></td></tr></table>",
            (26.0, 34.0),
        ),
    ];

    for (table, expected) in cases {
        let markup = format!("<style>body{{margin:0}}</style>{table}");
        let (output, table) = table_layout(&markup);
        let bounds = output.node_bounds.get(&table.id()).unwrap();
        assert_eq!((bounds.width, bounds.height), expected);
    }

    let (output, table) = table_layout(
        "<style>body{margin:0}</style><div style='display:flex'><table style='width:20px;height:30px;border-width:2px 4px 6px 8px;border-style:solid;border-collapse:collapse;box-sizing:content-box'><tr><td></td></tr></table></div>",
    );
    let bounds = output.node_bounds.get(&table.id()).unwrap();
    assert_eq!((bounds.width, bounds.height), (26.0, 34.0));
}

#[test]
fn captions_expand_the_wrapper_and_stack_above_or_below_the_grid() {
    let (output, table) = table_layout(
        "<style>body{margin:0}</style><table style='width:20px;height:30px'><caption style='width:40px;height:20px'></caption></table>",
    );
    let bounds = output.node_bounds.get(&table.id()).unwrap();
    assert_eq!((bounds.width, bounds.height), (40.0, 50.0));

    let (output, table) = table_layout(
        "<style>body{margin:0}</style><table><caption style='width:40px;height:50px;padding:1px 2px 3px 4px'></caption></table>",
    );
    let bounds = output.node_bounds.get(&table.id()).unwrap();
    assert_eq!((bounds.width, bounds.height), (46.0, 54.0));

    let page = Page::parse(
        "<style>body{margin:0}</style><table style='width:20px;height:30px;caption-side:bottom'><caption style='width:40px;height:20px'></caption></table>",
        "https://example.com/",
    );
    let table = page.dom.elements_named("table").next().unwrap();
    let caption = page.dom.elements_named("caption").next().unwrap();
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);
    assert_eq!(output.node_bounds.get(&table.id()).unwrap().height, 50.0);
    assert_eq!(output.node_bounds.get(&caption.id()).unwrap().y, 30.0);
}
