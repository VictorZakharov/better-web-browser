//! HTML's rendering section hides these elements in the user-agent style sheet.
//! https://html.spec.whatwg.org/multipage/rendering.html#hidden-elements
use super::*;

#[test]
fn html_hidden_elements_do_not_generate_boxes() {
    for tag in [
        "area", "base", "basefont", "datalist", "head", "link", "meta", "noembed", "noframes",
        "param", "rp", "script", "style", "template", "title",
    ] {
        assert_eq!(user_agent::user_agent_display(tag), Display::None, "{tag}");
    }
    assert_eq!(user_agent::user_agent_display("span"), Display::Inline);
}
