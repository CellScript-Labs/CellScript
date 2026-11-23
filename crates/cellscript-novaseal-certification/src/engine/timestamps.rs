use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn is_utc_timestamp_z(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 20
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[10] == b'T'
        && bytes[13] == b':'
        && bytes[16] == b':'
        && bytes[19] == b'Z'
        && ascii_digits(&bytes[0..4])
        && ascii_digits(&bytes[5..7])
        && ascii_digits(&bytes[8..10])
        && ascii_digits(&bytes[11..13])
        && ascii_digits(&bytes[14..16])
        && ascii_digits(&bytes[17..19])
        && valid_ymd_time(
            parse_digits(&bytes[0..4]),
            parse_digits(&bytes[5..7]),
            parse_digits(&bytes[8..10]),
            parse_digits(&bytes[11..13]),
            parse_digits(&bytes[14..16]),
            parse_digits(&bytes[17..19]),
        )
}

pub(super) fn is_utc_timestamp_z_not_future(value: &str) -> bool {
    let Some(timestamp) = utc_timestamp_seconds(value) else {
        return false;
    };
    let Some(now) = current_unix_seconds() else {
        return false;
    };
    timestamp <= now
}

pub(super) fn is_utc_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && ascii_digits(&bytes[0..4])
        && ascii_digits(&bytes[5..7])
        && ascii_digits(&bytes[8..10])
        && valid_ymd(parse_digits(&bytes[0..4]), parse_digits(&bytes[5..7]), parse_digits(&bytes[8..10]))
}

pub(super) fn is_utc_date_not_future(value: &str) -> bool {
    let Some(days) = utc_date_days(value) else {
        return false;
    };
    let Some(now) = current_unix_seconds() else {
        return false;
    };
    days <= (now / 86_400) as i64
}

fn ascii_digits(bytes: &[u8]) -> bool {
    bytes.iter().all(u8::is_ascii_digit)
}

fn parse_digits(bytes: &[u8]) -> Option<u32> {
    if ascii_digits(bytes) {
        Some(bytes.iter().fold(0, |acc, byte| (acc * 10) + u32::from(byte - b'0')))
    } else {
        None
    }
}

fn valid_ymd_time(
    year: Option<u32>,
    month: Option<u32>,
    day: Option<u32>,
    hour: Option<u32>,
    minute: Option<u32>,
    second: Option<u32>,
) -> bool {
    valid_ymd(year, month, day)
        && hour.is_some_and(|value| value < 24)
        && minute.is_some_and(|value| value < 60)
        && second.is_some_and(|value| value < 60)
}

fn valid_ymd(year: Option<u32>, month: Option<u32>, day: Option<u32>) -> bool {
    let (Some(year), Some(month), Some(day)) = (year, month, day) else {
        return false;
    };
    let Some(max_day) = days_in_month(year, month) else {
        return false;
    };
    year > 0 && (1..=max_day).contains(&day)
}

fn utc_timestamp_seconds(value: &str) -> Option<u64> {
    if !is_utc_timestamp_z(value) {
        return None;
    }
    let bytes = value.as_bytes();
    let days = utc_date_components_days(parse_digits(&bytes[0..4])?, parse_digits(&bytes[5..7])?, parse_digits(&bytes[8..10])?)?;
    let seconds = days.checked_mul(86_400)?.checked_add(
        parse_digits(&bytes[11..13])? as i64 * 3_600
            + parse_digits(&bytes[14..16])? as i64 * 60
            + parse_digits(&bytes[17..19])? as i64,
    )?;
    u64::try_from(seconds).ok()
}

fn utc_date_days(value: &str) -> Option<i64> {
    if !is_utc_date(value) {
        return None;
    }
    let bytes = value.as_bytes();
    utc_date_components_days(parse_digits(&bytes[0..4])?, parse_digits(&bytes[5..7])?, parse_digits(&bytes[8..10])?)
}

fn utc_date_components_days(year: u32, month: u32, day: u32) -> Option<i64> {
    if !valid_ymd(Some(year), Some(month), Some(day)) {
        return None;
    }
    let mut year = i64::from(year);
    let month = i64::from(month);
    let day = i64::from(day);
    year -= i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_for_year = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_for_year + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(era * 146_097 + day_of_era - 719_468)
}

fn current_unix_seconds() -> Option<u64> {
    SystemTime::now().duration_since(UNIX_EPOCH).ok().map(|duration| duration.as_secs())
}

fn days_in_month(year: u32, month: u32) -> Option<u32> {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => Some(31),
        4 | 6 | 9 | 11 => Some(30),
        2 if leap_year(year) => Some(29),
        2 => Some(28),
        _ => None,
    }
}

fn leap_year(year: u32) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}
