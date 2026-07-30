use crate::probe::{Probe, fields, lines, parse_i64, parse_u64, read_trimmed};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct Counters {
    pub rx: u64,
    pub tx: u64,
    pub errs: u64,
    pub drops: u64,
}

pub fn parse_net_dev(data: &[u8], want: &str) -> Option<Counters> {
    for line in lines(data).skip(2) {
        let colon = line.iter().position(|&b| b == b':')?;
        let name = std::str::from_utf8(&line[..colon]).ok()?.trim();
        if name != want {
            continue;
        }
        let v: Vec<u64> = fields(&line[colon + 1..])
            .take(12)
            .filter_map(parse_u64)
            .collect();
        if v.len() < 12 {
            return None;
        }
        return Some(Counters {
            rx: v[0],
            tx: v[8],
            errs: v[2] + v[10],
            drops: v[3] + v[11],
        });
    }
    None
}

pub fn is_wireless(name: &str) -> bool {
    std::path::Path::new(&format!("/sys/class/net/{name}/wireless")).exists()
        || std::path::Path::new(&format!("/sys/class/net/{name}/phy80211")).exists()
}

pub fn is_virtual(name: &str) -> bool {
    name == "lo"
        || name.starts_with("docker")
        || name.starts_with("veth")
        || name.starts_with("br-")
        || name.starts_with("virbr")
}

pub fn link_speed(name: &str) -> Option<u64> {
    let raw = std::fs::read(format!("/sys/class/net/{name}/speed")).ok()?;
    match parse_i64(&raw) {
        Some(v) if v > 0 => Some(v as u64),
        _ => None,
    }
}

pub fn operstate(name: &str) -> String {
    read_trimmed(&format!("/sys/class/net/{name}/operstate")).unwrap_or_else(|| "?".into())
}

pub fn discover() -> (Option<String>, Option<String>) {
    let mut eth = None;
    let mut wlan = None;
    let Ok(dir) = std::fs::read_dir("/sys/class/net") else {
        return (None, None);
    };
    let mut names: Vec<String> = dir
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !is_virtual(n))
        .collect();
    names.sort();
    for n in names {
        if is_wireless(&n) {
            if wlan.is_none() {
                wlan = Some(n);
            }
        } else if eth.is_none() {
            eth = Some(n);
        }
    }
    (eth, wlan)
}

pub struct Iface {
    pub name: String,
    pub speed: Option<u64>,
    pub state: String,
    pub rx_rate: Option<u64>,
    pub tx_rate: Option<u64>,
    pub errs: u64,
    pub drops: u64,
    pub present: bool,
    prev: Option<Counters>,
}

impl Iface {
    pub fn new(name: String) -> Self {
        Self {
            speed: link_speed(&name),
            state: operstate(&name),
            name,
            rx_rate: None,
            tx_rate: None,
            errs: 0,
            drops: 0,
            present: true,
            prev: None,
        }
    }

    fn update(&mut self, data: &[u8], dt_ms: u64) {
        let Some(cur) = parse_net_dev(data, &self.name) else {
            self.present = false;
            self.rx_rate = None;
            self.tx_rate = None;
            self.prev = None;
            return;
        };
        self.present = true;
        self.errs = cur.errs;
        self.drops = cur.drops;
        if let Some(prev) = self.prev {
            if cur.rx < prev.rx || cur.tx < prev.tx || dt_ms == 0 {
                self.rx_rate = None;
                self.tx_rate = None;
            } else {
                self.rx_rate = Some((cur.rx - prev.rx).saturating_mul(1000) / dt_ms);
                self.tx_rate = Some((cur.tx - prev.tx).saturating_mul(1000) / dt_ms);
            }
        }
        self.prev = Some(cur);
    }
}

pub struct NetSource {
    dev: Probe,
    pub eth: Option<Iface>,
    pub wlan: Option<Iface>,
    slow: u8,
}

impl NetSource {
    pub fn new() -> Self {
        let (e, w) = discover();
        Self {
            dev: Probe::open("/proc/net/dev", 16384),
            eth: e.map(Iface::new),
            wlan: w.map(Iface::new),
            slow: 0,
        }
    }

    pub fn tick(&mut self, dt_ms: u64) {
        if !self.dev.refresh() {
            return;
        }
        let data = self.dev.data();
        if let Some(i) = self.eth.as_mut() {
            i.update(data, dt_ms);
        }
        if let Some(i) = self.wlan.as_mut() {
            i.update(data, dt_ms);
        }
        if self.slow == 0 {
            if let Some(i) = self.eth.as_mut() {
                i.state = operstate(&i.name);
                i.speed = link_speed(&i.name);
            }
            if let Some(i) = self.wlan.as_mut() {
                i.state = operstate(&i.name);
                i.speed = link_speed(&i.name);
            }
            self.slow = 5;
        }
        self.slow -= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEV: &[u8] = b"Inter-|   Receive                    |  Transmit\n\
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n\
    lo:   12674     131    0    0    0     0          0         0    12674     131    0    0    0     0       0          0\n\
  eth0: 2645248   26119    0 18533    0     0          0       928   338186     995    0    0    0     0       0          0\n\
 wlan0: 1235004    6216    0    0    0     0          0       492    64509     404    0    0    0     0       0          0\n\
docker0:       0       0    0    0    0     0          0         0        0       0    0    0    0     0       0          0\n";

    #[test]
    fn parse_net_dev_picks_rx_and_tx_bytes_by_column() {
        let e = parse_net_dev(DEV, "eth0").unwrap();
        assert_eq!((e.rx, e.tx), (2_645_248, 338_186));
        let w = parse_net_dev(DEV, "wlan0").unwrap();
        assert_eq!((w.rx, w.tx), (1_235_004, 64_509));
    }

    #[test]
    fn parse_net_dev_handles_a_name_that_abuts_its_colon() {
        let d = parse_net_dev(DEV, "docker0").unwrap();
        assert_eq!((d.rx, d.tx), (0, 0));
    }

    #[test]
    fn parse_net_dev_sums_rx_and_tx_errors_and_drops() {
        let e = parse_net_dev(DEV, "eth0").unwrap();
        assert_eq!(e.drops, 18533);
        assert_eq!(e.errs, 0);
        let w = parse_net_dev(DEV, "wlan0").unwrap();
        assert_eq!(w.drops, 0);
    }

    #[test]
    fn parse_net_dev_returns_none_for_a_missing_interface() {
        assert_eq!(parse_net_dev(DEV, "eth1"), None);
        assert_eq!(parse_net_dev(DEV, ""), None);
    }

    #[test]
    fn parse_net_dev_skips_the_two_header_lines() {
        assert_eq!(parse_net_dev(DEV, "face"), None);
        assert_eq!(parse_net_dev(DEV, "Inter-|   Receive"), None);
    }

    #[test]
    fn first_tick_reports_no_rate() {
        let mut i = Iface {
            name: "eth0".into(),
            speed: None,
            state: "up".into(),
            rx_rate: None,
            tx_rate: None,
            errs: 0,
            drops: 0,
            present: true,
            prev: None,
        };
        i.update(DEV, 1000);
        assert_eq!(i.rx_rate, None);
        assert_eq!(i.tx_rate, None);
    }

    #[test]
    fn second_tick_computes_bytes_per_second() {
        let mut i = Iface {
            name: "eth0".into(),
            speed: None,
            state: "up".into(),
            rx_rate: None,
            tx_rate: None,
            errs: 0,
            drops: 0,
            present: true,
            prev: Some(Counters {
                rx: 2_645_248 - 2000,
                tx: 338_186 - 500,
                errs: 0,
                drops: 0,
            }),
        };
        i.update(DEV, 2000);
        assert_eq!(i.rx_rate, Some(1000));
        assert_eq!(i.tx_rate, Some(250));
    }

    #[test]
    fn counter_reset_suppresses_the_rate_instead_of_spiking() {
        let mut i = Iface {
            name: "eth0".into(),
            speed: None,
            state: "up".into(),
            rx_rate: Some(1),
            tx_rate: Some(1),
            errs: 0,
            drops: 0,
            present: true,
            prev: Some(Counters {
                rx: 9_999_999_999,
                tx: 9_999_999_999,
                errs: 0,
                drops: 0,
            }),
        };
        i.update(DEV, 1000);
        assert_eq!(i.rx_rate, None);
        assert_eq!(i.tx_rate, None);
    }

    #[test]
    fn disappearing_interface_clears_state() {
        let mut i = Iface {
            name: "eth9".into(),
            speed: None,
            state: "up".into(),
            rx_rate: Some(5),
            tx_rate: Some(5),
            errs: 0,
            drops: 0,
            present: true,
            prev: Some(Counters {
                rx: 1,
                tx: 1,
                errs: 0,
                drops: 0,
            }),
        };
        i.update(DEV, 1000);
        assert!(!i.present);
        assert_eq!(i.rx_rate, None);
    }

    #[test]
    fn zero_elapsed_time_does_not_divide_by_zero() {
        let mut i = Iface {
            name: "eth0".into(),
            speed: None,
            state: "up".into(),
            rx_rate: None,
            tx_rate: None,
            errs: 0,
            drops: 0,
            present: true,
            prev: Some(Counters {
                rx: 0,
                tx: 0,
                errs: 0,
                drops: 0,
            }),
        };
        i.update(DEV, 0);
        assert_eq!(i.rx_rate, None);
    }

    #[test]
    fn virtual_interfaces_are_excluded_from_discovery() {
        assert!(is_virtual("lo"));
        assert!(is_virtual("docker0"));
        assert!(is_virtual("veth1a2b"));
        assert!(is_virtual("br-abc"));
        assert!(!is_virtual("eth0"));
        assert!(!is_virtual("wlan0"));
    }
}
