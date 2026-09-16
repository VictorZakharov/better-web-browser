//! Bound expendable diagnostic output before the strict IPC encoder sees it.
//! Operational updates (cookies, storage, history, and input) never use this path.

use crate::limits::{MAX_RUNTIME_REPORT_ENTRIES, MAX_RUNTIME_REPORT_TEXT_BYTES};

pub(super) fn bounded(values: Vec<String>, lane: &str) -> Vec<String> {
    // Use the existing per-entry text limit as a stricter aggregate lane budget.
    // This prevents hundreds of individually legal strings from inflating a report.
    let bytes = values
        .iter()
        .fold(0_usize, |total, value| total.saturating_add(value.len()));
    if values.len() <= MAX_RUNTIME_REPORT_ENTRIES && bytes <= MAX_RUNTIME_REPORT_TEXT_BYTES {
        return values;
    }
    const NOTICE_RESERVE: usize = 128;
    let count = values.len();
    let mut remaining = MAX_RUNTIME_REPORT_TEXT_BYTES - NOTICE_RESERVE;
    let mut omitted = count;
    let mut truncated_bytes = 0;
    let mut result = Vec::with_capacity(count.min(MAX_RUNTIME_REPORT_ENTRIES));
    for mut value in values {
        if result.len() == MAX_RUNTIME_REPORT_ENTRIES - 1 || remaining == 0 {
            break;
        }
        if value.len() > remaining {
            let mut boundary = remaining;
            while !value.is_char_boundary(boundary) {
                boundary -= 1;
            }
            truncated_bytes += value.len() - boundary;
            value.truncate(boundary);
        }
        remaining -= value.len();
        result.push(value);
        omitted -= 1;
    }
    let notice = format!(
        "{lane} output truncated: {omitted} entries omitted; {truncated_bytes} bytes truncated"
    );
    debug_assert!(notice.len() <= NOTICE_RESERVE);
    result.push(notice);
    result
}
