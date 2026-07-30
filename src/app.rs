use std::time::Instant;

use crate::cpu::CpuSource;
use crate::host::{Host, Uptime};
use crate::jobs::Jobs;
use crate::mem::MemSource;
use crate::net::NetSource;
use crate::therm::Therm;

pub struct App {
    pub host: Host,
    pub cpu: CpuSource,
    pub mem: MemSource,
    pub net: NetSource,
    pub therm: Therm,
    pub jobs: Jobs,
    pub uptime: Uptime,
    pub ticks: u64,
    last: Instant,
}

impl App {
    pub fn new() -> Self {
        Self {
            host: Host::probe(),
            cpu: CpuSource::new(),
            mem: MemSource::new(),
            net: NetSource::new(),
            therm: Therm::new(),
            jobs: Jobs::new(),
            uptime: Uptime::new(),
            ticks: 0,
            last: Instant::now(),
        }
    }

    pub fn tick(&mut self) {
        let now = Instant::now();
        let dt_ms = now.duration_since(self.last).as_millis() as u64;
        self.last = now;
        self.cpu.tick();
        self.mem.tick();
        self.net.tick(dt_ms);
        self.therm.tick();
        self.jobs.tick();
        self.uptime.tick();
        self.ticks = self.ticks.saturating_add(1);
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
