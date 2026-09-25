//! Physical scroll-margin and scroll-padding properties from CSS Scroll Snap 1.
//! These also adjust CSSOM View's scroll-into-view target and optimal viewing region.
//! https://drafts.csswg.org/css-scroll-snap-1/#scroll-margin
//! https://drafts.csswg.org/css-scroll-snap-1/#scroll-padding

use super::{ComputedStyle, Edges, Length, parse_length};

pub(super) fn supports(property: &str, value: &str) -> bool {
    let padding = property.starts_with("scroll-padding");
    let parts = match parse_parts(value, padding) {
        Some(parts) => parts,
        None => return false,
    };
    property == "scroll-margin" || property == "scroll-padding" || parts.len() == 1
}

pub(super) fn apply(style: &mut ComputedStyle, property: &str, value: &str) {
    if !supports(property, value) {
        return;
    }
    let parts = parse_parts(value, property.starts_with("scroll-padding")).unwrap();
    let target = if property.starts_with("scroll-padding") {
        &mut style.scroll_padding
    } else {
        &mut style.scroll_margin
    };
    match property {
        "scroll-margin" | "scroll-padding" => *target = expand_edges(&parts),
        name if name.ends_with("-top") => target.top = parts[0],
        name if name.ends_with("-right") => target.right = parts[0],
        name if name.ends_with("-bottom") => target.bottom = parts[0],
        name if name.ends_with("-left") => target.left = parts[0],
        _ => {}
    }
}

fn parse_parts(value: &str, padding: bool) -> Option<Vec<Length>> {
    let tokens = split_components(value)?;
    if !(1..=4).contains(&tokens.len()) {
        return None;
    }
    tokens
        .into_iter()
        .map(|token| {
            let length = parse_length(token)?;
            let valid = match length {
                Length::Auto => padding,
                Length::Percent(value) => padding && value >= 0.0,
                Length::Px(value)
                | Length::Em(value)
                | Length::Rem(value)
                | Length::Vw(value)
                | Length::Vh(value)
                | Length::Vmin(value)
                | Length::Vmax(value) => !padding || value >= 0.0,
                Length::Calc { percent, .. } => padding || percent == 0.0,
            };
            valid.then_some(length)
        })
        .collect()
}

// Whitespace separates shorthand sides except within a CSS function such as calc().
pub(super) fn split_components(value: &str) -> Option<Vec<&str>> {
    let mut parts = Vec::new();
    let mut depth = 0_u32;
    let mut start = None;
    for (offset, character) in value.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => depth = depth.checked_sub(1)?,
            _ => {}
        }
        if character.is_ascii_whitespace() && depth == 0 {
            if let Some(begin) = start.take() {
                parts.push(&value[begin..offset]);
            }
        } else if start.is_none() {
            start = Some(offset);
        }
    }
    if depth != 0 {
        return None;
    }
    if let Some(begin) = start {
        parts.push(&value[begin..]);
    }
    Some(parts)
}

pub(super) fn expand_edges(parts: &[Length]) -> Edges {
    let top = parts[0];
    let right = *parts.get(1).unwrap_or(&top);
    let bottom = *parts.get(2).unwrap_or(&top);
    let left = *parts.get(3).unwrap_or(&right);
    Edges {
        top,
        right,
        bottom,
        left,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{css::StyleSet, dom};

    #[test]
    fn shorthand_expands_one_to_four_sides_and_keeps_calc_together() {
        let mut style = ComputedStyle::initial();
        apply(&mut style, "scroll-margin", "1px 2px 3px");
        assert_eq!(style.scroll_margin.top, Length::Px(1.0));
        assert_eq!(style.scroll_margin.right, Length::Px(2.0));
        assert_eq!(style.scroll_margin.bottom, Length::Px(3.0));
        assert_eq!(style.scroll_margin.left, Length::Px(2.0));
        apply(&mut style, "scroll-margin", "calc(1px + 2px) 4px");
        assert_eq!(style.scroll_margin.left, Length::Px(4.0));
        assert_eq!(style.scroll_margin.top, Length::Px(3.0));
    }

    #[test]
    fn rejects_invalid_values_atomically() {
        let mut style = ComputedStyle::initial();
        apply(&mut style, "scroll-padding", "4px");
        apply(&mut style, "scroll-padding", "2px -4px");
        assert_eq!(style.scroll_padding.left, Length::Px(4.0));
        assert!(!supports("scroll-margin", "10%"));
        assert!(!supports("scroll-margin-top", "auto"));
        assert!(!supports("scroll-padding-top", "2px 3px"));
        assert!(supports("scroll-padding", "auto 10%"));
    }

    #[test]
    fn cascade_resolves_font_units_preserves_percentages_and_serializes_cssom() {
        let dom = dom::parse(
            r#"
            <style>
                html { font-size: 10px }
                #pane { scroll-padding: auto 10% 1rem 2em; font-size: 12px }
                #target { scroll-margin: 1rem 2em -3px 4px; font-size: 20px }
            </style>
            <div id="pane"><span id="target">text</span></div>
        "#,
        );
        let styles = StyleSet::from_dom(&dom, &[], 800.0);
        let pane = dom.elements_named("div").next().unwrap();
        let target = dom.elements_named("span").next().unwrap();
        let pane_style = styles.get(&pane);
        let target_style = styles.get(&target);
        assert_eq!(pane_style.scroll_padding.top, Length::Auto);
        assert_eq!(pane_style.scroll_padding.right, Length::Percent(10.0));
        assert_eq!(pane_style.scroll_padding.bottom, Length::Px(10.0));
        assert_eq!(pane_style.scroll_padding.left, Length::Em(2.0));
        assert_eq!(target_style.scroll_margin.top, Length::Px(10.0));
        assert_eq!(target_style.scroll_margin.right, Length::Em(2.0));
        assert_eq!(target_style.scroll_margin.bottom, Length::Px(-3.0));
        assert_eq!(
            super::super::resolved_property_value(pane_style, "scroll-padding-right").as_deref(),
            Some("10%")
        );
        assert_eq!(
            super::super::resolved_property_value(pane_style, "scroll-padding-left").as_deref(),
            Some("24px")
        );
        assert_eq!(
            super::super::resolved_property_value(target_style, "scroll-margin-right").as_deref(),
            Some("40px")
        );
        assert_eq!(
            super::super::resolved_property_value(target_style, "scroll-margin").as_deref(),
            Some("10px 40px -3px 4px")
        );
        assert_eq!(
            super::super::resolved_property_value(pane_style, "scroll-padding").as_deref(),
            Some("auto 10% 10px 24px")
        );
    }

    #[test]
    fn css_wide_keywords_reset_or_inherit_without_inheriting_by_default() {
        let dom = dom::parse(
            r#"
            <style>
                #parent { scroll-margin: 5px; scroll-padding: 7px }
                #initial { scroll-margin: inherit; scroll-padding: initial }
                #inherited { scroll-padding-left: inherit }
            </style>
            <div id="parent"><i id="initial"></i><i id="inherited"></i></div>
        "#,
        );
        let styles = StyleSet::from_dom(&dom, &[], 800.0);
        let initial = dom.elements_named("i").next().unwrap();
        let inherited = dom.elements_named("i").nth(1).unwrap();
        assert_eq!(styles.get(&initial).scroll_margin.left, Length::Px(5.0));
        assert_eq!(styles.get(&initial).scroll_padding.left, Length::Auto);
        assert_eq!(styles.get(&inherited).scroll_margin.left, Length::Px(0.0));
        assert_eq!(styles.get(&inherited).scroll_padding.left, Length::Px(7.0));
    }
}
