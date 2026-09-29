use super::*;

#[test]
fn popover_ua_visibility_and_open_selector_follow_showing_state() {
    let dom = dom::parse("<div id=menu popover>Menu</div>");
    let menu = dom.elements_named("div").next().unwrap();
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(styles.get(&menu).display, Display::None);
    assert_eq!(
        styles
            .query_selector_all(&dom, ":popover-open")
            .unwrap()
            .len(),
        0
    );

    menu.set_popover_order(1);
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(styles.get(&menu).position, Position::Fixed);
    assert_ne!(styles.get(&menu).display, Display::None);
    assert_eq!(
        styles
            .query_selector_all(&dom, ":popover-open")
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn author_display_can_override_the_ua_hidden_rule() {
    let dom = dom::parse("<style>[popover]{display:block}</style><div popover>Menu</div>");
    let menu = dom.elements_named("div").next().unwrap();
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(styles.get(&menu).display, Display::Block);
    assert!(!menu.is_popover_open());
}
