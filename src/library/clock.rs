//! The "clock" module's helpers: format and parse timestamps with chrono.


/// Formats a Unix timestamp (seconds) as local time, using a date pattern
/// like "%Y-%m-%d %H:%M:%S". A bad pattern is a run-time error, not a crash.
pub(super) fn clock_format(stamp: i64, pattern: &str) -> Result<String, String> {
    use chrono::format::{Item, StrftimeItems};
    let utc = chrono::DateTime::from_timestamp(stamp, 0).ok_or("clock.format: the timestamp is out of range")?;
    let local = utc.with_timezone(&chrono::Local);
    let items: Vec<Item> = StrftimeItems::new(pattern).collect();
    if items.iter().any(|it| matches!(it, Item::Error)) {
        return Err(format!("clock.format: \"{}\" is not a valid date pattern", pattern));
    }
    Ok(local.format_with_items(items.iter()).to_string())
}

/// Reads a local date-and-time written like `pattern` and returns its Unix
/// timestamp (seconds).
pub(super) fn clock_parse(s: &str, pattern: &str) -> Result<i64, String> {
    use chrono::TimeZone;
    let naive = chrono::NaiveDateTime::parse_from_str(s, pattern)
        .map_err(|e| format!("clock.parse: {} does not match \"{}\" ({})", quote(s), pattern, e))?;
    match chrono::Local.from_local_datetime(&naive).single() {
        Some(dt) => Ok(dt.timestamp()),
        None => Err(format!("clock.parse: {} is an ambiguous or impossible local time", quote(s))),
    }
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s)
}
