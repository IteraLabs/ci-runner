use crate::probe::{lines, parse_u64, read_u64};

pub const WORKER: &str = "Runner.Worker";
pub const LISTENER: &str = "Runner.Listener";

pub fn comm_is(data: &[u8], want: &str) -> bool {
    let end = data
        .iter()
        .position(|&b| b == b'\n' || b == 0)
        .unwrap_or(data.len());
    &data[..end] == want.as_bytes()
}

pub fn parse_worker_stamp(name: &str) -> Option<String> {
    let core = name.strip_prefix("Worker_")?.strip_suffix("-utc.log")?;
    let (date, time) = core.split_once('-')?;
    if date.len() != 8
        || time.len() != 6
        || !date.bytes().chain(time.bytes()).all(|b| b.is_ascii_digit())
    {
        return None;
    }
    Some(format!(
        "{}-{}-{} {}:{} UTC",
        &date[0..4],
        &date[4..6],
        &date[6..8],
        &time[0..2],
        &time[2..4]
    ))
}

fn newest_worker_log(root: &str) -> Option<String> {
    let dir = std::fs::read_dir(format!("{root}/_diag")).ok()?;
    let mut best: Option<String> = None;
    for e in dir.flatten() {
        let name = e.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with("Worker_") || !name.ends_with(".log") {
            continue;
        }
        if best.as_deref().is_none_or(|b| name > b) {
            best = Some(name.to_string());
        }
    }
    best
}

pub fn parse_cgroup_path(data: &[u8]) -> Option<String> {
    for line in lines(data) {
        if let Some(rest) = line.strip_prefix(b"0::") {
            let s = std::str::from_utf8(rest).ok()?.trim();
            if s.is_empty() || s == "/" {
                return None;
            }
            return Some(format!("/sys/fs/cgroup{s}"));
        }
    }
    None
}

fn read_comm(pid: u64) -> Option<Vec<u8>> {
    std::fs::read(format!("/proc/{pid}/comm")).ok()
}

fn scan_proc() -> (u32, bool, Option<u64>) {
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return (0, false, None);
    };
    let mut workers = 0u32;
    let mut listener = None;
    for entry in dir.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.as_bytes().first().is_some_and(u8::is_ascii_digit) {
            continue;
        }
        let Some(pid) = parse_u64(name.as_bytes()) else {
            continue;
        };
        let Some(comm) = read_comm(pid) else { continue };
        if comm_is(&comm, WORKER) {
            workers += 1;
        } else if comm_is(&comm, LISTENER) {
            listener = Some(pid);
        }
    }
    (workers, listener.is_some(), listener)
}

#[derive(Clone, Copy, Default)]
pub struct Res {
    pub mem: u64,
    pub mem_peak: u64,
    pub pids: u64,
}

pub struct Jobs {
    cgroup: Option<String>,
    root: Option<String>,
    pub running: u32,
    pub listener: bool,
    pub last_job: Option<String>,
    pub res: Option<Res>,
    countdown: u8,
    log_countdown: u8,
}

const EVERY: u8 = 2;
const LOG_EVERY: u8 = 10;

impl Default for Jobs {
    fn default() -> Self {
        Self::new()
    }
}

impl Jobs {
    pub fn new() -> Self {
        let mut j = Self {
            cgroup: None,
            root: None,
            running: 0,
            listener: false,
            last_job: None,
            res: None,
            countdown: 0,
            log_countdown: 0,
        };
        j.bootstrap();
        j.refresh_last_job();
        j.read_res();
        j
    }

    fn bootstrap(&mut self) {
        let (workers, listener, pid) = scan_proc();
        self.running = workers;
        self.listener = listener;
        self.cgroup = pid
            .and_then(|p| std::fs::read(format!("/proc/{p}/cgroup")).ok())
            .and_then(|d| parse_cgroup_path(&d))
            .filter(|p| std::path::Path::new(&format!("{p}/cgroup.procs")).exists());
        self.root = pid
            .and_then(|p| std::fs::read_link(format!("/proc/{p}/cwd")).ok())
            .map(|p| p.to_string_lossy().into_owned());
    }

    fn refresh_last_job(&mut self) {
        let Some(root) = self.root.as_deref() else {
            return;
        };
        if let Some(stamp) = newest_worker_log(root)
            .as_deref()
            .and_then(parse_worker_stamp)
        {
            self.last_job = Some(stamp);
        }
    }

    fn read_res(&mut self) {
        let Some(dir) = self.cgroup.as_deref() else {
            return;
        };
        let g = |f: &str| read_u64(&format!("{dir}/{f}"));
        self.res = g("memory.current").map(|mem| Res {
            mem,
            mem_peak: g("memory.peak").unwrap_or(mem),
            pids: g("pids.current").unwrap_or(0),
        });
    }

    fn count_via_cgroup(&self) -> Option<(u32, bool)> {
        let raw = std::fs::read(format!("{}/cgroup.procs", self.cgroup.as_ref()?)).ok()?;
        let mut workers = 0u32;
        let mut listener = false;
        for line in lines(&raw) {
            let Some(pid) = parse_u64(line) else { continue };
            let Some(comm) = read_comm(pid) else { continue };
            if comm_is(&comm, WORKER) {
                workers += 1;
            } else if comm_is(&comm, LISTENER) {
                listener = true;
            }
        }
        Some((workers, listener))
    }

    pub fn tick(&mut self) {
        if self.log_countdown == 0 {
            self.refresh_last_job();
            self.log_countdown = LOG_EVERY;
        }
        self.log_countdown -= 1;

        if self.countdown > 0 {
            self.countdown -= 1;
            return;
        }
        self.countdown = EVERY;
        let before = self.running;
        match self.count_via_cgroup() {
            Some((w, true)) => {
                self.running = w;
                self.listener = true;
            }
            _ => self.bootstrap(),
        }
        if before > 0 && self.running == 0 {
            self.refresh_last_job();
        }
        self.read_res();
    }

    pub fn state(&self) -> &'static str {
        if !self.listener {
            "offline"
        } else if self.running > 0 {
            "busy"
        } else {
            "listening"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comm_is_matches_exactly_and_ignores_the_newline() {
        assert!(comm_is(b"Runner.Worker\n", WORKER));
        assert!(comm_is(b"Runner.Worker", WORKER));
        assert!(comm_is(b"Runner.Listener\n", LISTENER));
    }

    #[test]
    fn comm_is_rejects_the_truncated_plugin_host() {
        assert!(!comm_is(b"Runner.PluginHo\n", WORKER));
        assert!(!comm_is(b"Runner.PluginHo\n", LISTENER));
    }

    #[test]
    fn comm_is_rejects_prefixes_and_superstrings() {
        assert!(!comm_is(b"Runner.Worker2\n", WORKER));
        assert!(!comm_is(b"Runner\n", WORKER));
        assert!(!comm_is(b"", WORKER));
        assert!(!comm_is(b"Runner.Listener\n", WORKER));
    }

    #[test]
    fn parse_cgroup_path_builds_the_unified_hierarchy_path() {
        let d = b"0::/system.slice/actions.runner.IteraLabs.circadian-runner.service\n";
        assert_eq!(
            parse_cgroup_path(d).unwrap(),
            "/sys/fs/cgroup/system.slice/actions.runner.IteraLabs.circadian-runner.service"
        );
    }

    #[test]
    fn parse_cgroup_path_rejects_root_and_v1_only_output() {
        assert!(parse_cgroup_path(b"0::/\n").is_none());
        assert!(parse_cgroup_path(b"12:pids:/user.slice\n").is_none());
        assert!(parse_cgroup_path(b"").is_none());
    }

    #[test]
    fn parse_worker_stamp_formats_the_filename_timestamp() {
        assert_eq!(
            parse_worker_stamp("Worker_20260730-183213-utc.log").unwrap(),
            "2026-07-30 18:32 UTC"
        );
        assert_eq!(
            parse_worker_stamp("Worker_20251201-000000-utc.log").unwrap(),
            "2025-12-01 00:00 UTC"
        );
    }

    #[test]
    fn parse_worker_stamp_rejects_other_log_names() {
        assert!(parse_worker_stamp("Runner_20260730-183213-utc.log").is_none());
        assert!(parse_worker_stamp("Worker_20260730-183213.log").is_none());
        assert!(parse_worker_stamp("Worker_2026073-183213-utc.log").is_none());
        assert!(parse_worker_stamp("Worker_abcdefgh-183213-utc.log").is_none());
        assert!(parse_worker_stamp("").is_none());
    }

    #[test]
    fn worker_log_names_sort_chronologically_as_strings() {
        let mut v = [
            "Worker_20260730-065647-utc.log",
            "Worker_20251201-235959-utc.log",
            "Worker_20260730-183213-utc.log",
        ];
        v.sort();
        assert_eq!(*v.last().unwrap(), "Worker_20260730-183213-utc.log");
    }

    #[test]
    fn state_reflects_listener_and_worker_presence() {
        let mut j = Jobs {
            cgroup: None,
            root: None,
            running: 0,
            listener: false,
            last_job: None,
            res: None,
            countdown: 0,
            log_countdown: 0,
        };
        assert_eq!(j.state(), "offline");
        j.listener = true;
        assert_eq!(j.state(), "listening");
        j.running = 2;
        assert_eq!(j.state(), "busy");
    }
}
