//! Temporal input parsing for range-limitation checks.
//!
//! Date, month, week, time, and datetime-local values compare against `min`
//! and `max` as chronological ranks. Grammars follow the HTML date/time
//! string rules; out-of-grammar values and bounds parse to nothing and
//! therefore suffer no range violation.

/// Chronological rank; only same-state ranks are ever compared, so the two
/// components act as a major/minor pair (days plus millis, year plus
/// sub-year, or a single populated component).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(crate) struct TemporalRank(i64, i64);

/// Step-arithmetic rank in the state's unit, anchored so the default base
/// is zero: days for date, months since 1970-01 for month, weeks since the
/// Monday of 1970-W01 for week, millis since midnight for time, and millis
/// since the epoch for datetime-local.
pub(crate) fn step_rank(state: &str, value: &str) -> Option<i64> {
    match state {
        "date" => parse_date(value),
        "month" => parse_month(value)
            .and_then(|(year, month)| (year - 1970).checked_mul(12)?.checked_add(month - 1)),
        "week" => parse_week(value).map(|(year, week)| monday_ordinal(year, week)),
        "time" => parse_time(value),
        "datetime-local" => parse_datetime_local(value)
            .and_then(|(days, millis)| days.checked_mul(86_400_000)?.checked_add(millis)),
        _ => None,
    }
}

/// Default step-arithmetic base rank (zero everywhere except week, whose
/// base Monday 1969-12-29 predates the epoch Thursday).
pub(crate) fn default_step_rank(state: &str) -> i64 {
    if state == "week" {
        monday_ordinal(1970, 1)
    } else {
        0
    }
}

/// Weeks since the Monday of 1970-W01. 4 January always falls in week one;
/// backing up to its Monday keeps every week on the same grid.
fn monday_ordinal(year: i64, week: i64) -> i64 {
    let jan4 = days_from_civil(year, 1, 4);
    let monday_week_one = jan4 - (jan4 + 3).rem_euclid(7);
    (monday_week_one + (week - 1) * 7).div_euclid(7)
}

/// Underflow/overflow for a temporal value against authored bounds, or
/// `None` when the value is empty or out-of-grammar (it suffers nothing).
/// Out-of-grammar bounds are ignored. Time ranges wrap: when the maximum
/// is below the minimum the gap between them is out-of-range and
/// everything else is in-range (spec reversed range); every other state
/// compares linearly even when max < min.
pub(crate) fn temporal_underflow_overflow(
    state: &str,
    value: &str,
    min_attr: &Option<String>,
    max_attr: &Option<String>,
) -> Option<(bool, bool)> {
    if value.is_empty() {
        return None;
    }
    let actual = parse_temporal(state, value)?;
    let minimum = min_attr
        .as_deref()
        .and_then(|bound| parse_temporal(state, bound));
    let maximum = max_attr
        .as_deref()
        .and_then(|bound| parse_temporal(state, bound));
    if state == "time"
        && let (Some(minimum), Some(maximum)) = (minimum, maximum)
        && maximum < minimum
    {
        let in_gap = actual > maximum && actual < minimum;
        return Some((in_gap, in_gap));
    }
    Some((
        minimum.is_some_and(|minimum| actual < minimum),
        maximum.is_some_and(|maximum| actual > maximum),
    ))
}

/// True for the five states whose values compare as chronological ranks.
pub(crate) fn is_temporal_state(state: &str) -> bool {
    matches!(state, "date" | "month" | "week" | "time" | "datetime-local")
}

/// Parses a state value into its chronological rank.
pub(crate) fn parse_temporal(state: &str, value: &str) -> Option<TemporalRank> {
    match state {
        "date" => parse_date(value).map(|days| TemporalRank(days, 0)),
        "month" => parse_month(value).map(|(year, month)| TemporalRank(year, month)),
        "week" => parse_week(value).map(|(year, week)| TemporalRank(year, week)),
        "time" => parse_time(value).map(|millis| TemporalRank(0, millis)),
        "datetime-local" => {
            parse_datetime_local(value).map(|(days, millis)| TemporalRank(days, millis))
        }
        _ => None,
    }
}

/// Nonempty ASCII digits as an integer.
fn digits(value: &str) -> Option<i64> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}

/// Four or more digits with a value above zero.
fn parse_year(value: &str) -> Option<i64> {
    if value.len() < 4 {
        return None;
    }
    let year = digits(value)?;
    // The native rank uses signed 64-bit civil days. Reject years outside
    // that representation before any multiplication in calendar arithmetic.
    if !(1..=i64::MAX / 366).contains(&year) {
        None
    } else {
        Some(year)
    }
}

fn is_leap_year(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        _ => 28,
    }
}

/// Days since 1970-01-01 (Howard Hinnant's civil-days algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let shifted = if month <= 2 { year - 1 } else { year };
    let era = shifted.div_euclid(400);
    let year_of_era = shifted - era * 400;
    let month_index = (month + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146097 + day_of_era - 719468
}

/// Valid `YYYY-MM-DD` (year of four or more digits) as days since the epoch.
fn parse_date(value: &str) -> Option<i64> {
    let (year_text, rest) = value.split_once('-')?;
    let (month_text, day_text) = rest.split_once('-')?;
    if month_text.len() != 2 || day_text.len() != 2 {
        return None;
    }
    let year = parse_year(year_text)?;
    let month = digits(month_text)?;
    let day = digits(day_text)?;
    if !(1..=12).contains(&month) || !(1..=days_in_month(year, month)).contains(&day) {
        return None;
    }
    Some(days_from_civil(year, month, day))
}

/// Valid `YYYY-MM` as a (year, month) pair.
fn parse_month(value: &str) -> Option<(i64, i64)> {
    let (year_text, month_text) = value.split_once('-')?;
    if month_text.len() != 2 {
        return None;
    }
    let year = parse_year(year_text)?;
    let month = digits(month_text)?;
    if !(1..=12).contains(&month) {
        return None;
    }
    Some((year, month))
}

/// Monday-is-zero weekday of 1 January (1970-01-01 was a Thursday).
fn jan1_weekday(year: i64) -> i64 {
    (days_from_civil(year, 1, 1) + 3).rem_euclid(7)
}

/// ISO week count: 53 when 1 January is a Thursday, or a Wednesday in a
/// leap year; 52 otherwise.
fn weeks_in_year(year: i64) -> i64 {
    let jan1 = jan1_weekday(year);
    if jan1 == 3 || (is_leap_year(year) && jan1 == 2) {
        53
    } else {
        52
    }
}

/// Valid `YYYY-Www` as a (year, week) pair; chronological order matches
/// lexicographic order of the pair.
fn parse_week(value: &str) -> Option<(i64, i64)> {
    let (year_text, week_text) = value.split_once("-W")?;
    if week_text.len() != 2 {
        return None;
    }
    let year = parse_year(year_text)?;
    let week = digits(week_text)?;
    if week < 1 || week > weeks_in_year(year) {
        return None;
    }
    Some((year, week))
}

/// Valid `HH:MM` with optional `:SS` and optional fractional seconds, as
/// millis since midnight. Hours, minutes, and seconds are two digits.
fn parse_time(value: &str) -> Option<i64> {
    let (hour_text, rest) = value.split_once(':')?;
    if hour_text.len() != 2 {
        return None;
    }
    let hour = digits(hour_text)?;
    if hour > 23 {
        return None;
    }
    let (minute_text, second_text) = match rest.split_once(':') {
        Some((minute, second)) => (minute, Some(second)),
        None => (rest, None),
    };
    if minute_text.len() != 2 {
        return None;
    }
    let minute = digits(minute_text)?;
    if minute > 59 {
        return None;
    }
    let (second, millis) = match second_text {
        None => (0, 0),
        Some(text) => {
            let (second_text, fraction) = match text.split_once('.') {
                Some((second, fraction)) if !fraction.is_empty() => (second, fraction),
                Some(_) => return None,
                None => (text, ""),
            };
            if second_text.len() != 2 {
                return None;
            }
            let second = digits(second_text)?;
            if second > 59 {
                return None;
            }
            if fraction.len() > 3 || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            let mut padded = fraction.to_string();
            while padded.len() < 3 {
                padded.push('0');
            }
            (second, padded.parse().ok()?)
        }
    };
    Some(((hour * 60 + minute) * 60 + second) * 1000 + millis)
}

/// Valid datetime-local (`date` plus `T`/space plus `time`) as days and
/// millis components.
fn parse_datetime_local(value: &str) -> Option<(i64, i64)> {
    let separator = value.find(['T', ' '])?;
    let days = parse_date(&value[..separator])?;
    let millis = parse_time(&value[separator + 1..])?;
    Some((days, millis))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(min: &str, max: &str) -> (Option<String>, Option<String>) {
        (Some(min.to_string()), Some(max.to_string()))
    }

    #[test]
    fn date_bounds_and_grammar() {
        let (min, max) = bounds("2010-10-10", "2020-10-10");
        assert_eq!(
            temporal_underflow_overflow("date", "2015-06-01", &min, &max),
            Some((false, false))
        );
        assert_eq!(
            temporal_underflow_overflow("date", "2005-10-10", &min, &max),
            Some((true, false))
        );
        assert_eq!(
            temporal_underflow_overflow("date", "2030-10-10", &min, &max),
            Some((false, true))
        );
        assert_eq!(temporal_underflow_overflow("date", "", &min, &max), None);
        assert_eq!(
            temporal_underflow_overflow("date", "not-a-date", &min, &max),
            None
        );
        assert_eq!(
            temporal_underflow_overflow("date", "2024-02-29", &None, &None),
            Some((false, false))
        );
        assert_eq!(
            temporal_underflow_overflow("date", "2023-02-29", &None, &None),
            None
        );
        // Out-of-grammar minimum is ignored rather than barring the value.
        let (bad_min, _) = bounds("October", "2020-10-10");
        assert_eq!(
            temporal_underflow_overflow("date", "2005-10-10", &bad_min, &max),
            Some((false, false))
        );
    }

    #[test]
    fn month_week_and_datetime_bounds() {
        let (min, max) = bounds("2000-04", "2000-09");
        assert_eq!(
            temporal_underflow_overflow("month", "2000-06", &min, &max),
            Some((false, false))
        );
        assert_eq!(
            temporal_underflow_overflow("month", "2000-02", &min, &max),
            Some((true, false))
        );
        assert_eq!(
            temporal_underflow_overflow("month", "2000-11", &min, &max),
            Some((false, true))
        );
        assert_eq!(
            temporal_underflow_overflow("month", "2000-13", &min, &max),
            None
        );
        let (min, max) = bounds("2016-W05", "2016-W10");
        assert_eq!(
            temporal_underflow_overflow("week", "2016-W07", &min, &max),
            Some((false, false))
        );
        assert_eq!(
            temporal_underflow_overflow("week", "2016-W02", &min, &max),
            Some((true, false))
        );
        assert_eq!(
            temporal_underflow_overflow("week", "2016-W26", &min, &max),
            Some((false, true))
        );
        // 2015 has 53 ISO weeks; 2016 has 52.
        assert!(parse_week("2015-W53").is_some());
        assert!(parse_week("2016-W53").is_none());
        let (min, max) = bounds("2008-03-12T23:59:59", "2015-02-13T23:59:59");
        assert_eq!(
            temporal_underflow_overflow("datetime-local", "2012-11-28T23:59:59", &min, &max),
            Some((false, false))
        );
        assert_eq!(
            temporal_underflow_overflow("datetime-local", "2008-03-01T23:59:59", &min, &max),
            Some((true, false))
        );
        assert_eq!(
            temporal_underflow_overflow("datetime-local", "2016-01-01T23:59:59", &min, &max),
            Some((false, true))
        );
    }

    #[test]
    fn time_bounds_with_optional_seconds() {
        // Omitted seconds equal explicit `:00`.
        assert_eq!(parse_time("02:00"), parse_time("02:00:00"));
        let (min, max) = bounds("01:00:00", "05:00:00");
        assert_eq!(
            temporal_underflow_overflow("time", "02:00:00", &min, &max),
            Some((false, false))
        );
        assert_eq!(
            temporal_underflow_overflow("time", "00:59:59", &min, &max),
            Some((true, false))
        );
        assert_eq!(
            temporal_underflow_overflow("time", "07:00:00", &min, &max),
            Some((false, true))
        );
        assert!(parse_time("24:00").is_none());
        assert!(parse_time("02:60").is_none());
    }

    #[test]
    fn reversed_time_range_wraps_around_midnight() {
        let (min, max) = bounds("21:00:00", "03:00:00");
        for in_range in ["22:00:00", "02:00:00", "21:00:00", "03:00:00"] {
            assert_eq!(
                temporal_underflow_overflow("time", in_range, &min, &max),
                Some((false, false)),
                "{in_range}"
            );
        }
        for in_gap in ["12:00:00", "04:00:00", "20:00:00"] {
            assert_eq!(
                temporal_underflow_overflow("time", in_gap, &min, &max),
                Some((true, true)),
                "{in_gap}"
            );
        }
    }
}
