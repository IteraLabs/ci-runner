# ci-runner

Terminal UI and other Tools for self-hosted CI jobs running on small devices

## citop

Single-binary terminal dashboard for a self-hosted GitHub Actions runner. Reads
`/proc` and `/sys` directly and holds every sampled file open for the process
lifetime, so a refresh costs a handful of `pread` calls.

Measured on a Raspberry Pi 4 Model B (Ubuntu 24.04, aarch64), 1 Hz refresh:

| | citop | htop |
|---|---|---|
| CPU, 45 s average | 0.64 % of one core | 4.36 % of one core |
| Resident memory | 2.4 MB | 3.9 MB |
| Stripped binary | 595 KB | 399 KB |
| Threads | 1 | 2 |

### Display

| Field | Source |
|---|---|
| Name | `uname(2)` nodename |
| Operating system | `/etc/os-release` PRETTY_NAME, `uname(2)` release and machine |
| Hardware model | `/proc/device-tree/model` |
| System capacity | `sysconf(_SC_NPROCESSORS_ONLN)`, cpufreq policy limits, `MemTotal`, `statvfs` |
| Jobs running | count of `Runner.Worker` processes in the runner's systemd cgroup |
| Last job | newest `Worker_*.log` filename in the runner's `_diag` directory |
| CPU speed | `cpufreq/policy0/scaling_cur_freq` |
| CPU load | per-core `/proc/stat` deltas, `/proc/loadavg` |
| Temperature | `/sys/class/thermal/thermal_zone0/temp` |
| RAM usage | `/proc/meminfo`, `MemTotal - MemAvailable` |
| Disk usage | `statvfs("/")` |
| Ethernet I/O | `/proc/net/dev` counter deltas |
| Wireless I/O | `/proc/net/dev` counter deltas |

Interfaces are discovered from `/sys/class/net`; loopback, bridge, docker and
veth devices are excluded. Wireless is identified by the presence of
`wireless` or `phy80211`.

### Build

Requires a stable Rust toolchain. Build natively on the target device.

```bash
cargo build --release
```

The binary lands at `target/release/citop`.

### Run

```bash
./target/release/citop
```

Accepts an optional refresh period in milliseconds, minimum 100, default 1000.

```bash
./target/release/citop 2000
```

Keys: `q` or `Esc` to quit, `r` to refresh immediately.

### Design notes

The refresh loop blocks in `epoll` via crossterm's event poll with a deadline,
so the process leaves the run queue between frames rather than spinning.

Bars are drawn from integer arithmetic. No float ever reaches a `Display`
implementation, which keeps `flt2dec` out of the binary. Ratatui's `Gauge` is
deliberately unused for the same reason: its default label path formats a float
unconditionally.

Job counting compares `/proc/<pid>/comm` for exact equality against
`Runner.Worker`, scoped to the PIDs in the runner service's `cgroup.procs`.
`comm` truncates at 15 characters, so `Runner.PluginHost` appears as
`Runner.PluginHo`; a prefix match counts it as a job. A substring match over
`/proc/<pid>/cmdline` additionally matches the scanning process itself. Both are
avoided. The full `/proc` scan remains as a fallback for runners started outside
a systemd unit.

Delta-derived values render as `--` until a second sample exists. Network
counters reset to zero when an interface is recreated, so a decrease suppresses
the rate for that frame rather than reporting a spike.

`ratatui::run` restores the terminal on normal return, on error, and on panic.
Signals bypass that path, so `SIGTERM`, `SIGHUP`, `SIGINT` and `SIGQUIT` are
caught by a handler that restores the saved termios, leaves the alternate
screen, and re-raises with the default disposition.

One row per logical core is drawn from the `cpuN` lines of `/proc/stat`; the
aggregate percentage, shared clock and load average sit in the block title. All
four cores fit alongside the other meters at 80x24. On a machine with more cores
than the terminal has rows, the per-core rows collapse back to a single
aggregate row rather than truncating the panels below.

The last job timestamp is read from the newest `Worker_*.log` filename rather
than from its mtime, so it reports when the job started and needs no calendar
arithmetic. Those filenames sort chronologically as plain strings. The scan runs
every ten ticks and immediately after a worker exits.

Release profile uses `opt-level = "z"`, fat LTO, one codegen unit, `panic =
"abort"` and symbol stripping. Measured against `opt-level = "s"` on the target:
529,168 bytes versus 594,704 for the single-CPU-row version.

### Test

```bash
cargo test
```

Parsers are pure functions over byte slices with fixture inputs, so the suite
runs without the hardware it describes.
