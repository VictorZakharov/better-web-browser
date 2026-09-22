//! CSS Fonts 4 weight descriptor matching for downloadable faces.
//! https://drafts.csswg.org/css-fonts-4/#font-matching-algorithm

use super::WebFontFace;

impl WebFontFace {
    /// Lower keys win. An inclusive range containing the requested weight wins
    /// before the directional fallback search, even when another face has a
    /// numerically closer endpoint.
    pub(crate) fn weight_match_rank(&self, requested: u16) -> (u8, f32) {
        let requested = f32::from(requested);
        if (self.weight_min..=self.weight_max).contains(&requested) {
            return (0, 0.0);
        }
        if requested < 400.0 {
            if self.weight_max < requested {
                (1, requested - self.weight_max)
            } else {
                (2, self.weight_min - requested)
            }
        } else if requested <= 500.0 {
            if self.weight_min > requested && self.weight_min <= 500.0 {
                (1, self.weight_min - requested)
            } else if self.weight_max < requested {
                (2, requested - self.weight_max)
            } else {
                (3, self.weight_min - 500.0)
            }
        } else if self.weight_min > requested {
            (1, self.weight_min - requested)
        } else {
            (2, requested - self.weight_max)
        }
    }

    /// Fontique registers one weight per resource. Keep the chosen face's
    /// metadata at the requested weight when it lies inside the CSS range.
    pub(crate) fn registered_weight(&self, requested: u16) -> u16 {
        let requested_float = f32::from(requested);
        if requested_float < self.weight_min {
            self.weight_min.round() as u16
        } else if requested_float > self.weight_max {
            self.weight_max.round() as u16
        } else {
            requested
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::font::{discover_font_faces, parse_font_weight};

    fn face(min: f32, max: f32) -> WebFontFace {
        WebFontFace {
            family: "Fixture".into(),
            weight: min.round() as u16,
            weight_min: min,
            weight_max: max,
            italic: false,
            url: "https://example.test/font.woff".into(),
        }
    }

    #[test]
    fn descriptor_accepts_inclusive_and_fractional_ranges() {
        assert_eq!(parse_font_weight("300 400"), Some((300.0, 400.0)));
        assert_eq!(parse_font_weight("700 530.5"), Some((530.5, 700.0)));
        assert_eq!(parse_font_weight("normal"), Some((400.0, 400.0)));
        for invalid in ["0", "1001", "-1 400", "300 400 500", "NaN", "bold 700"] {
            assert_eq!(parse_font_weight(invalid), None, "{invalid}");
        }
    }

    #[test]
    fn discovers_ranges_without_copying_a_site_stylesheet() {
        let faces = discover_font_faces(
            "@font-face{font-family:Fixture;font-weight:300 400;src:url(regular.woff)}\
             @font-face{font-family:Fixture;font-weight:430;src:url(medium.woff)}",
            "https://example.test/style.css",
        );
        assert_eq!(faces.len(), 2);
        assert_eq!((faces[0].weight_min, faces[0].weight_max), (300.0, 400.0));
        assert!(faces[0].weight_match_rank(400) < faces[1].weight_match_rank(400));
        assert_eq!(faces[0].registered_weight(400), 400);
    }

    #[test]
    fn directional_fallback_follows_css_fonts_four() {
        // For 400..=500, inspect heavier faces through 500, then lighter,
        // then faces above 500. Outside that band, prefer the target direction.
        assert!(
            face(500.0, 500.0).weight_match_rank(400) < face(399.0, 399.0).weight_match_rank(400)
        );
        assert!(
            face(399.0, 399.0).weight_match_rank(400) < face(501.0, 501.0).weight_match_rank(400)
        );
        assert!(
            face(200.0, 200.0).weight_match_rank(300) < face(350.0, 350.0).weight_match_rank(300)
        );
        assert!(
            face(900.0, 900.0).weight_match_rank(600) < face(500.0, 500.0).weight_match_rank(600)
        );
    }
}
