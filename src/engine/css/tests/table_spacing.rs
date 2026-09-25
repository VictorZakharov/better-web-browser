use super::*;

fn styles_for(markup: &str) -> (dom::Dom, StyleSet) {
    let dom = dom::parse(markup);
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    (dom, styles)
}

#[test]
fn html_table_defaults_apply_only_to_html_table_and_cells() {
    let (dom, styles) = styles_for(
        "<table><tr><td id=cell>A</td><th id=heading>B</th></tr></table>\
         <div style='display:table'><div style='display:table-row'><div id=css-cell style='display:table-cell'>C</div></div></div>",
    );
    let table = dom.elements_named("table").next().unwrap();
    assert_eq!(styles.get(&table).border_spacing, [Length::Px(2.0); 2]);
    for name in ["td", "th"] {
        let cell = dom.elements_named(name).next().unwrap();
        assert_eq!(styles.get(&cell).padding, uniform_edges(Length::Px(1.0)));
    }
    let css_cell = dom
        .elements_named("div")
        .find(|node| node.attr("id").as_deref() == Some("css-cell"))
        .unwrap();
    assert_eq!(styles.get(&css_cell).padding, Edges::ZERO);
}

#[test]
fn spacing_inherits_and_author_declarations_override_table_hints() {
    let (dom, styles) = styles_for(
        "<style>table#author {border-spacing:7px 9px} #author td {padding:3px}</style>\
         <table id=author cellspacing='4' cellpadding='5'><tr><td>text</td></tr></table>\
         <table id=hint cellspacing='  +6junk' cellpadding='  +8junk'><tr><td>text</td></tr></table>",
    );
    let tables = dom.elements_named("table").collect::<Vec<_>>();
    assert_eq!(
        styles.get(&tables[0]).border_spacing,
        [Length::Px(7.0), Length::Px(9.0)]
    );
    assert_eq!(styles.get(&tables[1]).border_spacing, [Length::Px(6.0); 2]);
    let cells = dom.elements_named("td").collect::<Vec<_>>();
    assert_eq!(
        styles.get(&cells[0]).padding,
        uniform_edges(Length::Px(3.0))
    );
    assert_eq!(
        styles.get(&cells[1]).padding,
        uniform_edges(Length::Px(8.0))
    );
    assert_eq!(styles.get(&cells[1]).border_spacing, [Length::Px(6.0); 2]);
}

#[test]
fn nested_table_uses_own_default_or_hint_instead_of_inherited_spacing() {
    let (dom, styles) = styles_for(
        "<table style='border-spacing:11px'><tr><td><table id=inner><tr><td>one</td></tr></table>\
         <table id=hint cellspacing=3><tr><td>two</td></tr></table></td></tr></table>",
    );
    let tables = dom.elements_named("table").collect::<Vec<_>>();
    assert_eq!(styles.get(&tables[0]).border_spacing, [Length::Px(11.0); 2]);
    assert_eq!(styles.get(&tables[1]).border_spacing, [Length::Px(2.0); 2]);
    assert_eq!(styles.get(&tables[2]).border_spacing, [Length::Px(3.0); 2]);
}

#[test]
fn invalid_spacing_does_not_clobber_earlier_valid_cascade_value() {
    let (dom, styles) = styles_for(
        "<style>table{border-spacing:5px 7px;border-spacing:-1px;\
         border-spacing:2%;border-spacing:1px 2px 3px}</style><table></table>",
    );
    let table = dom.elements_named("table").next().unwrap();
    assert_eq!(
        styles.get(&table).border_spacing,
        [Length::Px(5.0), Length::Px(7.0)]
    );
}

#[test]
fn css_wide_keywords_restore_inherited_initial_and_ua_values() {
    let (dom, styles) = styles_for(
        "<div style='border-spacing:8px 10px'>\
         <section id=inherited style='border-spacing:inherit'></section>\
         <section id=initial style='border-spacing:initial'></section>\
         <table id=reverted style='border-spacing:revert'></table></div>",
    );
    let sections = dom.elements_named("section").collect::<Vec<_>>();
    assert_eq!(
        styles.get(&sections[0]).border_spacing,
        [Length::Px(8.0), Length::Px(10.0)]
    );
    assert_eq!(
        styles.get(&sections[1]).border_spacing,
        [Length::Px(0.0); 2]
    );
    let table = dom.elements_named("table").next().unwrap();
    assert_eq!(styles.get(&table).border_spacing, [Length::Px(2.0); 2]);
}

#[test]
fn computed_style_serializes_both_lengths_even_while_borders_are_collapsed() {
    let (dom, styles) = styles_for(
        "<table style='font-size:20px;border-collapse:collapse;border-spacing:1em 3px'></table>",
    );
    let table = dom.elements_named("table").next().unwrap();
    let style = styles.get(&table);
    assert_eq!(style.used_border_spacing(), (0.0, 0.0));
    assert_eq!(
        resolved_property_value(style, "border-spacing").as_deref(),
        Some("20px 3px")
    );
    assert!(supports::supports_matches("(border-spacing: 3px 5px)"));
    assert!(!supports::supports_matches("(border-spacing: 3%)"));
}
