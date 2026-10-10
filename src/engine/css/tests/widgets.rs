use super::*;
use crate::engine::Page;

#[test]
fn accent_inherits_resolved_currentcolor_but_appearance_does_not_inherit_by_default() {
    let page = Page::parse(
        "<style>main{color:red;accent-color:currentColor;appearance:none}input{color:blue}#inherit{appearance:inherit;accent-color:inherit}</style><main><input type=checkbox><input id=inherit type=radio></main>",
        "https://example.test/",
    );
    let styles = page.style(500.0);
    let inputs: Vec<_> = page.dom.elements_named("input").collect();
    let child = styles.get(&inputs[0]);
    assert_eq!(child.appearance, Appearance::Auto);
    assert_eq!(
        child.accent_color.resolve(child.color),
        Some(Color::rgb(255, 0, 0))
    );
    let inherited = styles.get(&inputs[1]);
    assert_eq!(inherited.appearance, Appearance::None);
    assert_eq!(
        inherited.accent_color.resolve(inherited.color),
        Some(Color::rgb(255, 0, 0))
    );
}

#[test]
fn widget_syntax_is_preserved_without_claiming_the_whole_native_control_contract() {
    for property in ["appearance", "-webkit-appearance", "accent-color"] {
        assert!(!super::super::supports::supports_property(property));
    }
    assert!(super::super::supports::cssom_declaration_value(
        "appearance",
        "none"
    ));
    assert!(super::super::supports::cssom_declaration_value(
        "accent-color",
        "currentColor"
    ));
    assert!(!super::super::supports::cssom_declaration_value(
        "appearance",
        "made-up-theme"
    ));
}
