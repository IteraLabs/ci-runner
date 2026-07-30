use crate::probe::{Probe, fields, lines, parse_u64};

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

pub const SECTOR: u64 = 512;

pub fn root_disk_name() -> Option<String> {
    let mounts = std::fs::read("/proc/mounts").ok()?;
    for line in lines(&mounts) {
        let mut it = fields(line);
        let dev = it.next()?;
        let mnt = it.next()?;
        if mnt != b"/" {
            continue;
        }
        let dev = std::str::from_utf8(dev).ok()?;
        let base = dev.rsplit('/').next()?;
        return Some(strip_partition(base));
    }
    None
}

pub fn strip_partition(name: &str) -> String {
    let b = name.as_bytes();
    let mut end = b.len();
    while end > 0 && b[end - 1].is_ascii_digit() {
        end -= 1;
    }
    if end > 0 && end < b.len() && b[end - 1] == b'p' && b[..end - 1].iter().any(u8::is_ascii_digit)
    {
        end -= 1;
    }
    if end == 0 {
        name.to_string()
    } else {
        name[..end].to_string()
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct DiskCounters {
    pub read_sectors: u64,
    pub write_sectors: u64,
    pub io_ms: u64,
}

pub fn parse_diskstats(data: &[u8], want: &str) -> Option<DiskCounters> {
    for line in lines(data) {
        let mut it = fields(line);
        let _major = it.next()?;
        let _minor = it.next()?;
        if it.next()? != want.as_bytes() {
            continue;
        }
        let v: Vec<u64> = it.take(10).filter_map(parse_u64).collect();
        if v.len() < 10 {
            return None;
        }
        return Some(DiskCounters {
            read_sectors: v[2],
            write_sectors: v[6],
            io_ms: v[9],
        });
    }
    None
}

pub struct DiskIo {
    probe: Probe,
    name: Option<String>,
    prev: Option<DiskCounters>,
    pub read_bps: Option<u64>,
    pub write_bps: Option<u64>,
    pub util: Option<u16>,
}

impl Default for DiskIo {
    fn default() -> Self {
        Self::new()
    }
}

impl DiskIo {
    pub fn new() -> Self {
        Self {
            probe: Probe::open("/proc/diskstats", 16384),
            name: root_disk_name(),
            prev: None,
            read_bps: None,
            write_bps: None,
            util: None,
        }
    }

    pub fn device(&self) -> &str {
        self.name.as_deref().unwrap_or("--")
    }

    pub fn tick(&mut self, dt_ms: u64) {
        let Some(name) = self.name.as_deref() else {
            return;
        };
        if !self.probe.refresh() {
            return;
        }
        let Some(cur) = parse_diskstats(self.probe.data(), name) else {
            return;
        };
        if let Some(p) = self.prev {
            if dt_ms == 0
                || cur.read_sectors < p.read_sectors
                || cur.write_sectors < p.write_sectors
            {
                self.read_bps = None;
                self.write_bps = None;
                self.util = None;
            } else {
                let rd = (cur.read_sectors - p.read_sectors).saturating_mul(SECTOR);
                let wr = (cur.write_sectors - p.write_sectors).saturating_mul(SECTOR);
                self.read_bps = Some(rd.saturating_mul(1000) / dt_ms);
                self.write_bps = Some(wr.saturating_mul(1000) / dt_ms);
                let busy = cur.io_ms.saturating_sub(p.io_ms);
                self.util = Some(crate::fmt::pct(busy, dt_ms));
            }
        }
        self.prev = Some(cur);
    }
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

    const DISKSTATS: &[u8] = b" 179       0 mmcblk0 73808 14004 10649174 274785 22425 34199 2609932 740017 0 213246 1014803 0 0 0 0 0 0\n 179       2 mmcblk0p2 73000 14000 10000000 270000 22000 34000 2600000 740000 0 213000 1014000 0 0 0 0 0 0\n";

    #[test]
    fn strip_partition_handles_mmc_nvme_and_sata() {
        assert_eq!(strip_partition("mmcblk0p2"), "mmcblk0");
        assert_eq!(strip_partition("nvme0n1p2"), "nvme0n1");
        assert_eq!(strip_partition("sda1"), "sda");
        assert_eq!(strip_partition("sda"), "sda");
        assert_eq!(strip_partition("vda15"), "vda");
    }

    #[test]
    fn parse_diskstats_reads_the_whole_disk_not_the_partition() {
        let c = parse_diskstats(DISKSTATS, "mmcblk0").unwrap();
        assert_eq!(c.read_sectors, 10_649_174);
        assert_eq!(c.write_sectors, 2_609_932);
        assert_eq!(c.io_ms, 213_246);
    }

    #[test]
    fn parse_diskstats_returns_none_for_an_absent_device() {
        assert!(parse_diskstats(DISKSTATS, "sda").is_none());
        assert!(parse_diskstats(b"", "mmcblk0").is_none());
    }

    #[test]
    fn disk_io_first_tick_has_no_rate() {
        let mut d = DiskIo {
            probe: Probe::open("/nonexistent", 16),
            name: Some("mmcblk0".into()),
            prev: None,
            read_bps: None,
            write_bps: None,
            util: None,
        };
        d.prev = parse_diskstats(DISKSTATS, "mmcblk0");
        assert_eq!(d.read_bps, None);
    }

    #[test]
    fn root_disk_resolves_on_a_live_system() {
        let n = root_disk_name().expect("root device must resolve");
        assert!(!n.is_empty());
        assert!(!n.ends_with(|c: char| c.is_ascii_digit() && n.len() > 3 && n.contains('p')));
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
