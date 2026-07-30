# ci-runner

Terminal UI and other Tools for self-hosted CI jobs running on small devices.

## citop

Single-binary terminal dashboard for a self-hosted GitHub Actions runner. Reads
`/proc` and `/sys` directly. No network access, no child processes, no writes.

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

## Benchmark

Raspberry Pi 4 Model B, Ubuntu 24.04 aarch64, 1 Hz refresh, 60 s idle average,
both tools at 80x26, measured back to back on an otherwise idle machine.

| | citop | htop |
|---|---|---|
| CPU | 0.83 % of one core | 4.25 % of one core |
| Resident memory | 2.4 MB | 3.9 MB |
| Stripped binary | 595 KB | 399 KB |
| Threads | 1 | 2 |
| Syscalls per second | 31 | not measured |

Reproduce the CPU figure:

```bash
P=$(pgrep -x citop); read _ _ _ _ _ _ _ _ _ _ _ _ _ U1 S1 _ < /proc/$P/stat; sleep 60; read _ _ _ _ _ _ _ _ _ _ _ _ _ U2 S2 _ < /proc/$P/stat; echo "scale=3; 100*(($U2-$U1)+($S2-$S1))/100/60" | bc
```

`opt-level = "z"` beat `"s"` on this target by 64 KiB, measured on device:
529,168 bytes versus 594,704 for the same source. The release profile pairs it
with fat LTO, one codegen unit, `panic = "abort"` and symbol stripping.

## Metrics

| Metric | Source | What it tells you |
|---|---|---|
| name | `uname(2)` nodename | Which runner you are looking at, when several are open side by side |
| os | `/etc/os-release`, `uname(2)` | Distro, kernel and architecture a job will build against |
| model | `/proc/device-tree/model` | Board revision, which sets the thermal and I/O ceiling |
| cpu | `sysconf`, cpufreq limits | Core count and clock range, the ceiling for job parallelism |
| clock | `cpufreq/policy0/scaling_cur_freq` | Current clock. Stuck at minimum under load means thermal or power limiting |
| load | `/proc/loadavg` | Run queue depth. Above the core count, jobs are queueing for CPU |
| psi | `/proc/pressure/{cpu,memory,io}` | Percent of time tasks stalled on each resource. Separates busy from starved better than load |
| runner | `Runner.Listener` in the service cgroup | Whether the runner is connected and able to accept work at all |
| jobs | `Runner.Worker` count in the service cgroup | How many jobs are executing right now |
| uptime | `/proc/uptime` | Host uptime, to compare against runner uptime when diagnosing restarts |
| last job | newest `Worker_*.log` name in `_diag` | When work last arrived. A stale value means jobs are not routed here |
| res | service cgroup `memory.current`, `memory.peak`, `pids.current` | What the runner and its job consume, separate from the rest of the box |
| cpu_N | `/proc/stat` `cpuN` deltas | Per-core utilisation. One core pinned while others idle means a serial build step |
| cpu-temp | `/sys/class/thermal/thermal_zone0/temp` | SoC temperature across the 30-85 C range |
| fan | `hwmon` `gpio_fan/pwm1` | Whether active cooling is engaged |
| fan-speed | `hwmon` `gpio_fan/fan1_input` | Fan RPM, or N/A where there is no tachometer |
| throttle | `soc:firmware/get_throttled` | Undervoltage, frequency capping, thermal throttling and soft temperature limit, current and since boot. A marginal power supply degrades builds with no other symptom |
| ram | `/proc/meminfo`, `MemTotal - MemAvailable` | Headroom before the OOM killer starts ending jobs |
| swap | `/proc/meminfo` | Swap in use. A sustained value on an SD card destroys build times |
| disk | `statvfs("/")` | Free space. A full disk fails checkout and artifact upload |
| disk-io | `/proc/diskstats` sectors and `io_ticks` | Throughput and device utilisation. Near 100 % means storage is the bottleneck |
| eth0, wlan0 | `/proc/net/dev` deltas | Throughput per interface, plus cumulative errors and drops. Rising drops explain flaky checkouts |

Interfaces come from `/sys/class/net`. Loopback, bridge, docker and veth are
excluded. Wireless is identified by `wireless` or `phy80211`.

Each metric is sampled at the rate it changes: disk I/O and per-core CPU every
tick, pressure and cgroup usage every second tick, throttle flags every fifth,
`statvfs` every tenth. Every sampled file is opened once at startup and re-read
with `pread` at offset 0.

## Install

### From a release

```bash
curl -fsSL https://raw.githubusercontent.com/IteraLabs/ci-runner/main/install.sh | sh
```

Downloads the binary for your architecture, verifies its SHA-256 against the
published checksum, and installs to `~/.local/bin/citop`. Override with
`CITOP_PREFIX`, `CITOP_TAG` or `CITOP_REPO`.

The same by hand:

```bash
curl -fsSLO https://github.com/IteraLabs/ci-runner/releases/latest/download/citop-aarch64-unknown-linux-gnu
curl -fsSLO https://github.com/IteraLabs/ci-runner/releases/latest/download/citop-aarch64-unknown-linux-gnu.sha256
sha256sum -c citop-aarch64-unknown-linux-gnu.sha256
```

### From source

```bash
cargo install --git https://github.com/IteraLabs/ci-runner
```

### Clone and run

```bash
git clone https://github.com/IteraLabs/ci-runner
cd ci-runner
cargo build --release
./target/release/citop
```

## Run

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

Keys: `q` or `Esc` quit, `r` refresh now. An interactive terminal is required on
both stdin and stdout; it exits with status 2 otherwise.

## Verifying what it does

The claims below are properties you can check yourself, not assurances.

### 1. Confirm it makes no network calls and writes nothing

```bash
strace -f -o /tmp/citop.trace citop
```

Quit with `q`, then:

```bash
grep -cE '\b(socket|connect|bind|sendto|sendmsg|recvfrom)\(' /tmp/citop.trace
grep -cE '\b(execve|fork|clone|clone3)\(' /tmp/citop.trace
grep 'openat(' /tmp/citop.trace | grep -vc O_RDONLY
grep -oE 'write\([0-9]+' /tmp/citop.trace | sort -u
```

Expected: `0` network syscalls, `1` process spawn (the `execve` of citop
itself), `0` opens without `O_RDONLY`, and writes only to file descriptor 1.

Every path it opens appears in the Metrics table, plus `/dev/tty` for terminal
size and the C library. List them from your own trace:

```bash
grep -oE 'openat\(AT_FDCWD, "[^"]+"' /tmp/citop.trace | cut -d'"' -f2 | sort -u
```

### 2. Confirm the syscall rate

```bash
sudo timeout 10 strace -c -p "$(pgrep -x citop)"
```

Expected on an idle host: about 310 syscalls over 10 seconds, dominated by
`pread64`. A busy loop would show orders of magnitude more.

### 3. Check the dependency tree for advisories

```bash
cargo install cargo-audit && cargo audit
```

Or query the same database without installing anything:

```bash
python3 - <<'EOF'
import json, re, urllib.request
pairs = re.findall(r'name = "([^"]+)"\nversion = "([^"]+)"', open('Cargo.lock').read())
q = {"queries": [{"package": {"name": n, "ecosystem": "crates.io"}, "version": v} for n, v in pairs]}
r = urllib.request.Request("https://api.osv.dev/v1/querybatch", data=json.dumps(q).encode(),
                           headers={"Content-Type": "application/json"})
res = json.load(urllib.request.urlopen(r))
print(sum(len(x.get("vulns", [])) for x in res["results"]), "advisories")
EOF
```

The dependency set is `ratatui` with default features off, and `libc`. The build
graph contains no HTTP client, no TLS stack, no serialisation framework and no
async runtime.

### 4. Verify a downloaded binary

```bash
sha256sum -c citop-aarch64-unknown-linux-gnu.sha256
```

Releases are built by `.github/workflows/release.yml` and carry a signed SLSA
provenance attestation naming the commit and workflow that produced them:

```bash
gh attestation verify citop-aarch64-unknown-linux-gnu --repo IteraLabs/ci-runner
```

### 5. Reproduce the binary yourself

The highest-trust path is to not use a published binary at all:

```bash
git clone https://github.com/IteraLabs/ci-runner
cd ci-runner && cargo build --release --locked
sha256sum target/release/citop
```

`--locked` builds against the committed `Cargo.lock`, so the dependency versions
are the ones audited above.

### What it needs, and what it does not

It runs as an unprivileged user and needs no root, no capabilities, no group
membership and no setuid bit. The one path it cannot read as a normal user,
`cpufreq/policy0/cpuinfo_cur_freq`, is deliberately avoided in favour of its
world-readable sibling `scaling_cur_freq`.

It never reads `~/actions-runner/.credentials`, `.credentials_rsaparams` or
`.runner`. From the runner directory it reads the file *names* in `_diag` and
the process list in the service cgroup, nothing else.

## Runner setup

The self-hosted runner needs its Rust toolchain isolated from the host's, because
`Swatinem/rust-cache` restores `$CARGO_HOME/bin` and will otherwise overwrite the
host `rustup` with a copy that does not contain it, leaving every shim dangling.

`scripts/setup-runner.sh` is idempotent and does the whole thing:

```bash
sh scripts/setup-runner.sh
```

| What it sets | Value |
|---|---|
| `~/actions-runner/.env` | `CARGO_HOME=$HOME/ci-toolchain/cargo`, `RUSTUP_HOME=$HOME/ci-toolchain/rustup` |
| `~/actions-runner/.path` | prepends `$HOME/ci-toolchain/cargo/bin` |
| host toolchain | installs `rustup` with `stable` plus clippy and rustfmt, only if absent |
| runner service | restarts it, only when `.env` or `.path` changed |

Jobs then resolve `cargo` from the CI toolchain, and nothing a workflow does can
reach `~/.cargo`. Override the locations with `RUNNER_ROOT`, `CI_ROOT` and
`RUNNER_SERVICE`.

Workflows in this repo pair `dtolnay/rust-toolchain` with
`Swatinem/rust-cache` set to `cache-bin: false`, so citop's own jobs cannot do
to another runner what this setup defends against.

Confirm the host survived a job that used the cache:

```bash
test -x ~/.cargo/bin/rustup && echo "host toolchain INTACT" || echo "host toolchain CLOBBERED"
```

## Test

```bash
cargo test
```

Parsers take byte slices and run against fixtures, so the suite passes off-target.

## License

Apache-2.0
