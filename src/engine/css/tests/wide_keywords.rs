use super::*;

#[test]
fn resolves_across_inherited_non_inherited_and_user_agent_values() {
    let dom = dom::parse(
        r#"<style>
                .parent { color: #123456; width: 81px; visibility: visible; }
                #inherit {
                    color: black; color: inherit;
                    width: 1px; width: inherit;
                    visibility: hidden; visibility: inherit;
                }
                #initial { color: red; color: initial; width: 2px; width: initial; }
                #unset { color: blue; color: unset; width: 3px; width: unset; }
                input {
                    display: block; display: revert;
                    border-width: 9px; border-width: revert-layer;
                }
                #all { display: block; color: blue; width: 4px; all: unset; }
               </style>
               <div class=parent>
                 <span id=inherit></span><span id=initial></span><span id=unset></span>
                 <span id=all></span><input>
               </div>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 1000.0);
    let by_id = |id| {
        dom::Node::descendants(&dom.document)
            .find(|node| node.attr("id").as_deref() == Some(id))
            .unwrap()
    };

    let inherited = styles.get(&by_id("inherit"));
    assert_eq!(inherited.color, Color::rgb(0x12, 0x34, 0x56));
    assert_eq!(inherited.width, Length::Px(81.0));
    assert!(inherited.visibility);

    let initial = styles.get(&by_id("initial"));
    assert_eq!(initial.color, Color::BLACK);
    assert_eq!(initial.width, Length::Auto);

    let unset = styles.get(&by_id("unset"));
    assert_eq!(unset.color, Color::rgb(0x12, 0x34, 0x56));
    assert_eq!(unset.width, Length::Auto);

    let all = styles.get(&by_id("all"));
    assert_eq!(all.display, Display::Inline);
    assert_eq!(all.color, Color::rgb(0x12, 0x34, 0x56));
    assert_eq!(all.width, Length::Auto);

    let input = dom.elements_named("input").next().unwrap();
    let reverted = styles.get(&input);
    assert_eq!(reverted.display, Display::InlineBlock);
    assert_eq!(reverted.border_width, uniform_edges(Length::Px(2.0)));

    for keyword in ["inherit", "initial", "unset", "revert", "revert-layer"] {
        assert!(crate::engine::css::supports::supports_matches(&format!(
            "@supports (visibility: {keyword})"
        )));
        assert!(crate::engine::css::supports::supports_matches(&format!(
            "@supports (all: {keyword})"
        )));
    }
}
