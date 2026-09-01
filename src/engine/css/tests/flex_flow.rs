use super::super::*;

#[test]
fn flex_flow_sets_direction_and_wrapping_in_either_order() {
    let dom = dom::parse(
        r#"<main>
             <section id="column" style="display:flex;flex-flow:column nowrap"></section>
             <section id="wrapped" style="display:flex;flex-flow:wrap row"></section>
             <section id="reversed" style="display:flex;flex-flow:wrap row-reverse"></section>
           </main>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let column = dom
        .elements_named("section")
        .find(|node| node.attr("id").as_deref() == Some("column"))
        .unwrap();
    let wrapped = dom
        .elements_named("section")
        .find(|node| node.attr("id").as_deref() == Some("wrapped"))
        .unwrap();
    let reversed = dom
        .elements_named("section")
        .find(|node| node.attr("id").as_deref() == Some("reversed"))
        .unwrap();

    assert_eq!(styles.get(&column).flex_direction, FlexDirection::Column);
    assert!(!styles.get(&column).flex_wrap);
    assert_eq!(styles.get(&wrapped).flex_direction, FlexDirection::Row);
    assert!(styles.get(&wrapped).flex_wrap);
    assert_eq!(
        styles.get(&reversed).flex_direction,
        FlexDirection::RowReverse
    );
    assert!(styles.get(&reversed).flex_wrap);
}

#[test]
fn supports_reports_only_implemented_flex_flow_values() {
    assert!(supports::supports_matches("(flex-flow: column nowrap)"));
    assert!(supports::supports_matches("(flex-flow: wrap row)"));
    assert!(supports::supports_matches("(flex-flow: row-reverse)"));
    assert!(supports::supports_matches(
        "(flex-direction: column-reverse)"
    ));
    assert!(!supports::supports_matches("(flex-flow: column column)"));
}
