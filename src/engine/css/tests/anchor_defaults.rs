//! Placeholder anchors inherit text styling; href presence enables UA link defaults.
use super::*;

#[test]
fn link_defaults_require_href_and_remain_below_author_styles() {
    let dom = dom::parse(
        "<style>body{color:red}#authored{color:green;text-decoration:none}</style><a id=placeholder>placeholder</a><a id=empty href=''>empty</a><a id=linked href='/'>link</a><a id=authored href='/'>authored</a>",
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    for node in dom.elements_named("a") {
        let style = styles.get(&node);
        match node.attr("id").as_deref() {
            Some("placeholder") => {
                assert_eq!(style.color, Color::rgb(255, 0, 0));
                assert!(!style.text_decoration_underline);
            }
            Some("authored") => {
                assert_eq!(style.color, Color::rgb(0, 128, 0));
                assert!(!style.text_decoration_underline);
            }
            _ => {
                assert_eq!(style.color, Color::rgb(0, 0, 238));
                assert!(style.text_decoration_underline);
            }
        }
    }
}
