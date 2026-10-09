//! Closed numeric records, fully admitted before any native command executes.
//! The operation table is shared with the trusted Window/Worker bootstrap.
use super::{MAX_NUMERIC_VALUES, NumericCommand};
use std::sync::OnceLock;

pub(crate) const MAX_PACKET_COMMANDS: usize = 128;
const HEADER_VALUES: usize = 4;
pub(crate) const MAX_PACKET_VALUES: usize =
    MAX_PACKET_COMMANDS * (HEADER_VALUES + MAX_NUMERIC_VALUES);

pub(crate) fn operations() -> &'static [String] {
    static TABLE: OnceLock<Vec<String>> = OnceLock::new();
    TABLE.get_or_init(|| {
        serde_json::from_str(include_str!("numeric_packet/operations.json"))
            .expect("checked-in numeric operation table is valid JSON")
    })
}

fn unsigned(value: f64, maximum: u32) -> Option<u32> {
    (value.is_finite() && value >= 0. && value <= maximum as f64 && value.fract() == 0.)
        .then_some(value as u32)
}

pub(crate) fn decode(values: &[f64]) -> Option<Vec<(u32, NumericCommand)>> {
    if values.is_empty() || values.len() > MAX_PACKET_VALUES {
        return None;
    }
    let mut entries = Vec::new();
    let mut cursor = 0;
    while cursor < values.len() {
        if entries.len() >= MAX_PACKET_COMMANDS {
            return None;
        }
        let header = values.get(cursor..cursor.checked_add(HEADER_VALUES)?)?;
        let operation = unsigned(header[0], u32::MAX)? as usize;
        let id = unsigned(header[1], u32::MAX)?;
        if id == 0 {
            return None;
        }
        let integer_count = unsigned(header[2], MAX_NUMERIC_VALUES as u32)? as usize;
        let float_count = unsigned(header[3], MAX_NUMERIC_VALUES as u32)? as usize;
        let count = integer_count.checked_add(float_count)?;
        if count > MAX_NUMERIC_VALUES {
            return None;
        }
        cursor += HEADER_VALUES;
        let arguments = values.get(cursor..cursor.checked_add(count)?)?;
        let mut integers = Vec::with_capacity(integer_count);
        for &value in &arguments[..integer_count] {
            if !value.is_finite() || value.abs() > 9_007_199_254_740_991. || value.fract() != 0. {
                return None;
            }
            integers.push(value as i64);
        }
        let command = NumericCommand::new(
            operations().get(operation)?.clone(),
            integers,
            arguments[integer_count..].to_vec(),
        )?;
        entries.push((id, command));
        cursor += count;
    }
    Some(entries)
}

#[cfg(test)]
mod tests;
