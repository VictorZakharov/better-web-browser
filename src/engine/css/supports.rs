//! CSS Conditional Rules feature-query evaluation.

use super::values::{BoxOrient, LineClamp, TextOverflow};
use super::*;
mod condition;
mod selector;
#[cfg(test)]
mod tests;

/// Evaluate CSS Conditional Rules queries against features the engine actually implements.
/// https://drafts.csswg.org/css-conditional-3/#at-supports
/// https://drafts.csswg.org/css-conditional-4/#at-supports
pub(crate) fn supports_matches(prelude: &str) -> bool {
    let condition = super::at_rule_prelude(prelude.trim(), "supports")
        .unwrap_or(prelude)
        .trim();
    condition::evaluate(condition)
}

/// Check the syntax of a `<supports-condition>` independently of whether its features exist.
/// An unknown but well-formed feature can evaluate false while remaining a valid import modifier.
pub(crate) fn supports_condition_valid(prelude: &str) -> bool {
    condition::valid(prelude)
}

/// Check the bare `<declaration>` alternative accepted inside `@import ... supports()`.
pub(crate) fn supports_import_declaration_valid(prelude: &str) -> bool {
    condition::declaration_valid(prelude)
}

/// Evaluate the two-argument CSS.supports(property, value) overload. Unlike a declaration-form
/// feature query, its property name is used literally and its value cannot carry `!important`.
/// https://drafts.csswg.org/css-conditional-3/#the-css-namespace
pub(crate) fn supports_declaration_value(property: &str, value: &str) -> bool {
    let mut input = ParserInput::new(property);
    let mut parser = Parser::new(&mut input);
    let Ok(Token::Ident(name)) = parser.next_including_whitespace_and_comments().cloned() else {
        return false;
    };
    if name.as_ref() != property || !parser.is_exhausted() {
        return false;
    }
    let Some(value) = condition::property_value(value) else {
        return false;
    };
    supports_declaration(property, value.trim())
}

pub(crate) fn supports_property(property: &str) -> bool {
    // These declarations currently establish containing blocks only. They do not
    // implement the visual effect and must not opt authors into 3D/filter branches.
    // https://drafts.csswg.org/css-transforms-2/#two-dimensional-subset
    !matches!(property, "perspective" | "transform-style" | "filter")
        && super::css_wide::supports_css_wide_keyword(property, "initial")
}

fn supports_declaration(property: &str, value: &str) -> bool {
    if property.is_empty()
        || value
            .trim_end()
            .to_ascii_lowercase()
            .ends_with("!important")
    {
        return false;
    }
    if property.starts_with("--") {
        return property.len() > 2;
    }
    if value.is_empty() {
        return false;
    }
    let property = property.to_ascii_lowercase();
    if matches!(
        property.as_str(),
        "perspective" | "transform-style" | "filter"
    ) {
        return false;
    }
    // A supported property with a syntactically valid var() reference is valid at parse time.
    // Its computed value may still become invalid after substitution; CSS.supports() must not
    // resolve variables or use the property's final-value parser for this case.
    // https://www.w3.org/TR/css-variables-1/#using-variables
    if supports_property(&property) && variables::contains_valid_variable_reference(value) {
        return true;
    }
    let value = value.to_ascii_lowercase();
    if super::css_wide::supports_css_wide_keyword(&property, &value) {
        return true;
    }
    match property.as_str() {
        "content" => GeneratedContent::parse(&value).is_some(),
        "display" => matches!(
            value.as_str(),
            "none"
                | "contents"
                | "block"
                | "flow"
                | "block flow"
                | "flow block"
                | "flow-root"
                | "block flow-root"
                | "flow-root block"
                | "inline flow-root"
                | "flow-root inline"
                | "inline flow"
                | "flow inline"
                | "inline flex"
                | "flex inline"
                | "block flex"
                | "flex block"
                | "inline"
                | "inline-block"
                | "inline-flex"
                | "-webkit-inline-flex"
                | "flex"
                | "-webkit-flex"
                | "-webkit-box"
                | "grid"
                | "-ms-grid"
                | "table"
                | "table-row"
                | "table-cell"
        ),
        "position" => matches!(
            value.as_str(),
            "static" | "relative" | "absolute" | "fixed" | "sticky"
        ),
        "z-index" => value == "auto" || value.parse::<i32>().is_ok(),
        "float" => matches!(value.as_str(), "none" | "left" | "right"),
        "clear" => super::Clear::parse(&value).is_some(),
        "box-sizing" | "-webkit-box-sizing" => {
            matches!(value.as_str(), "content-box" | "border-box")
        }
        "border-collapse" => matches!(value.as_str(), "separate" | "collapse"),
        "border-spacing" => properties::table_spacing::parse(&value).is_some(),
        "caption-side" => matches!(value.as_str(), "top" | "bottom"),
        "visibility" => matches!(value.as_str(), "visible" | "hidden" | "collapse"),
        "content-visibility" => matches!(value.as_str(), "visible" | "hidden"),
        "pointer-events" => matches!(value.as_str(), "auto" | "none"),
        "overflow" | "overflow-x" | "overflow-y" => {
            matches!(value.as_str(), "visible" | "hidden" | "clip")
        }
        "color" | "background-color" => parse_color(&value).is_some(),
        "border-color" => values::borders::color_values(&value).is_some(),
        "border-top-color" | "border-right-color" | "border-bottom-color" | "border-left-color" => {
            values::borders::color_values(&value).is_some_and(|colors| colors.len() == 1)
        }
        "width"
        | "height"
        | "min-width"
        | "min-height"
        | "max-width"
        | "max-height"
        | "top"
        | "right"
        | "bottom"
        | "left"
        | "inset"
        | "margin-top"
        | "margin-right"
        | "margin-bottom"
        | "margin-left"
        | "padding-top"
        | "padding-right"
        | "padding-bottom"
        | "padding-left"
        | "border-top-width"
        | "border-right-width"
        | "border-bottom-width"
        | "border-left-width"
        | "column-gap"
        | "grid-column-gap"
        | "row-gap"
        | "grid-row-gap"
        | "flex-basis"
        | "-webkit-flex-basis"
        | "-moz-flex-basis" => supported_length(&value),
        "opacity" => single_component_or_calc(&value) && parse_opacity(&value).is_some(),
        "transform" => super::transform::parse_transform(&value).is_some(),
        "clip-path" => super::clip_path::ClipPath::parse(&value).is_some(),
        "background-image" | "mask" | "-webkit-mask" | "mask-image" | "-webkit-mask-image" => {
            value == "none" || value.starts_with("url(")
        }
        "background-position" => parse_background_position(&value).is_some(),
        "background-position-x" => parse_background_axis(&value, true).is_some(),
        "background-position-y" => parse_background_axis(&value, false).is_some(),
        "background-size" => parse_background_size(&value).is_some(),
        "object-fit" => ObjectFit::parse(&value).is_some(),
        "object-position" => ObjectPosition::parse(&value).is_some(),
        "text-transform" => TextTransform::parse(&value).is_some(),
        "aspect-ratio" => AspectRatio::parse(&value).is_some(),
        "background-repeat" => {
            let repeats = value.split_ascii_whitespace().collect::<Vec<_>>();
            (1..=2).contains(&repeats.len())
                && repeats
                    .iter()
                    .all(|repeat| matches!(*repeat, "repeat" | "no-repeat"))
        }
        "font-size" => single_component_or_calc(&value) && parse_font_size(&value, 16.0).is_some(),
        "font-weight" => {
            matches!(value.as_str(), "normal" | "bold" | "bolder" | "lighter")
                || value.parse::<u16>().is_ok()
        }
        "font-style" => matches!(value.as_str(), "normal" | "italic" | "oblique"),
        "font-family" => super::font_family::parse(&value).is_some(),
        "letter-spacing" | "word-spacing" => {
            single_component_or_calc(&value) && parse_text_spacing(&value, 16.0).is_some()
        }
        "line-height" => {
            single_component_or_calc(&value) && parse_line_height(&value, 16.0).is_some()
        }
        "align-content" => ContentAlignment::parse(&value).is_some(),
        "text-align" => matches!(
            value.as_str(),
            "left" | "start" | "center" | "right" | "end"
        ),
        "white-space" => matches!(value.as_str(), "normal" | "nowrap" | "pre" | "pre-wrap"),
        "text-overflow" => TextOverflow::parse(&value).is_some(),
        "-webkit-line-clamp" => LineClamp::parse(&value).is_some(),
        "-webkit-box-orient" => BoxOrient::parse(&value).is_some(),
        "text-decoration" | "text-decoration-line" => {
            matches!(value.as_str(), "none" | "underline")
        }
        "list-style" | "list-style-type" => matches!(value.as_str(), "none" | "disc"),
        "margin" | "padding" | "border-width" => edge_lengths_supported(&value),
        "scroll-margin"
        | "scroll-margin-top"
        | "scroll-margin-right"
        | "scroll-margin-bottom"
        | "scroll-margin-left"
        | "scroll-padding"
        | "scroll-padding-top"
        | "scroll-padding-right"
        | "scroll-padding-bottom"
        | "scroll-padding-left" => super::scroll_spacing::supports(&property, &value),
        "border-radius" => supported_length(&value),
        "justify-content" | "-webkit-justify-content" | "-webkit-box-pack" => matches!(
            value.as_str(),
            "start"
                | "flex-start"
                | "left"
                | "end"
                | "flex-end"
                | "right"
                | "center"
                | "space-between"
                | "space-around"
                | "space-evenly"
                | "justify"
        ),
        "align-items" | "-webkit-align-items" | "-webkit-box-align" => matches!(
            value.as_str(),
            "stretch" | "start" | "flex-start" | "end" | "flex-end" | "center"
        ),
        "justify-self" => matches!(
            value.as_str(),
            "stretch" | "start" | "flex-start" | "left" | "end" | "flex-end" | "right" | "center"
        ),
        "flex-direction" | "-webkit-flex-direction" | "-moz-flex-direction" => {
            matches!(
                value.as_str(),
                "row" | "row-reverse" | "column" | "column-reverse"
            )
        }
        "flex-wrap" | "-webkit-flex-wrap" | "-moz-flex-wrap" => {
            matches!(value.as_str(), "nowrap" | "wrap")
        }
        "flex-flow" | "-webkit-flex-flow" | "-moz-flex-flow" => flex_flow_supported(&value),
        "flex-grow"
        | "-webkit-flex-grow"
        | "-moz-flex-grow"
        | "-webkit-box-flex"
        | "flex-shrink"
        | "-webkit-flex-shrink"
        | "-moz-flex-shrink" => value
            .parse::<f32>()
            .is_ok_and(|number| number.is_finite() && number >= 0.0),
        _ => false,
    }
}

fn flex_flow_supported(value: &str) -> bool {
    let mut direction = false;
    let mut wrap = false;
    let mut count = 0;
    for token in value.split_ascii_whitespace() {
        count += 1;
        match token {
            "row" | "row-reverse" | "column" | "column-reverse" if !direction => direction = true,
            "nowrap" | "wrap" if !wrap => wrap = true,
            _ => return false,
        }
    }
    (1..=2).contains(&count)
}

fn supported_length(value: &str) -> bool {
    single_component_or_calc(value) && parse_length(value).is_some()
}

/// Verify a simple value's CSS token boundary before handing it to property-specific parsing.
/// `25 %` is two tokens, not a percentage token, even if a string-oriented parser accepts it.
/// `calc()` is validated by its own expression parser instead of this single-token check.
fn single_component_or_calc(value: &str) -> bool {
    if value
        .get(..5)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("calc("))
    {
        return true;
    }
    let mut input = ParserInput::new(value);
    let mut parser = Parser::new(&mut input);
    parser.next().is_ok() && parser.is_exhausted()
}

fn edge_lengths_supported(value: &str) -> bool {
    let lengths = value.split_ascii_whitespace().collect::<Vec<_>>();
    (1..=4).contains(&lengths.len()) && lengths.iter().all(|length| parse_length(length).is_some())
}
