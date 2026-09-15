use super::*;

fn styles(dom: &Dom, sources: &[StylesheetSource]) -> StyleSet {
    StyleSet::from_sources_for_media_environment(
        dom,
        "https://example.test/",
        sources,
        MediaEnvironment::new(800.0, 600.0, 1.0, false),
    )
}

#[test]
fn linked_and_inline_sheets_follow_owner_order_not_completion_order() {
    let dom = dom::parse(
        "<link rel=stylesheet href=a.css><style>p{color:green}</style><link rel=stylesheet href=b.css><p>text</p>",
    );
    let a = StylesheetSource::linked("https://example.test/a.css", "p{color:red}".into());
    let b = StylesheetSource::linked("https://example.test/b.css", "p{color:blue}".into());
    let target = dom.elements_named("p").next().unwrap();
    let first = styles(&dom, &[b.clone(), a.clone()]);
    let same = styles(&dom, &[a.clone(), b.clone()]);
    assert_eq!(first.get(&target).color, Color::rgb(0, 0, 255));
    assert!(
        Rc::ptr_eq(&first.compiled, &same.compiled),
        "response reordering does not change the cascade"
    );
    let link = dom.elements_named("link").next().unwrap();
    Node::append_child(&dom.elements_named("head").next().unwrap(), link.clone());
    assert_eq!(
        styles(&dom, &[b.clone(), a.clone()]).get(&target).color,
        Color::rgb(255, 0, 0)
    );
    Node::remove_from_parent(&link);
    assert_eq!(
        styles(&dom, &[b, a]).get(&target).color,
        Color::rgb(0, 0, 255)
    );
}

#[test]
fn detached_disabled_non_css_and_nonmatching_media_owners_have_no_effect() {
    let dom =
        dom::parse("<style>p{color:green}</style><link rel=stylesheet href=a.css><p>text</p>");
    let source = StylesheetSource::linked("https://example.test/a.css", "p{color:red}".into());
    let link = dom.elements_named("link").next().unwrap();
    let target = dom.elements_named("p").next().unwrap();
    for (attribute, value) in [
        ("media", "print"),
        ("type", "text/plain"),
        ("disabled", ""),
        ("rel", "preload"),
    ] {
        link.set_attr(attribute, value);
        assert_eq!(
            styles(&dom, std::slice::from_ref(&source))
                .get(&target)
                .color,
            Color::rgb(0, 128, 0),
            "{attribute}"
        );
        link.remove_attr(attribute);
        link.set_attr("rel", "stylesheet");
    }
    Node::remove_from_parent(&link);
    assert_eq!(
        styles(&dom, &[source]).get(&target).color,
        Color::rgb(0, 128, 0)
    );
}

#[test]
fn repeated_link_owners_share_payload_but_keep_each_tree_position() {
    let dom = dom::parse(
        "<link rel=stylesheet href=a.css><style>p{color:green}</style><link rel=stylesheet href=a.css><p>text</p>",
    );
    let source = StylesheetSource::linked("https://example.test/a.css", "p{color:red}".into());
    let target = dom.elements_named("p").next().unwrap();
    assert_eq!(
        styles(&dom, std::slice::from_ref(&source))
            .get(&target)
            .color,
        Color::rgb(255, 0, 0)
    );
    Node::remove_from_parent(&dom.elements_named("link").last().unwrap());
    assert_eq!(
        styles(&dom, &[source]).get(&target).color,
        Color::rgb(0, 128, 0)
    );
}

#[test]
fn imports_precede_parent_rules_and_repeat_at_each_import_position() {
    let dom = dom::parse(
        "<style>@import 'a.css'; @import 'b.css'; @import 'a.css'; p{width:123px}</style><p>text</p>",
    );
    let sources = [
        StylesheetSource::linked("https://example.test/a.css", "@import 'child.css';".into()),
        StylesheetSource::linked(
            "https://example.test/b.css",
            "p{color:blue;width:456px}".into(),
        ),
        StylesheetSource::linked(
            "https://example.test/child.css",
            "p{color:red;width:789px}".into(),
        ),
    ];
    let target = dom.elements_named("p").next().unwrap();
    let result = styles(&dom, &sources);
    assert_eq!(result.get(&target).color, Color::rgb(255, 0, 0));
    assert_eq!(result.get(&target).width, Length::Px(123.0));
    dom.elements_named("style")
        .next()
        .unwrap()
        .set_attr("media", "print");
    assert_ne!(
        styles(&dom, &sources).get(&target).color,
        Color::rgb(255, 0, 0)
    );
}
