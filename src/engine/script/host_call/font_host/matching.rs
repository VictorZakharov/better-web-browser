//! FontFaceSet queries reuse CSS family/shorthand parsing and the downloadable
//! face weight ordering. They do not split quoted commas or accept arbitrary
//! strings merely because a regular expression finds a size-looking substring.

use crate::engine::css::font_family::{self, Family};
use crate::engine::font::WebFontFace;
use crate::engine::font::unicode_ranges::UnicodeRanges;
use crate::engine::script::{JsNativeError, JsResult, JsValue};

pub(super) fn family(value: &str) -> Option<String> {
    let mut families = font_family::parse(value)?;
    if families.len() != 1 {
        return None;
    }
    match families.remove(0) {
        Family::Named(name) => Some(name),
        Family::Generic(_) => None,
    }
}

pub(super) fn dispatch(operation: &str, args: &[JsValue]) -> JsResult<Option<JsValue>> {
    if operation == "fontFaceFeaturesSerialize" {
        let value = args.get(1).map(JsValue::string_value).unwrap_or_default();
        return Ok(Some(
            crate::engine::css::FontFeatures::specified_css_text(&value)
                .map_or(JsValue::Null, JsValue::from),
        ));
    }
    if operation == "fontFaceSourceURLs" {
        let value = args.get(1).map(JsValue::string_value).unwrap_or_default();
        return Ok(Some(crate::engine::font::sources::parse(&value).map_or(
            JsValue::Null,
            |sources| {
                JsValue::Array(
                    sources
                        .into_iter()
                        .filter_map(|source| match source {
                            crate::engine::font::sources::FontSource::Url(url) => {
                                Some(JsValue::from(url))
                            }
                            crate::engine::font::sources::FontSource::Local(_) => None,
                        })
                        .collect(),
                )
            },
        )));
    }
    if operation == "fontFaceRangeSerialize" {
        let value = args.get(1).map(JsValue::string_value).unwrap_or_default();
        return Ok(Some(
            crate::engine::font::unicode_ranges::UnicodeRanges::parse(&value)
                .map_or(JsValue::Null, |ranges| JsValue::from(ranges.serialize())),
        ));
    }
    if operation == "fontFaceSerializeFamily" {
        let value = args.get(1).map(JsValue::string_value).unwrap_or_default();
        if value.len() > 1024 {
            return Ok(Some(JsValue::Null));
        }
        if family(&value).as_deref() == Some(value.as_str()) {
            return Ok(Some(JsValue::from(value)));
        }
        let mut serialized = String::new();
        cssparser::serialize_string(&value, &mut serialized).expect("writing a String cannot fail");
        return Ok(Some(JsValue::from(serialized)));
    }
    if operation == "fontFaceFamilyValid" {
        let value = args.get(1).map(JsValue::string_value).unwrap_or_default();
        return Ok(Some(JsValue::Boolean(
            value.len() <= 1024 && family(&value).is_some(),
        )));
    }
    if operation != "fontFaceMatch" {
        return Ok(None);
    }
    let value = args.get(1).map(JsValue::string_value).unwrap_or_default();
    let Some(JsValue::Array(faces)) = args.get(2) else {
        return Ok(Some(JsValue::Null));
    };
    if value.len() > 8192 || faces.len() > 4096 {
        return Err(JsNativeError::range()
            .with_message("FontFaceSet query exceeds the matching budget")
            .into());
    }
    let Some(spec) = font_family::parse_canvas_font(&value) else {
        return Ok(Some(JsValue::Null));
    };
    let Some(families) = font_family::parse(&spec.family) else {
        return Ok(Some(JsValue::Null));
    };
    let Some(faces) = faces.iter().map(face).collect::<Option<Vec<_>>>() else {
        return Ok(Some(JsValue::Null));
    };
    let text = args
        .get(3)
        .map(JsValue::string_value)
        .unwrap_or_else(|| " ".into());
    let mut matches = Vec::new();
    for requested in families {
        let Family::Named(requested) = requested else {
            continue;
        };
        let ranked = faces
            .iter()
            .enumerate()
            .filter(|(_, face)| face.face.family.eq_ignore_ascii_case(&requested))
            .map(|(index, face)| {
                let weight = face.face.weight_match_rank(spec.weight);
                (
                    index,
                    (
                        u8::from(face.face.italic != spec.italic),
                        weight.0,
                        weight.1,
                    ),
                )
            })
            .collect::<Vec<_>>();
        let best = ranked
            .iter()
            .map(|(_, rank)| *rank)
            .min_by(|a, b| a.partial_cmp(b).unwrap());
        for (index, rank) in ranked {
            if Some(rank) == best
                && faces[index].ranges.intersects(&text)
                && !matches.contains(&index)
            {
                matches.push(index);
            }
        }
    }
    Ok(Some(JsValue::Array(
        matches
            .into_iter()
            .map(|index| JsValue::from(index as u32))
            .collect(),
    )))
}

struct MatchingFace {
    face: WebFontFace,
    ranges: UnicodeRanges,
}

fn face(value: &JsValue) -> Option<MatchingFace> {
    let JsValue::Array(parts) = value else {
        return None;
    };
    if parts.len() != 4 {
        return None;
    }
    let family = family(&parts[0].string_value())?;
    let weight = parts[1].as_number()?;
    let italic = parts[2].as_boolean()?;
    if !weight.is_finite() || weight.fract() != 0.0 || !(1.0..=1000.0).contains(&weight) {
        return None;
    }
    Some(MatchingFace {
        face: WebFontFace {
            family,
            weight: weight as u16,
            weight_min: weight as f32,
            weight_max: weight as f32,
            features: Default::default(),
            italic,
            url: String::new(),
            fallback_urls: Vec::new(),
            unicode_range: parts[3].string_value(),
        },
        ranges: UnicodeRanges::parse(&parts[3].string_value())?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn candidate(family: &str, weight: u16, italic: bool) -> JsValue {
        JsValue::Array(vec![
            JsValue::from(family.to_owned()),
            JsValue::from(u32::from(weight)),
            JsValue::from(italic),
            JsValue::from("U+0-10FFFF".to_owned()),
        ])
    }
    fn query(font: &str, faces: Vec<JsValue>) -> JsValue {
        dispatch(
            "fontFaceMatch",
            &[
                JsValue::Null,
                JsValue::from(font.to_owned()),
                JsValue::Array(faces),
            ],
        )
        .unwrap()
        .unwrap()
    }
    #[test]
    fn quoted_names_escapes_and_all_families_are_shared_with_css_parser() {
        assert_eq!(family("'Some, Family'"), Some("Some, Family".into()));
        assert_eq!(family("Some Font"), Some("Some Font".into()));
        for invalid in ["", "Arial, serif", "serif", "inherit", "12px"] {
            assert!(family(invalid).is_none());
        }
        assert_eq!(
            query(
                "italic 700 16px 'Some, Family', Missing, Second",
                vec![
                    candidate("'Some, Family'", 400, false),
                    candidate("'Some, Family'", 700, true),
                    candidate("Second", 400, false)
                ]
            ),
            JsValue::Array(vec![JsValue::from(1), JsValue::from(2)])
        );
    }
    #[test]
    fn css_four_hundred_to_five_hundred_weight_order_is_not_nearest_distance() {
        assert_eq!(
            query(
                "400 16px Example",
                vec![
                    candidate("Example", 300, false),
                    candidate("Example", 500, false),
                    candidate("Example", 500, false)
                ]
            ),
            JsValue::Array(vec![JsValue::from(1), JsValue::from(2)])
        );
        assert_eq!(
            query(
                "600 16px Example",
                vec![
                    candidate("Example", 500, false),
                    candidate("Example", 700, false)
                ]
            ),
            JsValue::Array(vec![JsValue::from(1)])
        );
    }
    #[test]
    fn invalid_shorthand_is_not_mistaken_for_a_family_and_generics_do_not_load_faces() {
        for font in [
            "",
            "inherit",
            "16px",
            "var(--font)",
            "url(test)",
            "unknown 16px Example",
        ] {
            assert_eq!(query(font, vec![]), JsValue::Null, "{font}");
        }
        assert_eq!(
            query("16px serif", vec![candidate("'serif'", 400, false)]),
            JsValue::Array(vec![])
        );
        assert_eq!(
            query("16px 'serif'", vec![candidate("'serif'", 400, false)]),
            JsValue::Array(vec![JsValue::from(0)])
        );
    }
}
