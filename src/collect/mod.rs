// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Raccoglitori: leggono /proc, /sys, /etc e calcolano i dati. Nessun panic su dati di sistema.

pub mod conn;
pub mod cpu;
pub mod disk;
pub mod gpu;
pub mod load;
pub mod mem;
pub mod net;
pub mod procdetail;
pub mod process;
pub mod sensors;
pub mod services;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use conn::ConnInfo;
use cpu::{CpuStats, CpuTimes};
use disk::{DiskInfo, FsMonitor};
use gpu::{GpuCollector, GpuInfo};
use load::{LoadAvg, Uptime};
use mem::MemInfo;
use net::NetInfo;
use process::{ProcPrev, Process};

use crate::source::{CommandRunner, LocalRunner, LocalSource, RemoteStatus, Source};

/// Radici del filesystem da cui si legge (nei test sono cartelle finte) e sorgente delle letture:
/// ogni lettura passa da `src`, ogni comando esterno da `run`.
#[derive(Clone)]
pub struct Roots {
    pub proc: PathBuf,
    pub sys: PathBuf,
    pub etc: PathBuf,
    pub src: Arc<dyn Source>,
    pub run: Arc<dyn CommandRunner>,
}

impl std::fmt::Debug for Roots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Roots")
            .field("proc", &self.proc)
            .field("sys", &self.sys)
            .field("etc", &self.etc)
            .finish_non_exhaustive()
    }
}

impl Roots {
    /// /proc, /sys, /etc della macchina locale.
    pub fn local() -> Roots {
        Roots::with_dirs("/proc".into(), "/sys".into(), "/etc".into())
    }

    /// Radici personalizzate (fixture dei test), sorgente locale.
    pub fn with_dirs(proc: PathBuf, sys: PathBuf, etc: PathBuf) -> Roots {
        Roots {
            proc,
            sys,
            etc,
            src: Arc::new(LocalSource),
            run: Arc::new(LocalRunner),
        }
    }

    /// /proc, /sys, /etc della macchina remota raggiunta da `src` e `run`.
    pub fn remote(src: Arc<dyn Source>, run: Arc<dyn CommandRunner>) -> Roots {
        Roots {
            src,
            run,
            ..Roots::local()
        }
    }
}

impl Default for Roots {
    fn default() -> Self {
        Roots::local()
    }
}

/// Stato complessivo del sistema: ultimi dati letti + campione precedente per le differenze.
pub struct System {
    pub roots: Roots,
    /// Tick al secondo (USER_HZ); modificabile nei test.
    pub clock_ticks: u64,
    /// Dimensione pagina in byte; modificabile nei test.
    pub page_size: u64,
    pub cpu: CpuStats,
    pub mem: MemInfo,
    pub load: LoadAvg,
    pub uptime: Uptime,
    pub processes: Vec<Process>,
    pub net: NetInfo,
    pub gpu: GpuInfo,
    gpu_collector: GpuCollector,
    pub disks: DiskInfo,
    fs_monitor: FsMonitor,
    /// Connessioni di rete: raccolte solo se `collect_conns` (sezione Rete e dischi visibile).
    pub conns: ConnInfo,
    pub collect_conns: bool,
    conn_tick: u64,
    /// Temperatura CPU in °C, se leggibile.
    pub cpu_temp: Option<f64>,
    /// OGGI SEMPRE FALSO (codice per il passo 8): nessuna sezione lo attiva; l'I/O del processo
    /// selezionato lo legge `procdetail`. Leggere /proc/[pid]/io (costoso): serve solo alle colonne I/O (sezione Processi).
    pub want_io: bool,
    /// ZRAM: (dati originali, dati compressi) in byte, se presente.
    pub zram: Option<(u64, u64)>,
    temp_path: Option<Option<PathBuf>>,
    refresh_count: u64,
    users: HashMap<u32, String>,
    prev_cpu: Option<CpuTimes>,
    prev_cores: HashMap<usize, CpuTimes>,
    prev_procs: HashMap<u32, ProcPrev>,
    last_refresh: Option<Instant>,
}

impl System {
    pub fn new(roots: Roots) -> Self {
        let users = process::load_users(&*roots.src, &roots.etc.join("passwd"));
        let mut s = System {
            clock_ticks: rustix::param::clock_ticks_per_second().max(1),
            page_size: (rustix::param::page_size() as u64).max(1),
            cpu: CpuStats::default(),
            mem: MemInfo::default(),
            load: LoadAvg::default(),
            uptime: Uptime::default(),
            processes: Vec::new(),
            net: NetInfo::default(),
            gpu: GpuInfo::default(),
            gpu_collector: GpuCollector::default(),
            disks: DiskInfo::default(),
            // Con radici diverse da /proc (--roots, test) niente statvfs sul sistema vero.
            fs_monitor: if roots.proc == std::path::Path::new("/proc") {
                FsMonitor::real(Arc::clone(&roots.src))
            } else {
                FsMonitor::from_file(
                    Arc::clone(&roots.src),
                    roots
                        .proc
                        .parent()
                        .map_or_else(|| "statvfs.json".into(), |d| d.join("statvfs.json")),
                )
            },
            conns: ConnInfo::default(),
            collect_conns: false,
            conn_tick: 0,
            cpu_temp: None,
            want_io: false,
            zram: None,
            temp_path: None,
            refresh_count: 0,
            users,
            prev_cpu: None,
            prev_cores: HashMap::new(),
            prev_procs: HashMap::new(),
            last_refresh: None,
            roots,
        };
        s.cpu.model = cpu::read_model(&*s.roots.src, &s.roots.proc.join("cpuinfo"));
        let (logical, physical) = cpu::read_topology(&*s.roots.src, &s.roots.proc.join("cpuinfo"));
        s.cpu.logical = logical;
        s.cpu.physical = physical;
        s
    }

    /// Stato della connessione se la sorgente è remota, `None` in locale.
    pub fn remote_status(&self) -> Option<RemoteStatus> {
        self.roots.src.remote_status()
    }

    /// Rilegge tutto; il tempo trascorso dal refresh precedente è misurato col clock reale.
    pub fn refresh(&mut self) {
        self.roots.src.begin_refresh();
        let now = Instant::now();
        let dt = self
            .last_refresh
            .map(|t| now.duration_since(t).as_secs_f64())
            .unwrap_or(0.0);
        self.last_refresh = Some(now);
        self.refresh_with_dt(dt);
    }

    /// Come `refresh` ma con il tempo trascorso (secondi) dato; `dt <= 0` → velocità a zero.
    pub fn refresh_with_dt(&mut self, dt: f64) {
        self.refresh_cpu();
        self.mem = mem::read_meminfo(&*self.roots.src, &self.roots.proc.join("meminfo"));
        self.load = load::read_loadavg(&*self.roots.src, &self.roots.proc.join("loadavg"));
        self.uptime = load::read_uptime(&*self.roots.src, &self.roots.proc.join("uptime"));
        self.refresh_processes(dt);
        self.refresh_gpu(dt);
        net::refresh(&mut self.net, &self.roots, dt);
        disk::refresh_io(&mut self.disks, &self.roots, dt);
        // Filesystem: statvfs gira in thread separati (può bloccarsi su NFS); si chiede una
        // nuova misura ogni 5 giri e si raccolgono le risposte già arrivate.
        let first = self.refresh_count == 0;
        if self.refresh_count.is_multiple_of(5) {
            self.fs_monitor.set_mounts(disk::read_mounts(&self.roots));
            self.fs_monitor.request();
        }
        self.fs_monitor.poll(if first {
            Duration::from_millis(300)
        } else {
            Duration::ZERO
        });
        self.disks.filesystems = self.fs_monitor.snapshot(Instant::now());
        // Connessioni: costose, solo a sezione visibile e al massimo ogni 2 giri.
        if self.collect_conns {
            if self.conn_tick.is_multiple_of(2) {
                let starts: HashMap<u32, u64> = self
                    .processes
                    .iter()
                    .map(|p| (p.pid, p.start_ticks))
                    .collect();
                conn::refresh_with(&mut self.conns, &self.roots, Some(&starts));
            }
            self.conn_tick += 1;
        } else {
            self.conn_tick = 0;
        }
        let (src, sys) = (&*self.roots.src, &self.roots.sys);
        let path = self
            .temp_path
            .get_or_insert_with(|| sensors::find_cpu_temp_path(src, sys));
        self.cpu_temp = path.as_deref().and_then(|p| sensors::read_temp(src, p));
        self.zram = read_zram(src, sys);
        self.refresh_count += 1;
    }

    fn refresh_gpu(&mut self, dt: f64) {
        let pids: Vec<gpu::PidInfo> = self
            .processes
            .iter()
            .map(|p| gpu::PidInfo {
                pid: p.pid,
                start_ticks: p.start_ticks,
                kernel: p.pid == 2 || p.ppid == 2,
            })
            .collect();
        self.gpu_collector
            .refresh(&mut self.gpu, &self.roots, &pids, dt);
        // GPU% e memoria per processo (somma sui dispositivi).
        let mut by_pid: HashMap<u32, (f64, u64)> = HashMap::new();
        for p in &self.gpu.procs {
            let e = by_pid.entry(p.pid).or_insert((0.0, 0));
            e.0 += p.gpu_pct;
            e.1 += p.mem;
        }
        for p in &mut self.processes {
            if let Some((pct, mem)) = by_pid.get(&p.pid) {
                p.gpu_percent = pct.min(100.0);
                p.gpu_mem = *mem;
            }
        }
    }

    fn refresh_cpu(&mut self) {
        let (total, cores) = cpu::read_stat(&*self.roots.src, &self.roots.proc.join("stat"));
        self.cpu.total_usage = match (&self.prev_cpu, &total) {
            (Some(p), Some(t)) => cpu::usage_between(p, t),
            _ => 0.0,
        };
        let mut out = Vec::with_capacity(cores.len());
        let mut new_prev = HashMap::new();
        for (id, t) in cores {
            let usage = self
                .prev_cores
                .get(&id)
                .map(|p| cpu::usage_between(p, &t))
                .unwrap_or(0.0);
            let freq_mhz = cpu::read_freq_mhz(&*self.roots.src, &self.roots.sys, id);
            out.push(cpu::CoreStats {
                id,
                usage,
                freq_mhz,
            });
            new_prev.insert(id, t);
        }
        self.cpu.cores = out;
        self.prev_cores = new_prev;
        if total.is_some() {
            self.prev_cpu = total;
        }
    }

    fn refresh_processes(&mut self, dt: f64) {
        // Core online (righe di /proc/stat); logici di cpuinfo solo come ripiego.
        let ncores = if self.cpu.cores.is_empty() {
            self.cpu.logical.max(1)
        } else {
            self.cpu.cores.len()
        } as f64;
        let list = process::read_all(
            &*self.roots.src,
            &self.roots.proc,
            self.clock_ticks,
            self.page_size,
            &self.users,
            &self.prev_procs,
            dt,
            ncores,
            self.want_io,
            self.refresh_count,
        );
        let io = self.want_io;
        self.prev_procs = list
            .iter()
            .map(|p| (p.pid, ProcPrev::from_process(p, io)))
            .collect();
        self.processes = list;
    }
}

/// Somma di orig_data_size e compr_data_size di tutti i dispositivi zram.
fn read_zram(src: &dyn Source, sys: &Path) -> Option<(u64, u64)> {
    let mut tot = (0u64, 0u64);
    let mut found = false;
    let block = sys.join("block");
    for n in src.read_dir(&block).ok()? {
        if !n.to_string_lossy().starts_with("zram") {
            continue;
        }
        let Some(t) = src.read_text(&block.join(&n).join("mm_stat")) else {
            continue;
        };
        let f: Vec<u64> = t
            .split_whitespace()
            .filter_map(|x| x.parse().ok())
            .collect();
        if let (Some(o), Some(c)) = (f.first(), f.get(1)) {
            tot = (tot.0 + o, tot.1 + c);
            found = true;
        }
    }
    (found && tot.0 > 0).then_some(tot)
}
