//! Termux bridge (ADR-004): sshd in Termux on 127.0.0.1:8022, reached via `adb forward`.

pub mod check;
pub mod forward;
pub mod keys;
pub mod known_hosts;
pub mod ssh;

/// POSIX single-quote escaping for embedding a value in a remote shell command.
pub fn shell_escape(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:=@%+,".contains(c))
    {
        return s.to_string();
    }
    format!("'{}'", s.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::shell_escape;

    #[test]
    fn escapes_for_posix_shells() {
        assert_eq!(shell_escape("plain-word_1.0"), "plain-word_1.0");
        assert_eq!(shell_escape(""), "''");
        assert_eq!(shell_escape("a b"), "'a b'");
        assert_eq!(shell_escape("it's"), r"'it'\''s'");
        assert_eq!(
            shell_escape("$(rm -rf /); `x` \"y\""),
            "'$(rm -rf /); `x` \"y\"'"
        );
        assert_eq!(shell_escape("line1\nline2"), "'line1\nline2'");
    }
}
