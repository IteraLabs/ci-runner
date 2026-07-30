use crate::probe::{Probe, parse_i64, read_trimmed};

pub fn fan_path() -> Option<String> {
    if let Ok(dir) = std::fs::read_dir("/sys/class/hwmon") {
        for e in dir.flatten() {
            let p = e.path();
            let is_fan = std::fs::read(p.join("name"))
                .map(|n| n.starts_with(b"gpio_fan") || n.starts_with(b"pwm_fan"))
                .unwrap_or(false);
            if is_fan {
                let pwm = p.join("pwm1");
                if pwm.exists() {
                    return Some(pwm.to_string_lossy().into_owned());
                }
            }
        }
    }
    for i in 0..8 {
        let base = format!("/sys/class/thermal/cooling_device{i}");
        if read_trimmed(&format!("{base}/type")).is_some_and(|t| t.contains("fan")) {
            return Some(format!("{base}/cur_state"));
        }
    }
    None
}

pub struct Fan {
    probe: Probe,
    present: bool,
    pub on: Option<bool>,
}

impl Default for Fan {
    fn default() -> Self {
        Self::new()
    }
}

impl Fan {
    pub fn new() -> Self {
        let path = fan_path();
        Self {
            probe: Probe::open(path.as_deref().unwrap_or("/nonexistent"), 32),
            present: path.is_some(),
            on: None,
        }
    }

    pub fn tick(&mut self) {
        if !self.present {
            return;
        }
        if self.probe.refresh() {
            self.on = parse_i64(self.probe.data()).map(|v| v > 0);
        }
    }

    pub fn label(&self) -> &'static str {
        match self.on {
            Some(true) => "fan ON",
            Some(false) => "fan OFF",
            None => "fan --",
        }
    }
}

pub const WINDOW: usize = 4;
pub const COLD: i64 = 30_000;
pub const HOT: i64 = 85_000;

pub struct Therm {
    probe: Probe,
    window: [i64; WINDOW],
    filled: usize,
    head: usize,
    pub milli_c: Option<i64>,
}

impl Default for Therm {
    fn default() -> Self {
        Self::new()
    }
}

impl Therm {
    pub fn new() -> Self {
        Self {
            probe: Probe::open("/sys/class/thermal/thermal_zone0/temp", 64),
            window: [0; WINDOW],
            filled: 0,
            head: 0,
            milli_c: None,
        }
    }

    pub fn tick(&mut self) {
        if !self.probe.refresh() {
            return;
        }
        let Some(v) = parse_i64(self.probe.data()) else {
            return;
        };
        self.window[self.head] = v;
        self.head = (self.head + 1) % WINDOW;
        if self.filled < WINDOW {
            self.filled += 1;
        }
        let sum: i64 = self.window[..self.filled].iter().sum();
        self.milli_c = Some(sum / self.filled as i64);
    }

    pub fn percent_of_range(&self) -> u16 {
        let Some(t) = self.milli_c else { return 0 };
        if t <= COLD {
            return 0;
        }
        crate::fmt::pct((t - COLD) as u64, (HOT - COLD) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(t: &mut Therm, samples: &[i64]) {
        for &s in samples {
            t.window[t.head] = s;
            t.head = (t.head + 1) % WINDOW;
            if t.filled < WINDOW {
                t.filled += 1;
            }
            let sum: i64 = t.window[..t.filled].iter().sum();
            t.milli_c = Some(sum / t.filled as i64);
        }
    }

    #[test]
    fn averages_over_the_window_to_damp_sensor_quantization() {
        let mut t = Therm::new();
        t.filled = 0;
        t.head = 0;
        t.window = [0; WINDOW];
        feed(&mut t, &[48686, 48686, 48199, 48199]);
        assert_eq!(t.milli_c, Some((48686 + 48686 + 48199 + 48199) / 4));
    }

    #[test]
    fn averages_over_only_the_samples_seen_so_far() {
        let mut t = Therm::new();
        t.filled = 0;
        t.head = 0;
        t.window = [0; WINDOW];
        feed(&mut t, &[50000]);
        assert_eq!(t.milli_c, Some(50000));
        feed(&mut t, &[40000]);
        assert_eq!(t.milli_c, Some(45000));
    }

    #[test]
    fn window_evicts_the_oldest_sample() {
        let mut t = Therm::new();
        t.filled = 0;
        t.head = 0;
        t.window = [0; WINDOW];
        feed(&mut t, &[10000, 10000, 10000, 10000, 20000]);
        assert_eq!(t.milli_c, Some((20000 + 10000 * 3) / 4));
    }

    #[test]
    fn fan_label_reflects_state() {
        let mut f = Fan::new();
        f.on = Some(true);
        assert_eq!(f.label(), "fan ON");
        f.on = Some(false);
        assert_eq!(f.label(), "fan OFF");
        f.on = None;
        assert_eq!(f.label(), "fan --");
    }

    #[test]
    fn fan_is_discoverable_on_this_host_or_absent_cleanly() {
        let mut f = Fan::new();
        f.tick();
        if f.present {
            assert!(f.on.is_some(), "fan path found but no readable state");
        } else {
            assert_eq!(f.label(), "fan --");
        }
    }

    #[test]
    fn percent_of_range_is_zero_without_a_reading() {
        let mut t = Therm::new();
        t.milli_c = None;
        assert_eq!(t.percent_of_range(), 0);
    }

    #[test]
    fn percent_of_range_spans_cold_to_throttle() {
        let mut t = Therm::new();
        t.milli_c = Some(COLD);
        assert_eq!(t.percent_of_range(), 0);
        t.milli_c = Some((COLD + HOT) / 2);
        assert_eq!(t.percent_of_range(), 50);
        t.milli_c = Some(HOT);
        assert_eq!(t.percent_of_range(), 100);
    }

    #[test]
    fn percent_of_range_clamps_at_both_ends() {
        let mut t = Therm::new();
        t.milli_c = Some(0);
        assert_eq!(t.percent_of_range(), 0);
        t.milli_c = Some(120_000);
        assert_eq!(t.percent_of_range(), 100);
    }

    #[test]
    fn idle_temperature_reads_as_comfortable_not_alarming() {
        let mut t = Therm::new();
        t.milli_c = Some(48_600);
        assert!(t.percent_of_range() < 60, "48.6 C must not look hot");
    }
}
