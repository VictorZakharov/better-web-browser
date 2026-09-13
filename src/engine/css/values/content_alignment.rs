//! Positional content alignment and the single-subject distribution fallbacks.
//! https://www.w3.org/TR/css-align-3/#distribution-block
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ContentAlignment {
    keyword: Keyword,
    overflow: Overflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Keyword {
    #[default]
    Normal,
    Start,
    End,
    Center,
    FlexStart,
    FlexEnd,
    Stretch,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Overflow {
    #[default]
    Default,
    Safe,
    Unsafe,
}

impl ContentAlignment {
    pub(crate) const CENTER: Self = Self {
        keyword: Keyword::Center,
        overflow: Overflow::Default,
    };

    pub(crate) fn parse(value: &str) -> Option<Self> {
        let value = value.to_ascii_lowercase();
        let mut tokens = value.split_ascii_whitespace();
        let first = tokens.next()?;
        let (overflow, keyword) = match first {
            "safe" => (Overflow::Safe, tokens.next()?),
            "unsafe" => (Overflow::Unsafe, tokens.next()?),
            _ => (Overflow::Default, first),
        };
        if tokens.next().is_some() {
            return None;
        }
        let keyword = match keyword {
            "normal" => Keyword::Normal,
            "start" => Keyword::Start,
            "end" => Keyword::End,
            "center" => Keyword::Center,
            "flex-start" => Keyword::FlexStart,
            "flex-end" => Keyword::FlexEnd,
            "stretch" => Keyword::Stretch,
            "space-between" => Keyword::SpaceBetween,
            "space-around" => Keyword::SpaceAround,
            "space-evenly" => Keyword::SpaceEvenly,
            _ => return None,
        };
        if overflow != Overflow::Default
            && !matches!(
                keyword,
                Keyword::Start
                    | Keyword::End
                    | Keyword::Center
                    | Keyword::FlexStart
                    | Keyword::FlexEnd
            )
        {
            return None;
        }
        Some(Self { keyword, overflow })
    }

    pub(crate) fn is_normal(self) -> bool {
        self.keyword == Keyword::Normal
    }

    pub(crate) fn css_text(self) -> String {
        let prefix = match self.overflow {
            Overflow::Default => "",
            Overflow::Safe => "safe ",
            Overflow::Unsafe => "unsafe ",
        };
        let keyword = match self.keyword {
            Keyword::Normal => "normal",
            Keyword::Start => "start",
            Keyword::End => "end",
            Keyword::Center => "center",
            Keyword::FlexStart => "flex-start",
            Keyword::FlexEnd => "flex-end",
            Keyword::Stretch => "stretch",
            Keyword::SpaceBetween => "space-between",
            Keyword::SpaceAround => "space-around",
            Keyword::SpaceEvenly => "space-evenly",
        };
        format!("{prefix}{keyword}")
    }

    pub(crate) fn block_offset(self, free_space: f32) -> f32 {
        // CSS Align permits safe overflow for implementations without automatic
        // scroll-safety adjustment. Distributed values use their safe fallback.
        let free = if self.overflow == Overflow::Unsafe {
            free_space
        } else {
            free_space.max(0.0)
        };
        match self.keyword {
            Keyword::End | Keyword::FlexEnd => free,
            Keyword::Center | Keyword::SpaceAround | Keyword::SpaceEvenly => free * 0.5,
            _ => 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn positional_grammar_and_single_subject_fallbacks() {
        for value in [
            "normal",
            "start",
            "end",
            "center",
            "safe center",
            "unsafe end",
            "stretch",
            "space-around",
            "space-evenly",
            "flex-start",
            "flex-end",
            "space-between",
        ] {
            assert_eq!(ContentAlignment::parse(value).unwrap().css_text(), value);
        }
        for value in [
            "safe stretch",
            "unsafe normal",
            "safe space-around",
            "left",
            "center end",
            "safe",
            "",
        ] {
            assert!(ContentAlignment::parse(value).is_none(), "{value}");
        }
        assert_eq!(ContentAlignment::CENTER.block_offset(-20.0), 0.0);
        assert_eq!(
            ContentAlignment::parse("unsafe center")
                .unwrap()
                .block_offset(-20.0),
            -10.0
        );
        assert_eq!(
            ContentAlignment::parse("space-evenly")
                .unwrap()
                .block_offset(40.0),
            20.0
        );
    }

    #[test]
    fn computed_value_and_layout_invalidation_follow_the_cascade() {
        use crate::engine::css::{StyleSet, resolved_property_value, supports::supports_matches};
        use crate::engine::dom;
        let dom = dom::parse(
            "<style>div{align-content:end}span{align-content:inherit}p{align-content:center;align-content:safe stretch}</style><div><span></span><p></p><section></section></div>",
        );
        let styles = StyleSet::from_dom(&dom, &[], 500.0);
        let style = |tag| styles.get(&dom.elements_named(tag).next().unwrap()).clone();
        assert_eq!(
            resolved_property_value(&style("span"), "align-content").as_deref(),
            Some("end")
        );
        assert_eq!(
            resolved_property_value(&style("p"), "align-content").as_deref(),
            Some("center")
        );
        assert!(
            style("section").align_content.is_normal(),
            "not inherited by default"
        );
        let original = style("section");
        let mut changed = original.clone();
        changed.align_content = ContentAlignment::CENTER;
        assert!(!original.layout_equivalent(&changed));
        assert!(supports_matches("(align-content: safe center)"));
        assert!(!supports_matches("(align-content: safe stretch)"));
    }
}
