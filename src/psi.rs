use crate::probe::{Probe, fields, lines, parse_centi};

pub fn parse_some_avg10(data: &[u8]) -> Option<u64> {
    for line in lines(data) {
        if !line.starts_with(b"some") {
            continue;
        }
        for f in fields(line).skip(1) {
            if let Some(v) = f.strip_prefix(b"avg10=") {
                return parse_centi(v);
            }
        }
    }
    None
}

pub struct Psi {
    cpu: Probe,
    mem: Probe,
    io: Probe,
    pub cpu_centi: Option<u64>,
    pub mem_centi: Option<u64>,
    pub io_centi: Option<u64>,
    countdown: u8,
}

const EVERY: u8 = 2;

impl Default for Psi {
    fn default() -> Self {
        Self::new()
    }
}

impl Psi {
    pub fn new() -> Self {
        Self {
            cpu: Probe::open("/proc/pressure/cpu", 256),
            mem: Probe::open("/proc/pressure/memory", 256),
            io: Probe::open("/proc/pressure/io", 256),
            cpu_centi: None,
            mem_centi: None,
            io_centi: None,
            countdown: 0,
        }
    }

    pub fn tick(&mut self) {
        if self.countdown > 0 {
            self.countdown -= 1;
            return;
        }
        self.countdown = EVERY;
        if self.cpu.refresh() {
            self.cpu_centi = parse_some_avg10(self.cpu.data());
        }
        if self.mem.refresh() {
            self.mem_centi = parse_some_avg10(self.mem.data());
        }
        if self.io.refresh() {
            self.io_centi = parse_some_avg10(self.io.data());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CPU: &[u8] = b"some avg10=0.00 avg60=0.00 avg300=0.00 total=34433583\n\
full avg10=0.00 avg60=0.00 avg300=0.00 total=0\n";

    #[test]
    fn reads_avg10_from_the_some_line() {
        assert_eq!(parse_some_avg10(CPU), Some(0));
        assert_eq!(
            parse_some_avg10(b"some avg10=12.34 avg60=0.00 avg300=0.00 total=1\n"),
            Some(1234)
        );
    }

    #[test]
    fn ignores_the_full_line() {
        let d = b"some avg10=1.00 avg60=0.00 avg300=0.00 total=1\n\
full avg10=99.00 avg60=0.00 avg300=0.00 total=1\n";
        assert_eq!(parse_some_avg10(d), Some(100));
    }

    #[test]
    fn handles_a_saturated_value() {
        assert_eq!(
            parse_some_avg10(b"some avg10=100.00 avg60=0.00 avg300=0.00 total=1\n"),
            Some(10000)
        );
    }

    #[test]
    fn returns_none_on_malformed_input() {
        assert!(parse_some_avg10(b"").is_none());
        assert!(parse_some_avg10(b"full avg10=1.00\n").is_none());
        assert!(parse_some_avg10(b"some avg60=1.00\n").is_none());
    }

    #[test]
    fn all_three_resources_are_read_on_a_live_system() {
        let mut p = Psi::new();
        p.tick();
        assert!(p.cpu_centi.is_some(), "cpu pressure unreadable");
        assert!(p.mem_centi.is_some(), "memory pressure unreadable");
        assert!(p.io_centi.is_some(), "io pressure unreadable");
    }
}
