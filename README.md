# ci-runner

Terminal UI and other Tools for self-hosted CI jobs running on small devices.

## citop

Single-binary terminal dashboard for a self-hosted GitHub Actions runner.
Reads `/proc` and `/sys` directly. One row per logical core.

```
   citop  circadian-runner
  Ubuntu 24.04.4 LTS 6.8.0-1060-raspi aarch64
  Raspberry Pi 4 Model B Rev 1.4
┌──────────────────────────── capacity ┐┌────────────────────────────── runner ┐
│ cpu       4 x 600-1800 MHz           ││ runner    listening                  │
│ clock     1800 MHz                   ││ jobs      0                          │
│ load      0.95 0.34 0.18             ││ uptime    3h 9m                      │
│ psi       cpu 0.00 mem 0.00 io 2.79  ││ last job  2026-07-30 18:32 UTC       │
│                                      ││ res       161/162 MiB  20 pids       │
└──────────────────────────────────────┘└──────────────────────────────────────┘
┌───────────────────────────────── cpu ┐┌──────────────────────────────── temp ┐
│ cpu_0     [█░░░░░░░░░░░░░░░░]   7%   ││ cpu-temp  [████░░░░░]  43%  53.6 C   │
│ cpu_1     [█████████████████] 100%   ││ fan       OFF                        │
│ cpu_2     [█████████████████] 100%   ││ fan-speed 0 RPM                      │
│ cpu_3     [░░░░░░░░░░░░░░░░░]   0%   ││ throttle  none                       │
└──────────────────────────────────────┘└──────────────────────────────────────┘
┌────────────────────────────────────────────────────────────────────── memory ┐
│ ram       [███░░░░░░░░░░░░░░░░░░░░]  13%  526.7 MiB / 3.6 GiB  cache 3.0 GiB │
│ swap      [░░░░░░░░░░░░░░░░░░░░░░░]   0%  512.0 KiB / 7.9 GiB                │
│ disk      [████████░░░░░░░░░░░░░░░]  37%  20.5 GiB / 57.9 GiB  free 35.0 GiB │
│ disk-io   [██████████████████████░]  97%  mmcblk0  rd 0 B/s  wr 29.9 MiB/s   │
└──────────────────────────────────────────────────────────────────────────────┘
┌────────────────────────────────────────────────────────────────── networking ┐
│ eth0      up    100 Mb  rx     239 B/s  tx       0 B/s  err 0 drop 45360     │
│ wlan0     up        --  rx       0 B/s  tx       0 B/s  err 0 drop 0         │
└──────────────────────────────────────────────────────────────────────────────┘
  q quit   r refresh
```
Fits 80x24, where the header collapses to one line. The full header and the key
hints appear at 26 rows and up. Per-core rows collapse to one aggregate row when
there are more cores than the terminal has room for.

### Measured on the Pi, not estimated

Raspberry Pi 4 Model B, Ubuntu 24.04 aarch64, 1 Hz refresh, 60 s idle average,
both at 80x26.

| | citop | htop |
|---|---|---|
| CPU | 0.83 % of one core | 4.25 % of one core |
| Resident memory | 2.4 MB | 3.9 MB |
| Stripped binary | 595 KB | 399 KB |
| Threads | 1 | 2 |

`opt-level = "z"` beat `"s"` on this target by 64 KiB, measured on device: 529,168
bytes versus 594,704 for the same source. The release profile pairs it with fat
LTO, one codegen unit, `panic = "abort"` and symbol stripping.

### Install

```bash
cargo install ci-runner
```

Build from source on the target device:

```bash
cargo build --release
```

### Run

Local:

```bash
citop
```

Over ethernet:

```bash
ssh circadian-eth -t citop
```

Over wifi:

```bash
ssh circadian-wifi -t citop
```

Variable refresh, in milliseconds, minimum 100, default 1000:

```bash
citop 2000
```

Keys: `q` or `Esc` quit, `r` refresh now.

### Display

| Field | Source |
|---|---|
| Name | `uname(2)` nodename |
| Operating system | `/etc/os-release`, `uname(2)` |
| Hardware model | `/proc/device-tree/model` |
| Capacity | `sysconf(_SC_NPROCESSORS_ONLN)`, cpufreq limits |
| Clock | `cpufreq/policy0/scaling_cur_freq` |
| Load | `/proc/loadavg` |
| psi | `/proc/pressure/{cpu,memory,io}` `some avg10` |
| Runner state | `Runner.Listener` in the service cgroup |
| Jobs | `Runner.Worker` count in the service cgroup |
| Last job | newest `Worker_*.log` name in `_diag` |
| res | service cgroup `memory.current`, `memory.peak`, `pids.current` |
| Uptime | `/proc/uptime` |
| Per-core CPU | `/proc/stat` `cpuN` deltas |
| cpu-temp | `/sys/class/thermal/thermal_zone0/temp` |
| fan | `hwmon` `gpio_fan/pwm1`, falling back to the fan cooling device |
| fan-speed | `hwmon` `gpio_fan/fan1_input`, N/A without a tachometer |
| throttle | `soc:firmware/get_throttled` bitmask, current and since boot |
| RAM, swap | `/proc/meminfo`, `MemTotal - MemAvailable` |
| Disk | `statvfs("/")` |
| disk-io | `/proc/diskstats` sector deltas and `io_ticks` for utilisation |
| Ethernet, wireless | `/proc/net/dev` deltas, rx+tx errors and drops |

Interfaces come from `/sys/class/net`. Loopback, bridge, docker and veth are
excluded. Wireless is identified by `wireless` or `phy80211`.

### Cost

Every sampled file is opened once and re-read with `pread` at offset 0. All
eight sources added for pressure, throttling, disk I/O and cgroup accounting
cost 81 us per cycle measured together on the target, against a 1000 ms tick.
Metrics that change slowly are sampled less often: pressure and cgroup usage
every second tick, throttle flags every fifth, `statvfs` every tenth. Network
errors and drops are free, coming from columns of a line already parsed.

Rendering dominates the cost, not sampling. Growing from one meter block to
four moved the process from 0.64 % to 0.83 % of one core.

### Test

```bash
cargo test
```

Parsers take byte slices and run against fixtures, so the suite passes off-target.

### License

Apache-2.0
