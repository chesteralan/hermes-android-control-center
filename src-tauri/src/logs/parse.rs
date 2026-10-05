use super::{LogLevel, LogLine};

pub fn level_from_logcat(c: char) -> Option<LogLevel> {
    match c {
        'V' | 'D' => Some(LogLevel::Debug),
        'I' => Some(LogLevel::Info),
        'W' => Some(LogLevel::Warn),
        'E' | 'F' | 'A' => Some(LogLevel::Error),
        _ => None,
    }
}

/// `MM-DD HH:MM:SS.mmm  PID  TID L Tag: message` (`logcat -v threadtime`). Tags may contain spaces.
pub fn parse_threadtime(seq: u64, received_at: u64, raw: &str) -> LogLine {
    let fallback = || LogLine {
        seq,
        received_at,
        timestamp: None,
        level: detect_level(raw),
        tag: None,
        message: raw.to_string(),
        raw: raw.to_string(),
    };
    let mut it = raw.split_whitespace();
    let (Some(date), Some(time), Some(_pid), Some(_tid), Some(lvl)) =
        (it.next(), it.next(), it.next(), it.next(), it.next())
    else {
        return fallback();
    };
    let looks_like_date = date.len() == 5 && date.as_bytes().get(2) == Some(&b'-');
    let level = lvl
        .chars()
        .next()
        .filter(|_| lvl.len() == 1)
        .and_then(level_from_logcat);
    if !looks_like_date || level.is_none() {
        return fallback();
    }
    // Everything after the level letter: "Tag: message".
    let after_level = raw
        .find(&format!(" {lvl} "))
        .map(|i| &raw[i + 3..])
        .unwrap_or("");
    let (tag, message) = match after_level.split_once(": ") {
        Some((t, m)) => (Some(t.trim().to_string()), m.to_string()),
        None => (None, after_level.trim().trim_end_matches(':').to_string()),
    };
    LogLine {
        seq,
        received_at,
        timestamp: Some(format!("{date} {time}")),
        level,
        tag,
        message,
        raw: raw.to_string(),
    }
}

fn is_iso_date(value: &str) -> bool {
    value.len() == 10
        && value.as_bytes().get(4) == Some(&b'-')
        && value.as_bytes().get(7) == Some(&b'-')
        && value
            .bytes()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
}

fn is_clock_time(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 8
        && bytes[0].is_ascii_digit()
        && bytes[1].is_ascii_digit()
        && bytes[2] == b':'
        && bytes[3].is_ascii_digit()
        && bytes[4].is_ascii_digit()
        && bytes[5] == b':'
        && bytes[6].is_ascii_digit()
        && bytes[7].is_ascii_digit()
        && (bytes.len() == 8 || matches!(bytes[8], b'.' | b',' | b'Z' | b'+' | b'-'))
}

fn split_timestamp(raw: &str) -> Option<(String, &str)> {
    let line = raw.trim_start();
    let first_end = line.find(char::is_whitespace).unwrap_or(line.len());
    let first = &line[..first_end];

    if let Some((date, time)) = first.split_once('T') {
        if is_iso_date(date) && is_clock_time(time) {
            return Some((first.to_string(), line[first_end..].trim_start()));
        }
    }

    if is_iso_date(first) || (first.len() == 5 && first.as_bytes().get(2) == Some(&b'-')) {
        let after_date = line[first_end..].trim_start();
        let time_end = after_date
            .find(char::is_whitespace)
            .unwrap_or(after_date.len());
        let time = &after_date[..time_end];
        if is_clock_time(time) {
            return Some((
                format!("{first} {time}"),
                after_date[time_end..].trim_start(),
            ));
        }
    }

    if is_clock_time(first) {
        return Some((first.to_string(), line[first_end..].trim_start()));
    }
    None
}

fn parse_timestamped_line(
    seq: u64,
    received_at: u64,
    timestamp: String,
    rest: &str,
    raw: &str,
) -> LogLine {
    let level = detect_level(rest);
    let after_level = [
        "CRITICAL", "WARNING", "ERROR", "FATAL", "WARN", "INFO", "DEBUG", "TRACE",
    ]
    .iter()
    .find_map(|word| rest.strip_prefix(word))
    .map(str::trim_start);
    let (tag, message) = match after_level {
        Some(after) if after.starts_with(": ") => (None, after[2..].to_string()),
        Some(after) => match after.split_once(": ") {
            Some((tag, message)) if !tag.is_empty() => (Some(tag.to_string()), message.to_string()),
            _ => (None, after.to_string()),
        },
        None => (None, rest.to_string()),
    };
    LogLine {
        seq,
        received_at,
        timestamp: Some(timestamp),
        level,
        tag,
        message,
        raw: raw.to_string(),
    }
}

/// Parses common Hermes/Python log lines and preserves the exact input in `raw`.
pub fn parse_log_line(seq: u64, received_at: u64, raw: &str) -> LogLine {
    let threadtime = parse_threadtime(seq, received_at, raw);
    if threadtime.timestamp.is_some() {
        return threadtime;
    }
    match split_timestamp(raw) {
        Some((timestamp, rest)) => parse_timestamped_line(seq, received_at, timestamp, rest, raw),
        None => LogLine {
            seq,
            received_at,
            timestamp: None,
            level: detect_level(raw),
            tag: None,
            message: raw.to_string(),
            raw: raw.to_string(),
        },
    }
}

pub struct LogLineParser {
    attach_multiline: bool,
    next_sequence: u64,
    pending: Option<LogLine>,
}

impl LogLineParser {
    pub fn new(attach_multiline: bool) -> Self {
        Self {
            attach_multiline,
            next_sequence: 0,
            pending: None,
        }
    }

    /// Returns completed records; the current record is held for possible continuation lines.
    pub fn push(&mut self, received_at: u64, raw: &str) -> Vec<LogLine> {
        let parsed = parse_log_line(self.next_sequence + 1, received_at, raw);
        if self.attach_multiline && parsed.timestamp.is_none() {
            if let Some(pending) = self.pending.as_mut() {
                pending.raw.push('\n');
                pending.raw.push_str(raw);
                pending.message.push('\n');
                pending.message.push_str(raw);
                return Vec::new();
            }
        }

        let completed = self.pending.take().into_iter().collect::<Vec<_>>();
        self.next_sequence += 1;
        self.pending = Some(LogLine {
            seq: self.next_sequence,
            ..parsed
        });
        completed
    }

    pub fn push_standalone(&mut self, received_at: u64, raw: &str) -> Vec<LogLine> {
        let mut completed = self.pending.take().into_iter().collect::<Vec<_>>();
        self.next_sequence += 1;
        completed.push(LogLine {
            seq: self.next_sequence,
            received_at,
            timestamp: None,
            level: detect_level(raw),
            tag: None,
            message: raw.to_string(),
            raw: raw.to_string(),
        });
        completed
    }

    pub fn finish(&mut self) -> Option<LogLine> {
        self.pending.take()
    }
}

fn has_word(line: &str, word: &str) -> bool {
    line.split(|c: char| !c.is_ascii_alphanumeric())
        .any(|w| w == word)
}

/// Best-effort level for unstructured text (e.g. Python logging). `None` when unsure.
pub fn detect_level(line: &str) -> Option<LogLevel> {
    if ["ERROR", "CRITICAL", "FATAL", "Traceback"]
        .iter()
        .any(|w| has_word(line, w))
    {
        Some(LogLevel::Error)
    } else if ["WARN", "WARNING"].iter().any(|w| has_word(line, w)) {
        Some(LogLevel::Warn)
    } else if has_word(line, "INFO") {
        Some(LogLevel::Info)
    } else if ["DEBUG", "TRACE"].iter().any(|w| has_word(line, w)) {
        Some(LogLevel::Debug)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_threadtime_with_spaced_tag() {
        let l = parse_threadtime(
            1,
            0,
            "10-02 16:11:21.369  1266  1266 I VoLTE IMCB: ENTER imcb_bearer_recv()@1#580",
        );
        assert_eq!(l.timestamp.as_deref(), Some("10-02 16:11:21.369"));
        assert_eq!(l.level, Some(LogLevel::Info));
        assert_eq!(l.tag.as_deref(), Some("VoLTE IMCB"));
        assert_eq!(l.message, "ENTER imcb_bearer_recv()@1#580");
    }

    #[test]
    fn maps_all_logcat_levels() {
        for (c, lvl) in [
            ('V', LogLevel::Debug),
            ('W', LogLevel::Warn),
            ('E', LogLevel::Error),
            ('F', LogLevel::Error),
        ] {
            let l = parse_threadtime(1, 0, &format!("10-02 16:11:21.369  1  2 {c} Tag: m"));
            assert_eq!(l.level, Some(lvl));
        }
    }

    #[test]
    fn unstructured_lines_keep_raw_text() {
        let l = parse_threadtime(1, 0, "--------- beginning of main");
        assert_eq!(l.level, None);
        assert_eq!(l.message, "--------- beginning of main");
        assert_eq!(l.raw, l.message);
    }

    #[test]
    fn detects_python_logging_levels() {
        assert_eq!(
            detect_level("2026-10-02 13:42:01,123 INFO gateway started"),
            Some(LogLevel::Info)
        );
        assert_eq!(detect_level("WARNING: retrying"), Some(LogLevel::Warn));
        assert_eq!(
            detect_level("Traceback (most recent call last):"),
            Some(LogLevel::Error)
        );
        assert_eq!(detect_level("information only"), None);
    }

    #[test]
    fn parses_iso_python_and_clock_timestamps() {
        let lines = [
            parse_log_line(1, 1, "2026-10-05T12:30:45.123Z INFO gateway started"),
            parse_log_line(2, 2, "2026-10-05 12:30:45,123 WARNING gateway: retrying"),
            parse_log_line(3, 3, "12:30:45 ERROR worker: failed"),
        ];

        assert_eq!(
            lines[0].timestamp.as_deref(),
            Some("2026-10-05T12:30:45.123Z")
        );
        assert_eq!(lines[0].level, Some(LogLevel::Info));
        assert_eq!(
            lines[1].timestamp.as_deref(),
            Some("2026-10-05 12:30:45,123")
        );
        assert_eq!(lines[1].level, Some(LogLevel::Warn));
        assert_eq!(lines[1].tag.as_deref(), Some("gateway"));
        assert_eq!(lines[1].message, "retrying");
        assert_eq!(lines[2].timestamp.as_deref(), Some("12:30:45"));
        assert_eq!(lines[2].level, Some(LogLevel::Error));
        assert_eq!(lines[2].tag.as_deref(), Some("worker"));
        assert_eq!(
            lines
                .iter()
                .map(|line| line.raw.as_str())
                .collect::<Vec<_>>(),
            [
                "2026-10-05T12:30:45.123Z INFO gateway started",
                "2026-10-05 12:30:45,123 WARNING gateway: retrying",
                "12:30:45 ERROR worker: failed",
            ]
        );
    }

    #[test]
    fn maps_warning_error_and_debug_aliases() {
        assert_eq!(detect_level("WARN worker retry"), Some(LogLevel::Warn));
        assert_eq!(
            detect_level("CRITICAL gateway failure"),
            Some(LogLevel::Error)
        );
        assert_eq!(detect_level("DEBUG parser enabled"), Some(LogLevel::Debug));
    }

    #[test]
    fn multiline_parser_attaches_continuations_when_enabled() {
        let mut parser = LogLineParser::new(true);
        assert!(parser
            .push(10, "2026-10-05 12:30:45,123 ERROR worker: failed")
            .is_empty());
        assert!(parser
            .push(11, "Traceback (most recent call last):")
            .is_empty());
        assert!(parser.push(12, "  ValueError: invalid input").is_empty());
        let next = parser.push(13, "2026-10-05 12:30:46,000 INFO worker: recovered");

        assert_eq!(next.len(), 1);
        assert_eq!(next[0].seq, 1);
        assert_eq!(next[0].level, Some(LogLevel::Error));
        assert_eq!(
            next[0].raw,
            "2026-10-05 12:30:45,123 ERROR worker: failed\nTraceback (most recent call last):\n  ValueError: invalid input"
        );
        let final_line = parser.finish().unwrap();
        assert_eq!(final_line.seq, 2);
        assert_eq!(final_line.level, Some(LogLevel::Info));
    }

    #[test]
    fn multiline_parser_can_keep_unstructured_lines_separate() {
        let mut parser = LogLineParser::new(false);
        assert!(parser.push(10, "first").is_empty());
        let first = parser.push(11, "second");
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].raw, "first");
        assert_eq!(parser.finish().unwrap().raw, "second");
    }
}
