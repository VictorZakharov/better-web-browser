//! Cascade, serialization and feature-query contracts for CSS text truncation:
//! `text-overflow`, `-webkit-line-clamp` and `-webkit-box-orient`, plus the
//! authored legacy `-webkit-box` display flag they activate on.

use super::super::values::{BoxOrient, LineClamp, TextOverflow};
use super::*;
use crate::engine::dom;

fn styled(html: &str) -> (dom::Dom, StyleSet) {
    let dom = dom::parse(html);
    let styles = cascade::StyleSet::from_dom(&dom, &[], 1000.0);
    (dom, styles)
}

fn node_by_id(dom: &dom::Dom, tag: &str, id: &str) -> dom::NodeRef {
    dom.elements_named(tag)
        .find(|node| node.attr("id").as_deref() == Some(id))
        .unwrap()
}

#[test]
fn truncation_properties_default_to_unclamped_clip() {
    let (dom, styles) = styled("<div></div>");
    let style = styles.get(&dom.elements_named("div").next().unwrap());
    assert_eq!(style.text_overflow, TextOverflow::Clip);
    assert_eq!(style.line_clamp, LineClamp::None);
    assert_eq!(style.box_orient, BoxOrient::Horizontal);
    assert!(!style.legacy_webkit_box);
}

#[test]
fn parses_valid_truncation_values() {
    let (dom, styles) = styled(
        r#"<style>
            #overflow { text-overflow: ellipsis; }
            #clamp { -webkit-line-clamp: 3; }
            #orient { -webkit-box-orient: vertical; }
            #alias { -webkit-box-orient: block-axis; }
        </style>
        <div id="overflow"></div><div id="clamp"></div>
        <div id="orient"></div><div id="alias"></div>"#,
    );
    assert_eq!(
        styles
            .get(&node_by_id(&dom, "div", "overflow"))
            .text_overflow,
        TextOverflow::Ellipsis
    );
    assert_eq!(
        styles.get(&node_by_id(&dom, "div", "clamp")).line_clamp,
        LineClamp::Lines(3)
    );
    assert_eq!(
        styles.get(&node_by_id(&dom, "div", "orient")).box_orient,
        BoxOrient::Vertical
    );
    assert_eq!(
        styles.get(&node_by_id(&dom, "div", "alias")).box_orient,
        BoxOrient::Vertical
    );
}

#[test]
fn rejects_invalid_truncation_values_without_losing_previous_ones() {
    let (dom, styles) = styled(
        r#"<style>
            div {
                text-overflow: ellipsis; text-overflow: dots;
                -webkit-line-clamp: 2; -webkit-line-clamp: 0;
                -webkit-box-orient: vertical; -webkit-box-orient: sideways;
            }
            #zero { -webkit-line-clamp: 0; }
            #negative { -webkit-line-clamp: -2; }
            #fraction { -webkit-line-clamp: 2.5; }
            #empty { text-overflow: ; }
            #two-valued { text-overflow: clip ellipsis; }
        </style>
        <div id="plain"></div><div id="zero"></div><div id="negative"></div>
        <div id="fraction"></div><div id="empty"></div><div id="two-valued"></div>"#,
    );
    let plain = styles.get(&node_by_id(&dom, "div", "plain"));
    assert_eq!(plain.text_overflow, TextOverflow::Ellipsis);
    assert_eq!(plain.line_clamp, LineClamp::Lines(2));
    assert_eq!(plain.box_orient, BoxOrient::Vertical);
    for id in ["zero", "negative", "fraction"] {
        assert_eq!(
            styles.get(&node_by_id(&dom, "div", id)).line_clamp,
            LineClamp::Lines(2),
            "{id} must be rejected, preserving the earlier valid count"
        );
    }
    for id in ["empty", "two-valued"] {
        assert_eq!(
            styles.get(&node_by_id(&dom, "div", id)).text_overflow,
            TextOverflow::Ellipsis,
            "{id} must be rejected, preserving the earlier valid value"
        );
    }
}

#[test]
fn accepts_large_clamp_counts_without_special_handling() {
    let (dom, styles) = styled("<div id=\"big\" style=\"-webkit-line-clamp: 1000000\"></div>");
    assert_eq!(
        styles.get(&node_by_id(&dom, "div", "big")).line_clamp,
        LineClamp::Lines(1_000_000)
    );
}

#[test]
fn applies_css_wide_keywords_to_truncation_properties() {
    let (dom, styles) = styled(
        r#"<style>
            .parent {
                text-overflow: ellipsis;
                -webkit-line-clamp: 2;
                -webkit-box-orient: vertical;
                display: -webkit-box;
            }
            #inherit {
                text-overflow: inherit; -webkit-line-clamp: inherit;
                -webkit-box-orient: inherit; display: inherit;
            }
            #initial {
                text-overflow: ellipsis; text-overflow: initial;
                -webkit-line-clamp: 2; -webkit-line-clamp: initial;
            }
            #unset {
                text-overflow: ellipsis; text-overflow: unset;
                -webkit-line-clamp: 2; -webkit-line-clamp: unset;
                -webkit-box-orient: vertical; -webkit-box-orient: unset;
            }
        </style>
        <div class="parent"><span id="inherit"></span><span id="initial"></span><span id="unset"></span></div>"#,
    );
    // None of the truncation properties inherit; `inherit` on a child without a
    // clamped parent still resolves against the (default) parent values.
    let inherited = styles.get(&node_by_id(&dom, "span", "inherit"));
    assert_eq!(inherited.text_overflow, TextOverflow::Ellipsis);
    assert_eq!(inherited.line_clamp, LineClamp::Lines(2));
    assert_eq!(inherited.box_orient, BoxOrient::Vertical);
    assert!(inherited.legacy_webkit_box);
    let initial = styles.get(&node_by_id(&dom, "span", "initial"));
    assert_eq!(initial.text_overflow, TextOverflow::Clip);
    assert_eq!(initial.line_clamp, LineClamp::None);
    // `unset` behaves as `initial` for these non-inherited properties.
    let unset = styles.get(&node_by_id(&dom, "span", "unset"));
    assert_eq!(unset.text_overflow, TextOverflow::Clip);
    assert_eq!(unset.line_clamp, LineClamp::None);
    assert_eq!(unset.box_orient, BoxOrient::Horizontal);
}

#[test]
fn truncation_does_not_leak_into_unrelated_blocks() {
    let (dom, styles) = styled(
        r#"<style>
            #legacy {
                display: -webkit-box; -webkit-box-orient: vertical;
                -webkit-line-clamp: 2;
            }
        </style><div id="legacy"><div id="child"></div></div><div id="sibling"></div>"#,
    );
    let legacy = styles.get(&node_by_id(&dom, "div", "legacy"));
    assert_eq!(legacy.display, Display::Block);
    assert!(legacy.legacy_webkit_box);
    for id in ["child", "sibling"] {
        let style = styles.get(&node_by_id(&dom, "div", id));
        assert_eq!(style.line_clamp, LineClamp::None, "{id} must not clamp");
        assert!(!style.legacy_webkit_box, "{id} must stay an ordinary block");
    }
}

#[test]
fn later_flex_display_still_wins_over_legacy_box() {
    let (dom, styles) = styled(
        r#"<h1 style="display:block;display:-webkit-box">first</h1>
           <nav style="display:-webkit-box;display:flex">second</nav>
           <p style="display:-webkit-box;display:inline">third</p>
           <b style="display:-webkit-box;display:bogus">fourth</b>"#,
    );
    let first = styles.get(&dom.elements_named("h1").next().unwrap());
    assert_eq!(first.display, Display::Block);
    assert!(first.legacy_webkit_box);
    let second = styles.get(&dom.elements_named("nav").next().unwrap());
    assert_eq!(second.display, Display::Flex);
    assert!(!second.legacy_webkit_box);
    let third = styles.get(&dom.elements_named("p").next().unwrap());
    assert!(!third.legacy_webkit_box);
    // An invalid value preserves the preceding valid declaration wholesale.
    let fourth = styles.get(&dom.elements_named("b").next().unwrap());
    assert_eq!(fourth.display, Display::Block);
    assert!(fourth.legacy_webkit_box);
}

#[test]
fn honors_cascade_precedence_and_variable_substitution() {
    let (dom, styles) = styled(
        r#"<style>
            :root { --marker: ellipsis; --count: 2; }
            div { text-overflow: clip; -webkit-line-clamp: none; }
            .low { text-overflow: var(--marker); -webkit-line-clamp: var(--count); }
            #high { text-overflow: clip !important; }
        </style>
        <div id="plain"></div>
        <div id="low" class="low"></div>
        <div id="high" class="low" style="text-overflow: ellipsis"></div>
        <div id="inline" style="text-overflow: ellipsis; -webkit-line-clamp: 4"></div>"#,
    );
    assert_eq!(
        styles.get(&node_by_id(&dom, "div", "plain")).text_overflow,
        TextOverflow::Clip
    );
    let low = styles.get(&node_by_id(&dom, "div", "low"));
    assert_eq!(low.text_overflow, TextOverflow::Ellipsis);
    assert_eq!(low.line_clamp, LineClamp::Lines(2));
    let high = styles.get(&node_by_id(&dom, "div", "high"));
    assert_eq!(high.text_overflow, TextOverflow::Clip);
    assert_eq!(high.line_clamp, LineClamp::Lines(2));
    let inline = styles.get(&node_by_id(&dom, "div", "inline"));
    assert_eq!(inline.text_overflow, TextOverflow::Ellipsis);
    assert_eq!(inline.line_clamp, LineClamp::Lines(4));
}

#[test]
fn serializes_computed_truncation_values() {
    let mut style = ComputedStyle::initial();
    assert_eq!(
        resolved_property_value(&style, "text-overflow").as_deref(),
        Some("clip")
    );
    assert_eq!(
        resolved_property_value(&style, "-webkit-line-clamp").as_deref(),
        Some("none")
    );
    assert_eq!(
        resolved_property_value(&style, "-webkit-box-orient").as_deref(),
        Some("horizontal")
    );
    style.text_overflow = TextOverflow::Ellipsis;
    style.line_clamp = LineClamp::Lines(2);
    style.box_orient = BoxOrient::Vertical;
    assert_eq!(
        resolved_property_value(&style, "text-overflow").as_deref(),
        Some("ellipsis")
    );
    assert_eq!(
        resolved_property_value(&style, "-webkit-line-clamp").as_deref(),
        Some("2")
    );
    assert_eq!(
        resolved_property_value(&style, "-webkit-box-orient").as_deref(),
        Some("vertical")
    );
}

#[test]
fn feature_queries_match_the_implemented_grammars() {
    for supported in [
        "(text-overflow: clip)",
        "(text-overflow: ellipsis)",
        "(-webkit-line-clamp: none)",
        "(-webkit-line-clamp: 3)",
        "(-webkit-box-orient: vertical)",
        "(-webkit-box-orient: horizontal)",
        "(text-overflow: inherit)",
    ] {
        assert!(
            super::super::supports::supports_matches(supported),
            "{supported} must be supported"
        );
    }
    for unsupported in [
        "(text-overflow: dots)",
        "(text-overflow: clip ellipsis)",
        "(-webkit-line-clamp: 0)",
        "(-webkit-line-clamp: -1)",
        "(-webkit-line-clamp: 1.5)",
        "(-webkit-line-clamp: many)",
        "(-webkit-box-orient: sideways)",
        "(line-clamp: 2)",
    ] {
        assert!(
            !super::super::supports::supports_matches(unsupported),
            "{unsupported} must not be supported"
        );
    }
}

#[test]
fn style_changes_classify_truncation_correctly() {
    let before = ComputedStyle::initial();
    let mut after = ComputedStyle::initial();
    after.text_overflow = TextOverflow::Ellipsis;
    assert!(
        before.layout_equivalent(&after),
        "swapping the overflow marker alone must stay paint-only"
    );
    after = ComputedStyle::initial();
    after.line_clamp = LineClamp::Lines(2);
    assert!(
        !before.layout_equivalent(&after),
        "changing the clamp count must trigger relayout"
    );
    after = ComputedStyle::initial();
    after.box_orient = BoxOrient::Vertical;
    assert!(!before.layout_equivalent(&after));
    after = ComputedStyle::initial();
    after.display = Display::Block;
    after.legacy_webkit_box = true;
    assert!(
        !before.layout_equivalent(&after),
        "toggling the legacy box flag must trigger relayout"
    );
}
