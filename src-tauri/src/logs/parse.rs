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
}
