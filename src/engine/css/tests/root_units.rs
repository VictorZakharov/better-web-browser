use super::*;

#[test]
fn rem_lengths_follow_the_computed_root_font_size_across_properties() {
    let dom = dom::parse(
        r#"<style>
            html { font-size: 10px }
            #target {
                font-size: 1.6rem;
                width: 20rem;
                margin-left: calc(1rem + 2px);
                transform: translateX(2rem);
            }
        </style><div id="target">root relative</div>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let root = dom.elements_named("html").next().unwrap();
    let target = dom
        .elements_named("div")
        .find(|node| node.attr("id").as_deref() == Some("target"))
        .unwrap();
    let style = styles.get(&target);

    assert_eq!(styles.get(&root).font_size, 10.0);
    assert_eq!(style.font_size, 16.0);
    assert_eq!(style.width, Length::Px(200.0));
    assert_eq!(style.margin.left, Length::Px(12.0));
    assert_eq!(style.transform.resolve(100.0, 100.0, 16.0), (20.0, 0.0));
}

#[test]
fn rem_on_the_root_font_size_uses_the_initial_font_size() {
    let dom =
        dom::parse(r#"<style>html { font-size: 2rem } body { width: 3rem }</style><p>text</p>"#);
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let root = dom.elements_named("html").next().unwrap();
    let body = dom.elements_named("body").next().unwrap();

    assert_eq!(styles.get(&root).font_size, 32.0);
    assert_eq!(styles.get(&body).width, Length::Px(96.0));
}

#[test]
fn calc_preserves_rem_until_the_root_basis_is_known() {
    assert_eq!(parse_length("2rem"), Some(Length::Rem(2.0)));
    assert_eq!(
        parse_length("calc(1rem + 5px)")
            .unwrap()
            .resolve_root_font_units(10.0),
        Length::Px(15.0)
    );
}
