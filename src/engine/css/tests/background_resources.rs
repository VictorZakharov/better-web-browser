//! Cascade ownership and URL resolution for background and mask resources.
use super::*;

#[test]
fn resolves_background_images_against_the_stylesheet_url() {
    let dom = dom::parse(r#"<a class="logo"></a>"#);
    let stylesheets = vec![(
            "https://cdn.example/assets/css/site.css".to_string(),
            r#".logo {
                width: 65px;
                height: 60px;
                background: no-repeat center/auto 36px url('../logo.svg'), linear-gradient(transparent, transparent);
            }"#
                .to_string(),
        )];
    let styles = StyleSet::from_sources_for_viewport(
        &dom,
        "https://example.com/page/",
        &stylesheets,
        1000.0,
        1000.0,
    );
    let logo = dom.elements_named("a").next().unwrap();
    let style = styles.get(&logo);
    assert_eq!(
        style.background_image.as_deref(),
        Some("https://cdn.example/assets/logo.svg")
    );
    assert!(!style.background_repeat_x);
    assert!(!style.background_repeat_y);
    assert_eq!(style.background_position_x, Length::Percent(50.0));
    assert_eq!(style.background_position_y, Length::Percent(50.0));
    assert_eq!(
        style.background_size,
        BackgroundSize::Explicit {
            width: Length::Auto,
            height: Length::Px(36.0)
        }
    );
}

#[test]
fn resolves_standard_and_prefixed_mask_images() {
    let dom = dom::parse(r#"<span class="icon"></span>"#);
    let stylesheets = vec![(
        "https://cdn.example/assets/css/icons.css".to_string(),
        ".icon { -webkit-mask-image: url('../menu.svg'); mask-image: url('../menu.svg') }"
            .to_string(),
    )];
    let styles = StyleSet::from_sources_for_viewport(
        &dom,
        "https://example.com/",
        &stylesheets,
        1000.0,
        1000.0,
    );
    let icon = dom.elements_named("span").next().unwrap();

    assert_eq!(
        styles.get(&icon).mask_image.as_deref(),
        Some("https://cdn.example/assets/menu.svg")
    );
}

#[test]
fn resolves_mask_shorthand_images_after_custom_property_substitution() {
    let dom = dom::parse(r#"<span class="icon"></span>"#);
    let stylesheets = vec![(
        "https://cdn.example/assets/css/icons.css".to_string(),
        ".icon { --logo: url('../menu.svg'); mask: var(--logo) center no-repeat }".to_string(),
    )];
    let styles = StyleSet::from_sources_for_viewport(
        &dom,
        "https://example.com/",
        &stylesheets,
        1000.0,
        1000.0,
    );
    let icon = dom.elements_named("span").next().unwrap();

    assert_eq!(
        styles.get(&icon).mask_image.as_deref(),
        Some("https://cdn.example/assets/menu.svg")
    );
}
