//! `:visited` paints only colors, and only in the rendering cascade.
//! https://drafts.csswg.org/selectors-4/#link
use super::*;
use crate::engine::css::selector_match::visited;

const CSS: &str = "a:link { color: red; background-color: rgba(0, 0, 255, 0.5); width: 10px }
    a:visited { color: white; background-color: lime; width: 50px; display: block }
    a { border: 1px solid red } a:visited { border-color: lime }";

fn styles(html: &str) -> (Dom, StyleSet) {
    let dom = dom::parse(html);
    let styles = StyleSet::from_sources_for_viewport(
        &dom,
        "https://example.test/",
        &[("".into(), CSS.into())],
        800.0,
        600.0,
    );
    (dom, styles)
}

#[test]
fn rendering_paints_visited_link_colors_but_keeps_unvisited_geometry_and_alpha() {
    visited::clear_visited_urls();
    visited::record_visited_url("https://example.test/seen");
    let (dom, styles) =
        styles(r#"<a id=seen href="seen">x</a><a id=other href="other">y</a><a id=bare>z</a>"#);
    let [seen, other, bare] = ["seen", "other", "bare"].map(|id| {
        dom.elements_named("a")
            .find(|node| node.attr("id").as_deref() == Some(id))
            .unwrap()
    });
    let seen = styles.get(&seen);
    assert_eq!(seen.color, Color::WHITE);
    // Alpha comes from the unvisited value; geometry and display never follow :visited.
    assert_eq!(
        seen.background_color,
        Color {
            alpha: 128,
            ..Color::rgb(0, 255, 0)
        }
    );
    assert_eq!(seen.border_colors[0], Some(Color::rgb(0, 255, 0)));
    assert_eq!(seen.width, Length::Px(10.0));
    assert_eq!(seen.display, Display::Inline);
    assert_eq!(styles.get(&other).color, Color::rgb(255, 0, 0));
    // A placeholder anchor without href is not a link.
    assert_eq!(styles.get(&bare).color, Color::BLACK);
    visited::clear_visited_urls();
}

#[test]
fn user_agent_visited_link_color_follows_the_html_rendering_suggestion() {
    // https://html.spec.whatwg.org/multipage/rendering.html#phrasing-content-3
    visited::clear_visited_urls();
    visited::record_visited_url("https://example.test/seen");
    let dom = dom::parse(r#"<a href="seen">x</a><a href="other">y</a>"#);
    let styles =
        StyleSet::from_sources_for_viewport(&dom, "https://example.test/", &[], 800.0, 600.0);
    let links = dom.elements_named("a").collect::<Vec<_>>();
    assert_eq!(styles.get(&links[0]).color, Color::rgb(85, 26, 139));
    assert_eq!(styles.get(&links[1]).color, Color::rgb(0, 0, 238));
    visited::clear_visited_urls();
}

#[test]
fn visited_history_is_partitioned_by_top_level_site() {
    visited::clear_visited_urls();
    visited::record_top_level_navigation("https://a.test/page");
    visited::record_visited_url("https://example.test/seen");
    let html = r#"<a href="https://a.test/page">a</a><a href="seen">b</a>"#;
    let (dom, rendered) = styles(html);
    let links = dom.elements_named("a").collect::<Vec<_>>();
    assert_eq!(rendered.get(&links[0]).color, Color::WHITE);
    assert_eq!(rendered.get(&links[1]).color, Color::WHITE);
    // Another top-level site starts with an empty history.
    visited::record_top_level_navigation("https://b.test/");
    let (dom, rendered) = styles(html);
    for link in dom.elements_named("a") {
        assert_eq!(rendered.get(&link).color, Color::rgb(255, 0, 0));
    }
    visited::clear_visited_urls();
}

#[test]
fn link_and_visited_together_never_match() {
    visited::clear_visited_urls();
    visited::record_visited_url("https://example.test/seen");
    let dom = dom::parse(r#"<a href="seen">x</a><a href="other">y</a>"#);
    let css = "a:link:visited { color: lime }";
    let styles = StyleSet::from_sources_for_viewport(
        &dom,
        "https://example.test/",
        &[("".into(), css.into())],
        800.0,
        600.0,
    );
    // Each link keeps its user-agent color: visited purple, unvisited blue, never lime.
    let links = dom.elements_named("a").collect::<Vec<_>>();
    assert_eq!(styles.get(&links[0]).color, Color::rgb(85, 26, 139));
    assert_eq!(styles.get(&links[1]).color, Color::rgb(0, 0, 238));
    visited::clear_visited_urls();
}
