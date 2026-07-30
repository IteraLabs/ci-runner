use crate::probe::{Probe, lines, parse_u64};

#[derive(Clone, Copy, Default)]
pub struct MemInfo {
    pub total: u64,
    pub available: u64,
    pub buffcache: u64,
    pub swap_total: u64,
    pub swap_free: u64,
}

impl MemInfo {
    pub fn used(&self) -> u64 {
        self.total.saturating_sub(self.available)
    }
    pub fn swap_used(&self) -> u64 {
        self.swap_total.saturating_sub(self.swap_free)
    }
}

fn kb_value(line: &[u8], label: &[u8]) -> Option<u64> {
    let rest = line.strip_prefix(label)?;
    let start = rest.iter().position(|b| b.is_ascii_digit())?;
    parse_u64(&rest[start..]).map(|kb| kb.saturating_mul(1024))
}

pub fn parse_meminfo(data: &[u8]) -> Option<MemInfo> {
    let mut m = MemInfo::default();
    let mut buffers = 0u64;
    let mut cached = 0u64;
    let mut reclaim = 0u64;
    let mut seen_total = false;
    for line in lines(data) {
        if let Some(v) = kb_value(line, b"MemTotal:") {
            m.total = v;
            seen_total = true;
        } else if let Some(v) = kb_value(line, b"MemAvailable:") {
            m.available = v;
        } else if let Some(v) = kb_value(line, b"Buffers:") {
            buffers = v;
        } else if let Some(v) = kb_value(line, b"Cached:") {
            cached = v;
        } else if let Some(v) = kb_value(line, b"SReclaimable:") {
            reclaim = v;
        } else if let Some(v) = kb_value(line, b"SwapTotal:") {
            m.swap_total = v;
        } else if let Some(v) = kb_value(line, b"SwapFree:") {
            m.swap_free = v;
        }
    }
    if !seen_total {
        return None;
    }
    m.buffcache = buffers + cached + reclaim;
    Some(m)
}

#[derive(Clone, Copy, Default)]
pub struct Disk {
    pub total: u64,
    pub used: u64,
    pub avail: u64,
}

impl Disk {
    pub fn percent(&self) -> u16 {
        let denom = self.used.saturating_add(self.avail);
        if denom == 0 {
            return 0;
        }
        let p = (self.used.saturating_mul(100)).div_ceil(denom);
        if p > 100 { 100 } else { p as u16 }
    }
}

pub fn statvfs_root() -> Option<Disk> {
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::statvfs(c"/".as_ptr(), &mut s) };
    if rc != 0 {
        return None;
    }
    let frsize = if s.f_frsize > 0 {
        s.f_frsize as u64
    } else {
        s.f_bsize as u64
    };
    let blocks = s.f_blocks as u64;
    let bfree = s.f_bfree as u64;
    let bavail = s.f_bavail as u64;
    Some(Disk {
        total: blocks.saturating_mul(frsize),
        used: blocks.saturating_sub(bfree).saturating_mul(frsize),
        avail: bavail.saturating_mul(frsize),
    })
}

pub struct MemSource {
    meminfo: Probe,
    pub info: Option<MemInfo>,
    pub disk: Option<Disk>,
    disk_countdown: u8,
}

const DISK_EVERY: u8 = 10;

impl MemSource {
    pub fn new() -> Self {
        Self {
            meminfo: Probe::open("/proc/meminfo", 8192),
            info: None,
            disk: None,
            disk_countdown: 0,
        }
    }

    pub fn tick(&mut self) {
        if self.meminfo.refresh() {
            self.info = parse_meminfo(self.meminfo.data());
        }
        if self.disk_countdown == 0 {
            self.disk = statvfs_root();
            self.disk_countdown = DISK_EVERY;
        }
        self.disk_countdown -= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MEMINFO: &[u8] = b"MemTotal:        3874892 kB\n\
MemFree:         2066732 kB\n\
MemAvailable:    3005216 kB\n\
Buffers:           44584 kB\n\
Cached:          1025988 kB\n\
SwapCached:            0 kB\n\
SReclaimable:      56240 kB\n\
SwapTotal:       8388604 kB\n\
SwapFree:        8388604 kB\n";

    #[test]
    fn parse_meminfo_converts_kb_to_bytes() {
        let m = parse_meminfo(MEMINFO).unwrap();
        assert_eq!(m.total, 3_874_892 * 1024);
        assert_eq!(m.available, 3_005_216 * 1024);
        assert_eq!(m.swap_total, 8_388_604 * 1024);
    }

    #[test]
    fn used_follows_the_procps_available_formula() {
        let m = parse_meminfo(MEMINFO).unwrap();
        assert_eq!(m.used(), (3_874_892 - 3_005_216) * 1024);
    }

    #[test]
    fn used_does_not_use_the_legacy_free_formula() {
        let m = parse_meminfo(MEMINFO).unwrap();
        let legacy: u64 = (3_874_892u64 - 2_066_732 - 44_584 - 1_025_988 - 56_240) * 1024;
        assert_eq!(legacy, 681_348 * 1024);
        assert_ne!(m.used(), legacy);
    }

    #[test]
    fn buffcache_sums_buffers_cached_and_sreclaimable() {
        let m = parse_meminfo(MEMINFO).unwrap();
        assert_eq!(m.buffcache, (44_584 + 1_025_988 + 56_240) * 1024);
    }

    #[test]
    fn swap_used_is_total_minus_free_without_swapcached() {
        let m = parse_meminfo(MEMINFO).unwrap();
        assert_eq!(m.swap_used(), 0);
    }

    #[test]
    fn cached_label_does_not_capture_swapcached() {
        let m = parse_meminfo(b"MemTotal: 100 kB\nSwapCached: 999 kB\nCached: 7 kB\n").unwrap();
        assert_eq!(m.buffcache, 7 * 1024);
    }

    #[test]
    fn parse_meminfo_requires_memtotal() {
        assert!(parse_meminfo(b"MemFree: 12 kB\n").is_none());
        assert!(parse_meminfo(b"").is_none());
    }

    #[test]
    fn disk_percent_ceilings_against_used_plus_available() {
        let d = Disk {
            used: (15_197_424 - 9_950_108) * 4096,
            avail: 9_319_025 * 4096,
            total: 15_197_424 * 4096,
        };
        assert_eq!(d.percent(), 37);
    }

    #[test]
    fn disk_percent_guards_empty_filesystem() {
        assert_eq!(Disk::default().percent(), 0);
    }

    #[test]
    fn statvfs_root_succeeds_on_a_live_system() {
        let d = statvfs_root().expect("statvfs on / must succeed");
        assert!(d.total > 0);
        assert!(d.used <= d.total);
    }
}
