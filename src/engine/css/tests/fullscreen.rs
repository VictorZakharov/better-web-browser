use super::super::*;

#[test]
fn fullscreen_ua_style_fills_the_viewport_and_beats_author_geometry() {
    let dom = dom::parse(
        r#"<div id="player" style="position:static;width:10px;height:20px;margin:30px;
           min-width:900px;max-width:40px;min-height:800px;max-height:50px"></div>"#,
    );
    let player = dom.elements_named("div").next().unwrap();
    player.set_fullscreen(true);
    let styles = StyleSet::from_sources_for_viewport(&dom, "", &[], 1280.0, 720.0);
    let style = styles.get(&player);
    assert_eq!(style.position, Position::Fixed);
    assert_eq!(style.box_sizing, BoxSizing::BorderBox);
    assert_eq!(style.margin, Edges::ZERO);
    assert_eq!(style.width, Length::Px(1280.0));
    assert_eq!(style.height, Length::Px(720.0));
    assert_eq!(style.min_width, Length::Px(0.0));
    assert_eq!(style.min_height, Length::Px(0.0));
    assert_eq!(style.max_width, Length::Auto);
    assert_eq!(style.max_height, Length::Auto);
}
