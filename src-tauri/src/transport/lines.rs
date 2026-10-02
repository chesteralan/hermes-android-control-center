/// Splits a byte stream into UTF-8 (lossy) lines, handling chunks that end mid-line and CRLF.
#[derive(Default)]
pub struct LineSplitter {
    buf: Vec<u8>,
}

impl LineSplitter {
    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        self.buf.extend_from_slice(bytes);
        let mut out = Vec::new();
        while let Some(pos) = self.buf.iter().position(|b| *b == b'\n') {
            let mut line: Vec<u8> = self.buf.drain(..=pos).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            out.push(String::from_utf8_lossy(&line).into_owned());
        }
        out
    }

    /// Remaining partial line at end of stream.
    pub fn finish(&mut self) -> Option<String> {
        if self.buf.is_empty() {
            return None;
        }
        let rest = std::mem::take(&mut self.buf);
        Some(
            String::from_utf8_lossy(&rest)
                .trim_end_matches('\r')
                .to_string(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_across_chunks_and_crlf() {
        let mut s = LineSplitter::default();
        assert_eq!(s.push(b"a\r\nb"), vec!["a"]);
        assert_eq!(s.push(b"c\nd"), vec!["bc"]);
        assert_eq!(s.finish(), Some("d".into()));
        assert_eq!(s.finish(), None);
    }

    #[test]
    fn invalid_utf8_is_lossy_not_panicking() {
        let mut s = LineSplitter::default();
        assert_eq!(s.push(&[0xff, b'x', b'\n']).len(), 1);
    }
}
