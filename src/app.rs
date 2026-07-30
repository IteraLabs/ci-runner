use std::time::Instant;

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
}

impl App {
    pub fn new() -> Self {
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
        Self::new()
    }
}
