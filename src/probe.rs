use std::fs::File;
use std::os::unix::fs::FileExt;
use std::path::Path;

pub struct Probe {
    file: Option<File>,
    buf: Vec<u8>,
    len: usize,
    truncated: bool,
}

impl Probe {
    pub fn open(path: impl AsRef<Path>, cap: usize) -> Self {
        Self {
            file: File::open(path).ok(),
            buf: vec![0u8; cap],
            len: 0,
            truncated: false,
        }
    }

    pub fn refresh(&mut self) -> bool {
        self.len = 0;
        self.truncated = false;
        let Some(f) = self.file.as_ref() else {
            return false;
        };
        match f.read_at(&mut self.buf, 0) {
            Ok(n) => {
                self.len = n;
                self.truncated = n == self.buf.len();
                !self.truncated
            }
            Err(_) => false,
        }
    }

    pub fn data(&self) -> &[u8] {
        &self.buf[..self.len]
    }
}

pub fn parse_u64(s: &[u8]) -> Option<u64> {
    let mut v: u64 = 0;
    let mut seen = false;
    for &b in s {
        if b.is_ascii_digit() {
            v = v.checked_mul(10)?.checked_add((b - b'0') as u64)?;
            seen = true;
        } else {
            break;
        }
    }
    if seen { Some(v) } else { None }
}

pub fn parse_i64(s: &[u8]) -> Option<i64> {
    if let Some(rest) = s.strip_prefix(b"-") {
        parse_u64(rest).map(|v| -(v as i64))
    } else {
        parse_u64(s).map(|v| v as i64)
    }
}

pub fn fields(line: &[u8]) -> impl Iterator<Item = &[u8]> {
    line.split(|b| b.is_ascii_whitespace())
        .filter(|f| !f.is_empty())
}

pub fn lines(data: &[u8]) -> impl Iterator<Item = &[u8]> {
    data.split(|&b| b == b'\n')
}

pub fn read_trimmed(path: &str) -> Option<String> {
    let raw = std::fs::read(path).ok()?;
    let end = raw
        .iter()
        .position(|&b| b == 0 || b == b'\n')
        .unwrap_or(raw.len());
    Some(String::from_utf8_lossy(&raw[..end]).into_owned())
}

pub fn read_u64(path: &str) -> Option<u64> {
    let raw = std::fs::read(path).ok()?;
    parse_u64(&raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_u64_stops_at_first_non_digit() {
        assert_eq!(parse_u64(b"1700000"), Some(1_700_000));
        assert_eq!(parse_u64(b"3874892 kB"), Some(3_874_892));
        assert_eq!(parse_u64(b"48686\n"), Some(48686));
        assert_eq!(parse_u64(b""), None);
        assert_eq!(parse_u64(b"kB"), None);
    }

    #[test]
    fn parse_u64_rejects_overflow_instead_of_wrapping() {
        assert_eq!(parse_u64(b"18446744073709551615"), Some(u64::MAX));
        assert_eq!(parse_u64(b"99999999999999999999999"), None);
    }

    #[test]
    fn parse_i64_handles_the_negative_speed_sentinel() {
        assert_eq!(parse_i64(b"-1\n"), Some(-1));
        assert_eq!(parse_i64(b"100\n"), Some(100));
    }

    #[test]
    fn fields_collapses_runs_of_whitespace() {
        let got: Vec<&[u8]> = fields(b"cpu  4372 0 5070").collect();
        assert_eq!(got, vec![&b"cpu"[..], b"4372", b"0", b"5070"]);
    }

    #[test]
    fn lines_splits_on_newline() {
        let got: Vec<&[u8]> = lines(b"a\nb\n").collect();
        assert_eq!(got, vec![&b"a"[..], b"b", b""]);
    }
}
