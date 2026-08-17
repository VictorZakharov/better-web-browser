//! RFC 6265 cookie-date parsing, which is deliberately more permissive than HTTP-date parsing.

use std::time::{Duration, SystemTime, UNIX_EPOCH};
use time::{Date, Month};

pub(super) fn parse_cookie_date(input: &str) -> Option<SystemTime> {
    let mut hour = None;
    let mut minute = None;
    let mut second = None;
    let mut day = None;
    let mut month = None;
    let mut year = None;

    for token in input
        .split(is_date_delimiter)
        .filter(|token| !token.is_empty())
    {
        if hour.is_none()
            && let Some((parsed_hour, parsed_minute, parsed_second)) = parse_time(token)
        {
            hour = Some(parsed_hour);
            minute = Some(parsed_minute);
            second = Some(parsed_second);
            continue;
        }
        if day.is_none()
            && token.len() <= 2
            && let Ok(value) = token.parse::<u8>()
        {
            day = Some(value);
            continue;
        }
        if month.is_none()
            && let Some(value) = parse_month(token)
        {
            month = Some(value);
            continue;
        }
        if year.is_none()
            && (2..=4).contains(&token.len())
            && token.bytes().all(|byte| byte.is_ascii_digit())
        {
            year = token.parse::<i32>().ok();
        }
    }

    let mut year = year?;
    if (70..=99).contains(&year) {
        year += 1900;
    } else if (0..=69).contains(&year) {
        year += 2000;
    }
    if year < 1601 || hour? > 23 || minute? > 59 || second? > 59 {
        return None;
    }
    let timestamp = Date::from_calendar_date(year, month?, day?)
        .ok()?
        .with_hms(hour?, minute?, second?)
        .ok()?
        .assume_utc()
        .unix_timestamp();
    if timestamp >= 0 {
        UNIX_EPOCH.checked_add(Duration::from_secs(timestamp as u64))
    } else {
        UNIX_EPOCH.checked_sub(Duration::from_secs(timestamp.unsigned_abs()))
    }
}

fn parse_time(token: &str) -> Option<(u8, u8, u8)> {
    let mut fields = token.split(':');
    let hour = parse_one_or_two_digits(fields.next()?)?;
    let minute = parse_one_or_two_digits(fields.next()?)?;
    let second = parse_one_or_two_digits(fields.next()?)?;
    fields.next().is_none().then_some((hour, minute, second))
}

fn parse_one_or_two_digits(value: &str) -> Option<u8> {
    (!value.is_empty() && value.len() <= 2 && value.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| value.parse().ok())?
}

fn parse_month(token: &str) -> Option<Month> {
    let prefix = token.get(..3)?.to_ascii_lowercase();
    Some(match prefix.as_str() {
        "jan" => Month::January,
        "feb" => Month::February,
        "mar" => Month::March,
        "apr" => Month::April,
        "may" => Month::May,
        "jun" => Month::June,
        "jul" => Month::July,
        "aug" => Month::August,
        "sep" => Month::September,
        "oct" => Month::October,
        "nov" => Month::November,
        "dec" => Month::December,
        _ => return None,
    })
}

fn is_date_delimiter(character: char) -> bool {
    matches!(character as u32, 0x09 | 0x20..=0x2f | 0x3b..=0x40 | 0x5b..=0x60 | 0x7b..=0x7e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_legacy_cookie_date_tokens_and_rejects_invalid_dates() {
        assert_eq!(
            parse_cookie_date("Wed, 21 Oct 2015 07:28:00 GMT")
                .unwrap()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            1_445_412_480
        );
        assert!(parse_cookie_date("Wed Oct 21 07:28:00 15").is_some());
        assert!(parse_cookie_date("32 Oct 2015 07:28:00 GMT").is_none());
        assert!(parse_cookie_date("21 Oct 1500 07:28:00 GMT").is_none());
    }
}
