//! Parse one animation shorthand item without treating quoted names as keywords.
use super::*;

pub(super) fn parse(value: &str) -> Option<AnimationSettings> {
    let mut result = AnimationSettings {
        name_scope: None,
        names: Vec::new(),
        durations: Vec::new(),
        delays: Vec::new(),
        easings: Vec::new(),
        iterations: Vec::new(),
        directions: Vec::new(),
        fills: Vec::new(),
        states: Vec::new(),
    };
    for part in split_css_top_level(value, ',') {
        if result.names.len() == 64 {
            return None;
        }
        let tokens = components(part)?;
        if tokens.is_empty() || tokens.len() > 8 {
            return None;
        }
        let (mut name, mut duration, mut delay, mut timing) = (None, None, None, None);
        let (mut repeats, mut direction, mut fill, mut state) = (None, None, None, None);
        for token in tokens {
            let lower = token.to_ascii_lowercase();
            if let Some(seconds) = time(&lower, true) {
                if duration.is_none() {
                    if seconds < 0.0 {
                        return None;
                    }
                    duration = Some(seconds);
                } else if delay.is_none() {
                    delay = Some(seconds);
                } else {
                    return None;
                }
            } else if timing.is_none() && easing(&lower).is_some() {
                timing = Some(lower);
            } else if repeats.is_none() && iteration(&lower).is_some() {
                repeats = iteration(&lower);
            } else if direction.is_none()
                && matches!(
                    lower.as_str(),
                    "normal" | "reverse" | "alternate" | "alternate-reverse"
                )
            {
                direction = Some(lower);
            } else if fill.is_none()
                && matches!(lower.as_str(), "none" | "forwards" | "backwards" | "both")
            {
                fill = Some(lower);
            } else if state.is_none() && matches!(lower.as_str(), "running" | "paused") {
                state = Some(lower);
            } else if name.is_none() {
                name = Some(AnimationName::parse(token)?);
            } else {
                return None;
            }
        }
        result.names.push(name.unwrap_or(AnimationName::None));
        result.durations.push(duration.unwrap_or(0.0));
        result.delays.push(delay.unwrap_or(0.0));
        result.easings.push(timing.unwrap_or_else(|| "ease".into()));
        result.iterations.push(repeats.unwrap_or(1.0));
        result
            .directions
            .push(direction.unwrap_or_else(|| "normal".into()));
        result.fills.push(fill.unwrap_or_else(|| "none".into()));
        result
            .states
            .push(state.unwrap_or_else(|| "running".into()));
    }
    (!result.names.is_empty()).then_some(result)
}

// Unlike transition-property, an animation name can be a string containing spaces.
// Use the existing CSS tokenizer rather than a second ad-hoc string/escape parser.
fn components(value: &str) -> Option<Vec<&str>> {
    let mut input = ParserInput::new(value);
    let mut parser = Parser::new(&mut input);
    let mut result = Vec::new();
    while !parser.is_exhausted() {
        parser.skip_whitespace();
        let start = parser.position();
        let token = parser.next().ok()?.clone();
        if matches!(
            token,
            Token::Function(_) | Token::ParenthesisBlock | Token::SquareBracketBlock
        ) {
            parser.parse_nested_block(|inner| consume(inner, 0)).ok()?;
        }
        result.push(parser.slice_from(start));
        if result.len() > 8 {
            return None;
        }
    }
    Some(result)
}

fn consume<'i, 't>(
    parser: &mut cssparser::Parser<'i, 't>,
    depth: usize,
) -> Result<(), cssparser::ParseError<'i, ()>> {
    if depth >= 16 {
        return Err(parser.new_custom_error(()));
    }
    while !parser.is_exhausted() {
        let token = parser.next()?.clone();
        if matches!(
            token,
            Token::Function(_) | Token::ParenthesisBlock | Token::SquareBracketBlock
        ) {
            parser.parse_nested_block(|inner| consume(inner, depth + 1))?;
        }
    }
    Ok(())
}
