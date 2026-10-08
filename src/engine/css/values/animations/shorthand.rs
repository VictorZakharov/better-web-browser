//! Parse one animation shorthand item without treating quoted names as keywords.
use super::super::calculated_times::TimeValue;
use super::*;

pub(super) fn parse(value: &str) -> Option<AnimationSettings> {
    let mut result = AnimationSettings {
        calculated_times: CalculatedTimes::default(),
        calculated_easings: Vec::new(),
        calculated_iterations: Vec::new(),
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
            if let Some(seconds) = TimeValue::parse(&lower, true) {
                if duration.is_none() {
                    duration = Some(TimeValue::parse(&lower, false)?);
                } else if delay.is_none() {
                    delay = Some(seconds);
                } else {
                    return None;
                }
            } else if timing.is_none() && EasingValue::parse(&lower).is_some() {
                timing = EasingValue::parse(&lower);
            } else if repeats.is_none() && IterationValue::parse(&lower).is_some() {
                repeats = IterationValue::parse(&lower);
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
        duration.unwrap_or(TimeValue::Fixed(0.0)).append(
            &mut result.durations,
            &mut result.calculated_times.durations,
        );
        delay
            .unwrap_or(TimeValue::Fixed(0.0))
            .append(&mut result.delays, &mut result.calculated_times.delays);
        timing
            .unwrap_or_else(|| EasingValue::Fixed("ease".into()))
            .append(&mut result.easings, &mut result.calculated_easings);
        repeats
            .unwrap_or(IterationValue::Number(NumberValue::Fixed(1.0)))
            .append(&mut result.iterations, &mut result.calculated_iterations);
        result
            .directions
            .push(direction.unwrap_or_else(|| "normal".into()));
        result.fills.push(fill.unwrap_or_else(|| "none".into()));
        result
            .states
            .push(state.unwrap_or_else(|| "running".into()));
    }
    result.calculated_times.compact();
    calculated_easing::compact(&mut result.calculated_easings);
    iterations::compact(&mut result.calculated_iterations);
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
