use crate::probe::{Probe, fields, lines, parse_u64, read_trimmed, read_u64};

pub struct Host {
    pub nodename: String,
    pub os: String,
    pub kernel: String,
    pub arch: String,
    pub model: String,
    pub cores: usize,
    pub mhz_min: Option<u64>,
    pub mhz_max: Option<u64>,
}

fn c_field(f: &[libc::c_char]) -> String {
    let bytes = unsafe { std::slice::from_raw_parts(f.as_ptr().cast::<u8>(), f.len()) };
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

pub fn parse_pretty_name(data: &[u8]) -> Option<String> {
    for line in lines(data) {
        if let Some(rest) = line.strip_prefix(b"PRETTY_NAME=") {
            let s = std::str::from_utf8(rest).ok()?.trim();
            return Some(s.trim_matches('"').to_string());
        }
    }
    None
}

pub fn parse_dt_model(raw: &[u8]) -> String {
    let end = raw
        .iter()
        .position(|&b| b == 0 || b == b'\n')
        .unwrap_or(raw.len());
    String::from_utf8_lossy(&raw[..end]).trim().to_string()
}

pub fn parse_uptime(data: &[u8]) -> Option<u64> {
    let line = lines(data).next()?;
    let first = fields(line).next()?;
    let dot = first.iter().position(|&b| b == b'.').unwrap_or(first.len());
    parse_u64(&first[..dot])
}

impl Host {
    pub fn probe() -> Self {
        let mut u: libc::utsname = unsafe { std::mem::zeroed() };
        let ok = unsafe { libc::uname(&mut u) } == 0;
        let nodename = if ok {
            c_field(&u.nodename)
        } else {
            read_trimmed("/proc/sys/kernel/hostname").unwrap_or_else(|| "unknown".into())
        };
        let kernel = if ok {
            c_field(&u.release)
        } else {
            String::new()
        };
        let arch = if ok {
            c_field(&u.machine)
        } else {
            String::new()
        };
        let os = std::fs::read("/etc/os-release")
            .ok()
            .and_then(|d| parse_pretty_name(&d))
            .unwrap_or_else(|| "unknown".into());
        let model = std::fs::read("/proc/device-tree/model")
            .ok()
            .map(|d| parse_dt_model(&d))
            .unwrap_or_default();
        let cores = match unsafe { libc::sysconf(libc::_SC_NPROCESSORS_ONLN) } {
            n if n > 0 => n as usize,
            _ => 1,
        };
        const BASE: &str = "/sys/devices/system/cpu/cpufreq/policy0";
        Self {
            nodename,
            os,
            kernel,
            arch,
            model,
            cores,
            mhz_min: read_u64(&format!("{BASE}/cpuinfo_min_freq")).map(|k| k / 1000),
            mhz_max: read_u64(&format!("{BASE}/cpuinfo_max_freq")).map(|k| k / 1000),
        }
    }
}

pub struct Uptime {
    probe: Probe,
    pub secs: Option<u64>,
}

impl Uptime {
    pub fn new() -> Self {
        Self {
            probe: Probe::open("/proc/uptime", 128),
            secs: None,
        }
    }
    pub fn tick(&mut self) {
        if self.probe.refresh() {
            self.secs = parse_uptime(self.probe.data());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pretty_name_strips_surrounding_quotes() {
        let d = b"NAME=\"Ubuntu\"\nPRETTY_NAME=\"Ubuntu 24.04.4 LTS\"\nID=ubuntu\n";
        assert_eq!(parse_pretty_name(d).unwrap(), "Ubuntu 24.04.4 LTS");
    }

    #[test]
    fn parse_pretty_name_handles_unquoted_values() {
        assert_eq!(
            parse_pretty_name(b"PRETTY_NAME=Alpine\n").unwrap(),
            "Alpine"
        );
    }

    #[test]
    fn parse_pretty_name_returns_none_when_absent() {
        assert!(parse_pretty_name(b"NAME=\"Ubuntu\"\n").is_none());
        assert!(parse_pretty_name(b"").is_none());
    }

    #[test]
    fn parse_dt_model_trims_at_the_nul_not_a_newline() {
        let raw = b"Raspberry Pi 4 Model B Rev 1.4\0";
        assert_eq!(parse_dt_model(raw), "Raspberry Pi 4 Model B Rev 1.4");
    }

    #[test]
    fn parse_dt_model_tolerates_a_missing_terminator() {
        assert_eq!(parse_dt_model(b"Some Board"), "Some Board");
        assert_eq!(parse_dt_model(b""), "");
    }

    #[test]
    fn parse_uptime_takes_the_whole_seconds_of_the_first_field() {
        assert_eq!(parse_uptime(b"4681.79 18583.09\n"), Some(4681));
        assert_eq!(parse_uptime(b"12 34\n"), Some(12));
        assert!(parse_uptime(b"").is_none());
    }

    #[test]
    fn host_probe_reports_a_plausible_machine() {
        let h = Host::probe();
        assert!(!h.nodename.is_empty());
        assert!(h.cores >= 1);
    }
}
