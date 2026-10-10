//! UTS #51: symbol blocks are not a substitute for Emoji_Presentation data.
//! https://www.unicode.org/reports/tr51/#Presentation_Style
use icu_properties::{CodePointSetData, props::EmojiPresentation};

pub(super) fn prefers_emoji(cluster: &str) -> bool {
    if cluster.contains('\u{fe0e}') {
        return false;
    }
    cluster.contains('\u{fe0f}')
        || cluster
            .chars()
            .any(|ch| CodePointSetData::new::<EmojiPresentation>().contains(ch))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn text_symbols_and_variation_selectors_keep_their_presentation_contract() {
        for text in ["⚔", "✦", "⟳", "⛁", "♟", "A", "🌠\u{fe0e}"] {
            assert!(!prefers_emoji(text), "{text}");
        }
        for text in ["🌠", "😀", "⚔\u{fe0f}", "1\u{fe0f}\u{20e3}"] {
            assert!(prefers_emoji(text), "{text}");
        }
    }
}
