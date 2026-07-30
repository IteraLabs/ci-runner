use crate::probe::{lines, parse_u64};

pub const WORKER: &str = "Runner.Worker";
pub const LISTENER: &str = "Runner.Listener";

pub fn comm_is(data: &[u8], want: &str) -> bool {
    let end = data
        .iter()
        .position(|&b| b == b'\n' || b == 0)
        .unwrap_or(data.len());
    &data[..end] == want.as_bytes()
}

pub fn parse_cgroup_path(data: &[u8]) -> Option<String> {
    for line in lines(data) {
        if let Some(rest) = line.strip_prefix(b"0::") {
            let s = std::str::from_utf8(rest).ok()?.trim();
            if s.is_empty() || s == "/" {
                return None;
            }
            return Some(format!("/sys/fs/cgroup{s}/cgroup.procs"));
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

pub struct Jobs {
    cgroup: Option<String>,
    pub running: u32,
    pub listener: bool,
    countdown: u8,
}

const EVERY: u8 = 2;

impl Default for Jobs {
    fn default() -> Self {
        Self::new()
    }
}

impl Jobs {
    pub fn new() -> Self {
        let mut j = Self {
            cgroup: None,
            running: 0,
            listener: false,
            countdown: 0,
        };
        j.bootstrap();
        j
    }

    fn bootstrap(&mut self) {
        let (workers, listener, pid) = scan_proc();
        self.running = workers;
        self.listener = listener;
        self.cgroup = pid
            .and_then(|p| std::fs::read(format!("/proc/{p}/cgroup")).ok())
            .and_then(|d| parse_cgroup_path(&d))
            .filter(|p| std::path::Path::new(p).exists());
    }

    fn count_via_cgroup(&self) -> Option<(u32, bool)> {
        let raw = std::fs::read(self.cgroup.as_ref()?).ok()?;
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
        if self.countdown > 0 {
            self.countdown -= 1;
            return;
        }
        self.countdown = EVERY;
        match self.count_via_cgroup() {
            Some((w, true)) => {
                self.running = w;
                self.listener = true;
            }
            _ => self.bootstrap(),
        }
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
            "/sys/fs/cgroup/system.slice/actions.runner.IteraLabs.circadian-runner.service/cgroup.procs"
        );
    }

    #[test]
    fn parse_cgroup_path_rejects_root_and_v1_only_output() {
        assert!(parse_cgroup_path(b"0::/\n").is_none());
        assert!(parse_cgroup_path(b"12:pids:/user.slice\n").is_none());
        assert!(parse_cgroup_path(b"").is_none());
    }

    #[test]
    fn state_reflects_listener_and_worker_presence() {
        let mut j = Jobs {
            cgroup: None,
            running: 0,
            listener: false,
            countdown: 0,
        };
        assert_eq!(j.state(), "offline");
        j.listener = true;
        assert_eq!(j.state(), "listening");
        j.running = 2;
        assert_eq!(j.state(), "busy");
    }
}
