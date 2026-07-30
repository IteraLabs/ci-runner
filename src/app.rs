use std::time::{Duration, Instant};

use crate::cpu::CpuSource;
use crate::host::{Host, Uptime};
use crate::jobs::Jobs;
use crate::mem::{DiskIo, MemSource};
use crate::net::NetSource;
use crate::psi::Psi;
use crate::therm::{Fan, Therm, Throttle};

pub struct App {
    pub host: Host,
    pub cpu: CpuSource,
    pub mem: MemSource,
    pub net: NetSource,
    pub therm: Therm,
    pub fan: Fan,
    pub throttle: Throttle,
    pub psi: Psi,
    pub diskio: DiskIo,
    pub jobs: Jobs,
    pub uptime: Uptime,
    pub ticks: u64,
    last: Instant,
    floor_ms: u64,
}

pub fn min_dt_ms(tick: Duration) -> u64 {
    (tick.as_millis() as u64 / 2).max(1)
}

pub fn effective_dt(dt_ms: u64, floor_ms: u64) -> u64 {
    if dt_ms < floor_ms { 0 } else { dt_ms }
}

impl App {
    pub fn new(tick: Duration) -> Self {
        Self {
            host: Host::probe(),
            cpu: CpuSource::new(),
            mem: MemSource::new(),
            net: NetSource::new(),
            therm: Therm::new(),
            fan: Fan::new(),
            throttle: Throttle::new(),
            psi: Psi::new(),
            diskio: DiskIo::new(),
            jobs: Jobs::new(),
            uptime: Uptime::new(),
            ticks: 0,
            last: Instant::now(),
            floor_ms: min_dt_ms(tick),
        }
    }

    pub fn tick(&mut self) {
        let now = Instant::now();
        let dt_ms = effective_dt(
            now.duration_since(self.last).as_millis() as u64,
            self.floor_ms,
        );
        self.last = now;
        self.cpu.tick();
        self.mem.tick();
        self.net.tick(dt_ms);
        self.therm.tick();
        self.fan.tick();
        self.throttle.tick();
        self.psi.tick();
        self.diskio.tick(dt_ms);
        self.jobs.tick();
        self.uptime.tick();
        self.ticks = self.ticks.saturating_add(1);
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new(Duration::from_millis(1000))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T1S: Duration = Duration::from_millis(1000);

    #[test]
    fn the_floor_is_half_the_configured_interval() {
        assert_eq!(min_dt_ms(T1S), 500);
        assert_eq!(min_dt_ms(Duration::from_millis(2000)), 1000);
        assert_eq!(min_dt_ms(Duration::from_millis(100)), 50);
    }

    #[test]
    fn the_floor_is_never_zero() {
        assert_eq!(min_dt_ms(Duration::from_millis(1)), 1);
        assert_eq!(min_dt_ms(Duration::from_millis(0)), 1);
    }

    #[test]
    fn a_scheduled_tick_is_a_valid_rate_window() {
        let f = min_dt_ms(T1S);
        assert_eq!(effective_dt(1000, f), 1000);
        assert_eq!(effective_dt(f, f), f);
    }

    #[test]
    fn a_manual_refresh_shortly_after_a_tick_yields_no_window() {
        let f = min_dt_ms(T1S);
        assert_eq!(effective_dt(20, f), 0);
        assert_eq!(effective_dt(300, f), 0);
        assert_eq!(effective_dt(f - 1, f), 0);
    }

    #[test]
    fn a_fast_interval_still_measures_its_own_ticks() {
        let f = min_dt_ms(Duration::from_millis(100));
        assert_eq!(effective_dt(100, f), 100);
        assert_eq!(effective_dt(10, f), 0);
    }

    #[test]
    fn zero_stays_zero() {
        assert_eq!(effective_dt(0, min_dt_ms(T1S)), 0);
    }
}
