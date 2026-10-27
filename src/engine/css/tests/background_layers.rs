use super::*;

fn style(value: &str) -> ComputedStyle {
    let dom = dom::parse(&format!("<i style='{value}'></i>"));
    StyleSet::from_dom(&dom, &[], 800.0)
        .get(&dom.elements_named("i").next().unwrap())
        .clone()
}

#[test]
fn each_shorthand_layer_retains_its_own_size_position_and_repeat() {
    let style = style(
        "background:linear-gradient(red,red) 0 0/26px 2px no-repeat,linear-gradient(blue,blue) 100% 100%/2px 26px no-repeat #123456",
    );
    assert_eq!(style.background_image.0.len(), 2);
    assert_eq!(
        style.background_layers.at(1),
        (
            BackgroundSize::Explicit {
                width: Length::Px(2.0),
                height: Length::Px(26.0)
            },
            Length::Percent(100.0),
            Length::Percent(100.0),
            (false, false)
        )
    );
    assert_eq!(style.background_color, Color::rgb(0x12, 0x34, 0x56));
    assert_eq!(
        resolved_property_value(&style, "background-size").as_deref(),
        Some("26px 2px, 2px 26px")
    );
}

#[test]
fn lists_cycle_independently_and_longhands_only_replace_their_own_component() {
    let style = style(
        "background:none,none,none;background-size:10px 20px,30px 40px;background-position:10% 20%,30% 40%;background-position-y:80%;background-repeat:no-repeat repeat",
    );
    assert_eq!(
        style.background_layers.at(2).0,
        BackgroundSize::Explicit {
            width: Length::Px(10.0),
            height: Length::Px(20.0)
        }
    );
    assert!(
        matches!(style.background_layers.at(1).1, Length::Percent(v) if (v - 30.0).abs() < 0.0001)
    );
    assert_eq!(style.background_layers.at(1).2, Length::Percent(80.0));
    assert_eq!(style.background_layers.at(1).3, (false, true));
}

#[test]
fn invalid_background_layers_do_not_partially_reset_prior_declarations() {
    for invalid in [
        "red, blue",
        "none,",
        "left left",
        "none /20px",
        "none 0/20px garbage",
        "none center /-1px",
        "red blue",
        "none none",
        "repeat-x no-repeat",
        "linear-gradient(red,blue),garbage",
    ] {
        let style = style(&format!("background:blue;background:{invalid}"));
        assert_eq!(style.background_color, Color::rgb(0, 0, 255), "{invalid}");
        assert!(style.background_image.is_none(), "{invalid}");
    }
}

#[test]
fn css_wide_keywords_copy_and_reset_the_complete_lists() {
    let dom = dom::parse(
        "<div style='background:none,none;background-position:10% 20%,30% 40%;background-size:10px,20px'><i style='background-position:inherit;background-size:inherit'></i><b style='background:inherit;background-size:initial'></b></div>",
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let child = styles.get(&dom.elements_named("i").next().unwrap());
    assert_eq!(child.background_layers.sizes.len(), 2);
    assert_eq!(child.background_layers.x.len(), 2);
    let reset = styles.get(&dom.elements_named("b").next().unwrap());
    assert_eq!(*reset.background_layers.sizes, [BackgroundSize::Auto]);
    assert_eq!(reset.background_layers.x.len(), 2);
}

#[test]
fn supports_rejects_malformed_lists_instead_of_accepting_only_the_first_layer() {
    for (property, value) in [
        ("background-size", "cover,garbage"),
        ("background-position", "left top,garbage"),
        ("background-repeat", "no-repeat,repeat-x repeat"),
        ("background-position-x", "10px,auto"),
    ] {
        assert!(
            !supports::supports_declaration_value(property, value),
            "{property}: {value}"
        );
    }
    assert!(supports::supports_declaration_value(
        "background-size",
        "cover, calc(20px + 10%) auto"
    ));
    assert!(supports::supports_declaration_value(
        "background-position",
        "right bottom, calc(20px + 10%) center"
    ));
}
