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
│ cpu      4 x 600-1800 MHz            ││ runner   listening                   │
│ clock    1800 MHz                    ││ jobs     0                           │
│ load     0.49 0.31 0.28              ││ uptime   2h 25m                      │
│ tasks    3 runnable / 233            ││ last job 2026-07-30 18:32 UTC        │
└──────────────────────────────────────┘└──────────────────────────────────────┘
┌───────────────────────────────────────────────────────────────────────── cpu ┐
│ cpu_0  [░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░]   0%   │
│ cpu_1  [████████████████████████████████████████████████████████████] 100%   │
│ cpu_2  [██░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░]   4%   │
│ cpu_3  [████████████████████████████████████████████████████████████] 100%   │
│ temp   [██████████░░░░░░░░░░░░░░]  42%  53.1 C  fan OFF                      │
│ ram    [███░░░░░░░░░░░░░░░░░░░░░]  13%  525.0 MiB / 3.6 GiB  cache 3.0 GiB   │
│ swap   [░░░░░░░░░░░░░░░░░░░░░░░░]   0%  512.0 KiB / 7.9 GiB                  │
│ disk   [████████░░░░░░░░░░░░░░░░]  37%  20.4 GiB / 57.9 GiB  free 35.0 GiB   │
└──────────────────────────────────────────────────────────────────────────────┘
┌───────────────────────────────────────────────────────────────────── network ┐
│ eth0   up    100 Mb  rx      239 B/s  tx        0 B/s                        │
│ wlan0  up        --  rx        0 B/s  tx        0 B/s                        │
└──────────────────────────────────────────────────────────────────────────────┘
  q quit   r refresh
```

Fits 80x24. Per-core rows collapse to one aggregate row on shorter terminals.

### Measured on the Pi, not estimated

Raspberry Pi 4 Model B, Ubuntu 24.04 aarch64, 1 Hz refresh, 45 s average.

| | citop | htop |
|---|---|---|
| CPU | 0.64 % of one core | 4.36 % of one core |
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
| Load, tasks | `/proc/loadavg` |
| Runner state | `Runner.Listener` in the service cgroup |
| Jobs | `Runner.Worker` count in the service cgroup |
| Last job | newest `Worker_*.log` name in `_diag` |
| Uptime | `/proc/uptime` |
| Per-core CPU | `/proc/stat` `cpuN` deltas |
| Temperature | `/sys/class/thermal/thermal_zone0/temp` |
| Fan | `hwmon` `gpio_fan/pwm1`, falling back to the fan cooling device |
| RAM, swap | `/proc/meminfo`, `MemTotal - MemAvailable` |
| Disk | `statvfs("/")` |
| Ethernet, wireless | `/proc/net/dev` deltas |

Interfaces come from `/sys/class/net`. Loopback, bridge, docker and veth are
excluded. Wireless is identified by `wireless` or `phy80211`.

### Test

```bash
cargo test
```

Parsers take byte slices and run against fixtures, so the suite passes off-target.

### License

Apache-2.0
