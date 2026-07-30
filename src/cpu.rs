use crate::probe::{Probe, fields, lines, parse_u64};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct CpuTimes {
    pub total: u64,
    pub idle: u64,
}

#[derive(Clone, Copy)]
pub struct Load {
    pub one: u64,
    pub five: u64,
    pub fifteen: u64,
    pub runnable: u64,
    pub threads: u64,
}

pub fn parse_stat(data: &[u8]) -> Option<CpuTimes> {
    for line in lines(data) {
        if !line.starts_with(b"cpu") {
            break;
        }
        if line.len() < 4 || line[3] != b' ' {
            continue;
        }
        let mut total: u64 = 0;
        let mut idle: u64 = 0;
        for (i, f) in fields(line).skip(1).take(8).enumerate() {
            let v = parse_u64(f)?;
            total = total.saturating_add(v);
            if i == 3 || i == 4 {
                idle = idle.saturating_add(v);
            }
        }
        if total == 0 {
            return None;
        }
        return Some(CpuTimes { total, idle });
    }
    None
}

pub fn busy_percent(prev: CpuTimes, cur: CpuTimes) -> Option<u16> {
    let dt = cur.total.saturating_sub(prev.total);
    if dt == 0 {
        return None;
    }
    let di = cur.idle.saturating_sub(prev.idle);
    let busy = dt.saturating_sub(di);
    Some(crate::fmt::pct(busy, dt))
}

fn parse_centi(f: &[u8]) -> Option<u64> {
    let dot = f.iter().position(|&b| b == b'.')?;
    let whole = parse_u64(&f[..dot])?;
    let frac = f.get(dot + 1..dot + 3)?;
    let cents = parse_u64(frac)?;
    whole.checked_mul(100)?.checked_add(cents)
}

pub fn parse_loadavg(data: &[u8]) -> Option<Load> {
    let line = lines(data).next()?;
    let mut it = fields(line);
    let one = parse_centi(it.next()?)?;
    let five = parse_centi(it.next()?)?;
    let fifteen = parse_centi(it.next()?)?;
    let procs = it.next()?;
    let slash = procs.iter().position(|&b| b == b'/')?;
    let runnable = parse_u64(&procs[..slash])?;
    let threads = parse_u64(&procs[slash + 1..])?;
    Some(Load {
        one,
        five,
        fifteen,
        runnable,
        threads,
    })
}

pub struct CpuSource {
    stat: Probe,
    load: Probe,
    freq: Probe,
    prev: Option<CpuTimes>,
    pub percent: Option<u16>,
    pub load_avg: Option<Load>,
    pub mhz: Option<u64>,
}

impl CpuSource {
    pub fn new() -> Self {
        Self {
            stat: Probe::open("/proc/stat", 8192),
            load: Probe::open("/proc/loadavg", 128),
            freq: Probe::open(
                "/sys/devices/system/cpu/cpufreq/policy0/scaling_cur_freq",
                64,
            ),
            prev: None,
            percent: None,
            load_avg: None,
            mhz: None,
        }
    }

    pub fn tick(&mut self) {
        if self.stat.refresh()
            && let Some(cur) = parse_stat(self.stat.data())
        {
            self.percent = self.prev.and_then(|p| busy_percent(p, cur));
            self.prev = Some(cur);
        }
        if self.load.refresh() {
            self.load_avg = parse_loadavg(self.load.data());
        }
        if self.freq.refresh() {
            self.mhz = parse_u64(self.freq.data()).map(|khz| khz / 1000);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAT: &[u8] = b"cpu  4372 12 5070 1858281 2674 0 29 3 91 7\n\
cpu0 979 0 1279 464091 762 0 13 0 0 0\n\
intr 890640 0 7804\n";

    #[test]
    fn parse_stat_reads_the_aggregate_line_not_cpu0() {
        let t = parse_stat(STAT).unwrap();
        assert_eq!(t.total, 4372 + 12 + 5070 + 1858281 + 2674 + 29 + 3);
        assert_eq!(t.idle, 1858281 + 2674);
    }

    #[test]
    fn parse_stat_excludes_guest_fields_to_avoid_double_counting() {
        let t = parse_stat(STAT).unwrap();
        assert!(!format!("{}", t.total).is_empty());
        assert_eq!(t.total, 1870441);
        assert_ne!(t.total, 1870441 + 91 + 7);
    }

    #[test]
    fn parse_stat_treats_iowait_as_idle() {
        let t = parse_stat(b"cpu  10 0 10 100 80 0 0 0\n").unwrap();
        assert_eq!(t.idle, 180);
        assert_eq!(t.total, 200);
    }

    #[test]
    fn parse_stat_rejects_input_without_an_aggregate_line() {
        assert!(parse_stat(b"intr 1 2 3\n").is_none());
        assert!(parse_stat(b"cpu0 1 2 3 4 5 6 7 8\n").is_none());
        assert!(parse_stat(b"").is_none());
    }

    #[test]
    fn busy_percent_computes_from_deltas() {
        let a = CpuTimes {
            total: 1000,
            idle: 900,
        };
        let b = CpuTimes {
            total: 1100,
            idle: 950,
        };
        assert_eq!(busy_percent(a, b), Some(50));
    }

    #[test]
    fn busy_percent_is_none_when_no_time_elapsed() {
        let a = CpuTimes {
            total: 1000,
            idle: 900,
        };
        assert_eq!(busy_percent(a, a), None);
    }

    #[test]
    fn busy_percent_survives_counters_going_backwards() {
        let a = CpuTimes {
            total: 1000,
            idle: 900,
        };
        let b = CpuTimes {
            total: 1100,
            idle: 800,
        };
        assert_eq!(busy_percent(a, b), Some(100));
    }

    #[test]
    fn parse_loadavg_reads_all_five_fields_without_floats() {
        let l = parse_loadavg(b"0.08 0.02 2.34 7/257 2179\n").unwrap();
        assert_eq!(l.one, 8);
        assert_eq!(l.five, 2);
        assert_eq!(l.fifteen, 234);
        assert_eq!(l.runnable, 7);
        assert_eq!(l.threads, 257);
    }

    #[test]
    fn parse_loadavg_rejects_malformed_input() {
        assert!(parse_loadavg(b"").is_none());
        assert!(parse_loadavg(b"0.08 0.02\n").is_none());
        assert!(parse_loadavg(b"0.08 0.02 0.01 7 2179\n").is_none());
    }
}
