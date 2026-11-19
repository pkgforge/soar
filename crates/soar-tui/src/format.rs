//! Turns raw counts and times into the short strings the interface shows.
//!
//! Most helpers below wait for the views that will call them.

#![allow(dead_code)]

use std::{
    path::Path,
    time::{Duration, SystemTime},
};

use ratatui::text::Span;
use soar_utils::bytes::format_bytes;

/// A byte count with one decimal, such as `5.2 MiB`.
pub(crate) fn size(bytes: u64) -> String {
    format_bytes(bytes, 1)
}

/// A transfer rate, such as `12.5 MiB/s`.
pub(crate) fn speed(bytes_per_sec: f64) -> String {
    format!("{}/s", size(bytes_per_sec as u64))
}

/// An elapsed time as `m:ss`, or `h:mm:ss` past one hour.
pub(crate) fn clock(d: Duration) -> String {
    let total = d.as_secs();
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// A remaining time rounded down to its coarsest unit, such as `3s` or `2m`.
pub(crate) fn eta(d: Duration) -> String {
    let total = d.as_secs();
    if total < 60 {
        format!("{total}s")
    } else if total < 3600 {
        format!("{}m", total / 60)
    } else {
        format!("{}h", total / 3600)
    }
}

/// How long ago something happened, such as `just now` or `3 days ago`.
pub(crate) fn relative(then: SystemTime, now: SystemTime) -> String {
    let elapsed = now.duration_since(then).unwrap_or(Duration::ZERO).as_secs();
    match elapsed {
        0..=59 => "just now".to_string(),
        60..=3599 => ago(elapsed / 60, "minute"),
        3600..=86399 => ago(elapsed / 3600, "hour"),
        86400..=172799 => "yesterday".to_string(),
        172800..=604799 => ago(elapsed / 86400, "day"),
        604800..=2591999 => ago(elapsed / 604800, "week"),
        2592000..=31535999 => ago(elapsed / 2592000, "month"),
        _ => ago(elapsed / 31536000, "year"),
    }
}

/// One count of a named span of time, singular for one.
fn ago(count: u64, unit: &str) -> String {
    if count == 1 {
        format!("1 {unit} ago")
    } else {
        format!("{count} {unit}s ago")
    }
}

/// Cuts text to fit `width` display columns, keeping its start and adding
/// the ellipsis.
pub(crate) fn truncate(s: &str, width: usize, ellipsis: &str) -> String {
    if Span::raw(s).width() <= width {
        return s.to_string();
    }
    let ellipsis_width = Span::raw(ellipsis).width();
    if ellipsis_width >= width {
        return take(s, width);
    }
    format!("{}{ellipsis}", take(s, width - ellipsis_width))
}

/// Cuts text to fit `width` display columns, keeping its start and end, so
/// long paths and URLs keep both sides.
pub(crate) fn truncate_middle(s: &str, width: usize, ellipsis: &str) -> String {
    if Span::raw(s).width() <= width {
        return s.to_string();
    }
    let ellipsis_width = Span::raw(ellipsis).width();
    if ellipsis_width >= width {
        return take(s, width);
    }
    let kept = width - ellipsis_width;
    let start_keep = kept.div_ceil(2);
    let start = take(s, start_keep);
    let end = take_end(s, kept - start_keep);
    format!("{start}{ellipsis}{end}")
}

/// The longest prefix of `s` that fits `width` display columns.
fn take(s: &str, width: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for ch in s.chars() {
        let char_width = Span::raw(ch.to_string()).width();
        if used + char_width > width {
            break;
        }
        out.push(ch);
        used += char_width;
    }
    out
}

/// The longest suffix of `s` that fits `width` display columns.
fn take_end(s: &str, width: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for ch in s.chars().rev() {
        let char_width = Span::raw(ch.to_string()).width();
        if used + char_width > width {
            break;
        }
        out.push(ch);
        used += char_width;
    }
    out.chars().rev().collect()
}

/// A number with a separator every three digits: `4213` becomes `4,213`.
pub(crate) fn thousands(n: usize) -> String {
    if n < 1000 {
        return n.to_string();
    }
    format!("{},{:03}", thousands(n / 1000), n % 1000)
}

/// Splits `text` into rows at most `width` display columns wide,
/// breaking at spaces so words stay whole. A word wider than the row is
/// cut into pieces that fit.
pub(crate) fn wrap(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    // Words wider than the wrap width become pieces that fit, so the
    // assembly below only handles spaces.
    let mut tokens = Vec::new();
    for word in text.split(' ') {
        let mut rest = word;
        while Span::raw(rest).width() > width {
            let piece = take(rest, width);
            if piece.is_empty() {
                break;
            }
            tokens.push(piece.clone());
            rest = &rest[piece.len()..];
        }
        tokens.push(rest.to_string());
    }
    let mut rows = Vec::new();
    let mut row = String::new();
    let mut row_width = 0;
    for token in tokens {
        let token_width = Span::raw(&token).width();
        let gap = usize::from(!row.is_empty());
        if row_width + gap + token_width > width {
            // A token that cannot fit an empty row, such as a wide
            // character at a tiny width, starts its own row instead of
            // flushing an empty one ahead of it.
            if !row.is_empty() {
                rows.push(std::mem::take(&mut row));
                row_width = 0;
            }
        } else if gap == 1 {
            row.push(' ');
        }
        row.push_str(&token);
        row_width += gap + token_width;
    }
    if !row.is_empty() {
        rows.push(row);
    }
    rows
}

/// A path with the user's home directory replaced by `~`.
pub(crate) fn home_relative(path: &Path) -> String {
    let text = path.to_string_lossy();
    let Ok(home) = std::env::var("HOME") else {
        return text.into_owned();
    };
    if home.is_empty() {
        return text.into_owned();
    }
    if text == home {
        return "~".to_string();
    }
    let prefix = format!("{home}/");
    match text.strip_prefix(prefix.as_str()) {
        Some(rest) => format!("~/{rest}"),
        None => text.into_owned(),
    }
}

/// Parses an install date as soar records it in UTC, `2026-11-02 13:45:07`.
pub(crate) fn parse_installed_date(s: &str) -> Option<SystemTime> {
    let date = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok()?;
    Some(SystemTime::from(date.and_utc()))
}

/// The current local time as `HH:MM:SS`.
pub(crate) fn clock_now() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}

#[cfg(test)]
mod tests {
    use std::{
        path::Path,
        time::{Duration, SystemTime},
    };

    use ratatui::text::Span;

    use super::{
        clock, clock_now, eta, home_relative, parse_installed_date, relative, size, speed,
        thousands, truncate, truncate_middle, wrap,
    };

    #[test]
    fn sizes_show_one_decimal() {
        assert_eq!(size(0), "0.0 B");
        assert_eq!(size(1024), "1.0 KiB");
        assert_eq!(size(5452595), "5.2 MiB");
    }

    #[test]
    fn speeds_append_per_second() {
        assert_eq!(speed(0.0), "0.0 B/s");
        assert_eq!(speed(1024.0), "1.0 KiB/s");
    }

    #[test]
    fn clocks_split_below_an_hour() {
        assert_eq!(clock(Duration::from_secs(7)), "0:07");
        assert_eq!(clock(Duration::from_secs(307)), "5:07");
        assert_eq!(clock(Duration::from_secs(3600)), "1:00:00");
        assert_eq!(clock(Duration::from_secs(3723)), "1:02:03");
    }

    #[test]
    fn etas_round_down_to_the_coarsest_unit() {
        assert_eq!(eta(Duration::from_secs(3)), "3s");
        assert_eq!(eta(Duration::from_secs(59)), "59s");
        assert_eq!(eta(Duration::from_secs(60)), "1m");
        assert_eq!(eta(Duration::from_secs(125)), "2m");
        assert_eq!(eta(Duration::from_secs(3599)), "59m");
        assert_eq!(eta(Duration::from_secs(3600)), "1h");
        assert_eq!(eta(Duration::from_secs(7200)), "2h");
    }

    #[test]
    fn relative_times_use_the_right_unit() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(2_000_000_000);
        let ago = |secs: u64| now - Duration::from_secs(secs);
        assert_eq!(relative(ago(0), now), "just now");
        assert_eq!(relative(ago(59), now), "just now");
        assert_eq!(relative(ago(60), now), "1 minute ago");
        assert_eq!(relative(ago(300), now), "5 minutes ago");
        assert_eq!(relative(ago(3600), now), "1 hour ago");
        assert_eq!(relative(ago(3 * 3600), now), "3 hours ago");
        assert_eq!(relative(ago(86399), now), "23 hours ago");
        assert_eq!(relative(ago(86400), now), "yesterday");
        assert_eq!(relative(ago(3 * 86400), now), "3 days ago");
        assert_eq!(relative(ago(7 * 86400), now), "1 week ago");
        assert_eq!(relative(ago(14 * 86400), now), "2 weeks ago");
        assert_eq!(relative(ago(30 * 86400), now), "1 month ago");
        assert_eq!(relative(ago(60 * 86400), now), "2 months ago");
        assert_eq!(relative(ago(365 * 86400), now), "1 year ago");
        assert_eq!(relative(ago(2 * 365 * 86400), now), "2 years ago");
        assert_eq!(relative(now + Duration::from_secs(100), now), "just now");
    }

    #[test]
    fn truncate_keeps_the_start() {
        assert_eq!(truncate("bat", 10, "…"), "bat");
        assert_eq!(truncate("bat-extras", 10, "…"), "bat-extras");
        assert_eq!(truncate("bat-extras", 9, "…"), "bat-extr…");
        assert_eq!(truncate("bat-extras", 0, "…"), "");
        assert_eq!(truncate("日本語", 3, "…"), "日…");
    }

    #[test]
    fn truncate_drops_the_ellipsis_when_it_does_not_fit() {
        assert_eq!(truncate("bat-extras", 3, "..."), "bat");
    }

    #[test]
    fn truncate_middle_keeps_both_ends() {
        assert_eq!(truncate_middle("bat", 10, "…"), "bat");
        assert_eq!(truncate_middle("bat-extras", 10, "…"), "bat-extras");
        assert_eq!(truncate_middle("abcdefgh", 5, "…"), "ab…gh");
        assert_eq!(
            truncate_middle("https://example.com/path", 12, "…"),
            "https:…/path"
        );
        assert_eq!(truncate_middle("bat-extras", 0, "…"), "");
    }

    #[test]
    fn thousands_separate_every_third_digit() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1000), "1,000");
        assert_eq!(thousands(4213), "4,213");
        assert_eq!(thousands(1_000_000), "1,000,000");
    }

    #[test]
    fn paths_under_home_shrink_to_a_tilde() {
        let Ok(home) = std::env::var("HOME") else {
            return;
        };
        if home.is_empty() {
            return;
        }
        let deep = Path::new(&home).join("packages").join("bat");
        assert_eq!(home_relative(&deep), "~/packages/bat");
        assert_eq!(home_relative(Path::new(&home)), "~");
        assert_eq!(home_relative(Path::new("/usr/bin")), "/usr/bin");
    }

    #[test]
    fn installed_dates_parse_from_the_recorded_format() {
        assert_eq!(
            parse_installed_date("1970-01-01 00:00:01"),
            Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1))
        );
        assert!(parse_installed_date("2026-11-02 13:45:07").is_some());
        assert!(parse_installed_date("2026-11-02").is_none());
        assert!(parse_installed_date("").is_none());
    }

    #[test]
    fn wrap_breaks_at_spaces_within_the_width() {
        assert_eq!(wrap("one two three", 8), ["one two", "three"]);
        assert_eq!(wrap("short", 8), ["short"]);
        assert!(wrap("", 8).is_empty());
    }

    #[test]
    fn wrap_counts_display_columns_not_characters() {
        // Each character is two columns wide, so four of them cannot
        // share a five-column row.
        assert_eq!(wrap("日本語よ", 5), ["日本", "語よ"]);
        assert_eq!(wrap("日本語 テスト", 6), ["日本語", "テスト"]);
    }

    #[test]
    fn wrap_cuts_a_long_ascii_word_to_fit() {
        assert_eq!(wrap("abcdefghij", 4), ["abcd", "efgh", "ij"]);
    }

    #[test]
    fn wrap_of_a_token_wider_than_the_row_never_starts_empty() {
        let token = "a".repeat(70);
        let rows = wrap(&token, 30);
        assert_eq!(rows.len(), 3);
        for row in &rows {
            assert!(!row.is_empty(), "empty row among {rows:?}");
            assert!(
                Span::raw(row).width() <= 30,
                "row {row:?} is over 30 columns"
            );
        }
    }

    #[test]
    fn wrap_of_a_wide_character_at_a_one_column_row_is_one_row() {
        assert_eq!(wrap("日本", 1), ["日本"]);
    }

    #[test]
    fn the_clock_shows_hours_minutes_and_seconds() {
        let now = clock_now();
        assert_eq!(now.len(), 8);
        assert_eq!(now.as_bytes()[2], b':');
        assert_eq!(now.as_bytes()[5], b':');
    }
}
