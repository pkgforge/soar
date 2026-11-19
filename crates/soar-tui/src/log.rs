//! The rolling log of everything the interface and soar printed.

use std::collections::VecDeque;

use crate::widgets::scroll::ListCursor;

/// How many lines the log keeps, dropping the oldest beyond that.
const CAP: usize = 2000;

/// How serious one log line is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LogLevel {
    /// Detail that usually does not matter.
    Debug,
    /// Ordinary progress.
    Info,
    /// Something worth reading.
    Warn,
    /// Something failed.
    Error,
}

/// One line of the log, with when it arrived.
#[derive(Debug)]
pub(crate) struct LogLine {
    /// When the line arrived, as a wall-clock string.
    pub(crate) time: String,
    /// How serious the line is.
    pub(crate) level: LogLevel,
    /// What the line says.
    pub(crate) text: String,
}

/// The last few thousand lines, with where the log pane is scrolled to.
#[derive(Debug, Default)]
pub(crate) struct Log {
    /// The kept lines, oldest first.
    pub(crate) lines: VecDeque<LogLine>,
    /// Which line the log pane shows first.
    pub(crate) cursor: ListCursor,
    /// Whether the pane sticks to the newest line.
    pub(crate) follow: bool,
}

impl Log {
    /// Creates a log that follows its own end.
    pub(crate) fn new() -> Log {
        Log {
            lines: VecDeque::new(),
            cursor: ListCursor::default(),
            follow: true,
        }
    }

    /// Appends one line, stamped now, and drops the oldest lines above the
    /// cap. While following, the cursor stays on the newest line.
    pub(crate) fn push(&mut self, level: LogLevel, text: impl Into<String>) {
        self.lines.push_back(LogLine {
            time: crate::format::clock_now(),
            level,
            text: text.into(),
        });
        let len = self.lines.len();
        if len > CAP {
            self.lines.drain(..len - CAP);
        }
        if self.follow {
            self.cursor.bottom(self.lines.len());
        }
        self.cursor.clamp(self.lines.len());
    }

    /// Splits one formatted subscriber line into its level and message.
    /// The level word is padded to a fixed width, so the line is trimmed
    /// first; a line without a known level word counts as info.
    pub(crate) fn parse_output(raw: &str) -> (LogLevel, String) {
        let trimmed = raw.trim_start();
        let Some((word, rest)) = trimmed.split_once(' ') else {
            return (LogLevel::Info, trimmed.to_string());
        };
        let level = match word {
            "ERROR" => LogLevel::Error,
            "WARN" => LogLevel::Warn,
            "INFO" => LogLevel::Info,
            "DEBUG" | "TRACE" => LogLevel::Debug,
            _ => return (LogLevel::Info, trimmed.to_string()),
        };
        (level, rest.trim_start().to_string())
    }

    /// Removes ANSI escape sequences, which hook and child output often
    /// carry: `ESC [` sequences end at the first byte from `@` to `~`, and
    /// any other escape covers exactly one following byte.
    pub(crate) fn strip_ansi(input: &str) -> String {
        let bytes = input.as_bytes();
        let mut kept = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] != 0x1b {
                kept.push(bytes[i]);
                i += 1;
                continue;
            }
            if bytes.get(i + 1) == Some(&b'[') {
                i += 2;
                while i < bytes.len() && !(0x40..=0x7e).contains(&bytes[i]) {
                    i += 1;
                }
                i += 1;
            } else {
                i += 2;
            }
        }
        String::from_utf8_lossy(&kept).into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::{CAP, Log, LogLevel};

    #[test]
    fn pushing_past_the_cap_keeps_only_the_newest_lines() {
        let mut log = Log::new();
        for n in 0..=CAP {
            log.push(LogLevel::Info, format!("line {n}"));
        }
        assert_eq!(log.lines.len(), CAP);
        assert_eq!(
            log.lines.front().map(|line| line.text.as_str()),
            Some("line 1")
        );
        assert_eq!(
            log.lines.back().map(|line| line.text.clone()),
            Some(format!("line {CAP}"))
        );
    }

    #[test]
    fn a_padded_level_word_sets_the_level_and_the_rest_is_text() {
        let (level, text) = Log::parse_output(" WARN x");
        assert_eq!(level, LogLevel::Warn);
        assert_eq!(text, "x");
        let (level, text) = Log::parse_output("ERROR  disk full");
        assert_eq!(level, LogLevel::Error);
        assert_eq!(text, "disk full");
    }

    #[test]
    fn a_line_without_a_level_word_is_info() {
        let (level, text) = Log::parse_output("just some output");
        assert_eq!(level, LogLevel::Info);
        assert_eq!(text, "just some output");
    }

    #[test]
    fn trace_maps_to_debug() {
        let (level, _) = Log::parse_output("TRACE  deep detail");
        assert_eq!(level, LogLevel::Debug);
    }

    #[test]
    fn ansi_sequences_are_stripped() {
        assert_eq!(Log::strip_ansi("\x1b[33mhi\x1b[0m"), "hi");
        assert_eq!(Log::strip_ansi("a\x1b[1mb"), "ab");
        assert_eq!(Log::strip_ansi("a\x1bXb"), "ab");
    }

    #[test]
    fn following_keeps_the_cursor_on_the_newest_line() {
        let mut log = Log::new();
        log.push(LogLevel::Info, "one");
        log.push(LogLevel::Info, "two");
        assert_eq!(log.cursor.window(2, 1), 1..2);
        log.follow = false;
        log.cursor.up(1);
        log.push(LogLevel::Info, "three");
        assert_eq!(log.cursor.window(3, 1), 0..1);
    }
}
