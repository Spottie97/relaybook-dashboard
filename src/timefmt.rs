use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_rfc3339() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    format_unix(secs)
}

pub fn format_unix(secs: i64) -> String {
    let (year, month, day, hour, minute, second) = civil(secs);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn civil(secs: i64) -> (i64, u32, u32, u32, u32, u32) {
    let day_secs = 86_400;
    let days = secs.div_euclid(day_secs);
    let rem = secs.rem_euclid(day_secs) as u32;
    let hour = rem / 3600;
    let minute = (rem % 3600) / 60;
    let second = rem % 60;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let year = if month <= 2 { y + 1 } else { y };
    (year, month, day, hour, minute, second)
}

pub fn parse_timestamp(value: &serde_json::Value, fallback: &str) -> String {
    if let Some(text) = value.as_str() {
        let trimmed = text.trim();
        if trimmed.len() >= 19 && trimmed.contains('T') {
            return trimmed.to_string();
        }
    }
    if let Some(number) = value.as_i64() {
        return from_number(number);
    }
    if let Some(number) = value.as_u64() {
        return from_number(number as i64);
    }
    if let Some(number) = value.as_f64() {
        return from_number(number as i64);
    }
    fallback.to_string()
}

fn from_number(number: i64) -> String {
    if number > 10_000_000_000 {
        format_unix(number / 1000)
    } else if number > 1_000_000_000 {
        format_unix(number)
    } else {
        now_rfc3339()
    }
}
