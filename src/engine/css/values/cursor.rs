//! The CSS UI 4 `cursor` keyword set, retained as a computed value.
//! https://drafts.csswg.org/css-ui-4/#cursor
//!
//! Image cursors (`url(...) [x y]?, <keyword>`) are not admitted yet; such a
//! declaration is invalid and leaves the cascaded value untouched.

/// Keywords in the CSS UI 4 grammar order. The computed value is the keyword.
const KEYWORDS: [&str; 36] = [
    "auto",
    "default",
    "none",
    "context-menu",
    "help",
    "pointer",
    "progress",
    "wait",
    "cell",
    "crosshair",
    "text",
    "vertical-text",
    "alias",
    "copy",
    "move",
    "no-drop",
    "not-allowed",
    "grab",
    "grabbing",
    "e-resize",
    "n-resize",
    "ne-resize",
    "nw-resize",
    "s-resize",
    "se-resize",
    "sw-resize",
    "w-resize",
    "ew-resize",
    "ns-resize",
    "nesw-resize",
    "nwse-resize",
    "col-resize",
    "row-resize",
    "all-scroll",
    "zoom-in",
    "zoom-out",
];

/// An index into [`KEYWORDS`]; `auto` is the initial value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cursor(u8);

impl Cursor {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        KEYWORDS
            .iter()
            .position(|keyword| keyword.eq_ignore_ascii_case(value))
            .map(|index| Self(index as u8))
    }

    pub(crate) fn css_text(self) -> &'static str {
        KEYWORDS[usize::from(self.0)]
    }
}

#[cfg(test)]
mod tests {
    use super::Cursor;

    #[test]
    fn cursor_keywords_compute_to_themselves_and_unknown_keywords_are_invalid() {
        assert_eq!(Cursor::default().css_text(), "auto");
        for keyword in ["auto", "nesw-resize", "all-scroll", "zoom-out"] {
            assert_eq!(Cursor::parse(keyword).map(Cursor::css_text), Some(keyword));
        }
        assert_eq!(
            Cursor::parse("POINTER").map(Cursor::css_text),
            Some("pointer")
        );
        assert_eq!(Cursor::parse("bogus"), None);
        assert_eq!(Cursor::parse("hand"), None);
    }
}
