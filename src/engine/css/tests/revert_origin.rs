//! Elements skip copying the revert origin only when no declaration can resolve to `revert`;
//! every route that can still rolls back to the user-agent value.
use super::*;

fn input_display(sheet: &str, inline: &str) -> Display {
    let dom = dom::parse(&format!(
        "<style>input {{ display: block; }} {sheet}</style><input style=\"{inline}\">"
    ));
    let styles = StyleSet::from_dom(&dom, &[], 1000.0);
    styles
        .get(&dom.elements_named("input").next().unwrap())
        .display
}

#[test]
fn every_route_to_revert_restores_the_user_agent_value() {
    for sheet in [
        "input { display: revert; }",
        "input { display: REVERT; }",
        r"input { display: r\65 vert; }",
        "input { display: var(--missing, revert); }",
        "input { display: revert-layer; }",
        "input { --kept: 1; display: var(--missing, revert-layer); }",
    ] {
        assert_eq!(input_display(sheet, ""), Display::InlineBlock, "{sheet}");
    }
    assert_eq!(input_display("", "display: revert"), Display::InlineBlock);
    // Without `revert`, the author value applies.
    assert_eq!(input_display("", ""), Display::Block);
}

#[test]
fn revert_rolls_back_presentational_hints_too() {
    // Presentational hints count as author-level declarations, so `revert` restores the value
    // from before them.
    let dom = dom::parse(r#"<img width=50 style="width: revert"><img width=50>"#);
    let styles = StyleSet::from_dom(&dom, &[], 1000.0);
    let mut images = dom.elements_named("img");
    assert_eq!(styles.get(&images.next().unwrap()).width, Length::Auto);
    assert_eq!(styles.get(&images.next().unwrap()).width, Length::Px(50.0));
}
