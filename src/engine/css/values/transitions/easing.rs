//! CSS timing functions share typed math with lengths, times and colors.
//! Normalize arguments before handing them to the existing animation sampler;
//! accepting a string which the sampler cannot evaluate would fake support.
use crate::engine::css::{
    syntax::{borrowed_components, function, ident},
    value_parser::numbers,
};
#[cfg(test)]
mod tests;

pub(super) fn normalize(value: &str) -> Option<String> {
    if value.len() > 16_384 {
        return None;
    }
    let value = value.trim().to_ascii_lowercase();
    if let Some(name) = ident(&value)
        && matches!(
            name.as_str(),
            "linear" | "ease" | "ease-in" | "ease-out" | "ease-in-out" | "step-start" | "step-end"
        )
    {
        return Some(name);
    }
    let (name, body) = function(&value)?;
    let tokens = borrowed_components(body)?;
    match name.as_str() {
        "cubic-bezier" => {
            let [a, ",", b, ",", c, ",", d] = tokens.as_slice() else {
                return None;
            };
            let points = [*a, *b, *c, *d].map(numbers::number);
            let [Some(a), Some(b), Some(c), Some(d)] = points else {
                return None;
            };
            if !(0.0..=1.0).contains(&a) || !(0.0..=1.0).contains(&c) {
                return None;
            }
            Some(format!("cubic-bezier({a}, {b}, {c}, {d})"))
        }
        "steps" => {
            let (count, position) = match tokens.as_slice() {
                [count] => (*count, "end".to_owned()),
                [count, ",", position] => (*count, ident(position)?),
                _ => return None,
            };
            // Values 4 §10.12: calculated integers clamp to this argument's
            // positive range; literal zero/negative counts remain invalid.
            let count = numbers::positive_integer(count)?;
            if count < 1
                || position == "jump-none" && count < 2
                || !matches!(
                    position.as_str(),
                    "start" | "end" | "jump-start" | "jump-end" | "jump-none" | "jump-both"
                )
            {
                return None;
            }
            Some(format!("steps({count}, {position})"))
        }
        "linear" => linear(&tokens),
        _ => None,
    }
}

fn linear(tokens: &[&str]) -> Option<String> {
    let stops = tokens.split(|token| *token == ",").collect::<Vec<_>>();
    if !(2..=64).contains(&stops.len()) {
        return None;
    }
    let mut result = Vec::with_capacity(stops.len());
    for stop in stops {
        if !(1..=3).contains(&stop.len()) {
            return None;
        }
        let mut output = None;
        let mut positions = Vec::new();
        for (index, token) in stop.iter().enumerate() {
            if let Some(number) = numbers::number(token) {
                if output.replace(number).is_some() {
                    return None;
                }
                if index != 0 && index != stop.len() - 1 {
                    return None;
                }
            } else if let Some(percent) = numbers::percentage(token) {
                // The && grammar permits the output before or after the
                // contiguous group of one/two input percentages, not between.
                positions.push(percent);
            } else {
                return None;
            }
        }
        let output = output?;
        if positions.len() > 2 {
            return None;
        }
        let positions = positions
            .into_iter()
            .map(|value| format!(" {value}%"))
            .collect::<String>();
        result.push(format!("{output}{positions}"));
    }
    Some(format!("linear({})", result.join(", ")))
}
