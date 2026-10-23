// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! GPU: rilevamento da /sys/class/drm, metriche amdgpu / i915 / xe e uso per processo da fdinfo.
//! Le GPU sospese non vengono interrogate (per non svegliarle); NVIDIA è solo rilevata.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::{Roots, read_text};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GpuState {
    #[default]
    Active,
    /// Discreta sospesa (runtime PM / D3cold): nessun file sysfs del dispositivo viene letto.
    Suspended,
    /// Integrata con runtime PM "suspended": si legge normalmente (non c'è nulla da risparmiare).
    Idle,
    /// Driver non supportato (es. nvidia): solo rilevata.
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkKind {
    #[default]
    None,
    Graphics,
    Compute,
    Video,
    Copy,
}

impl WorkKind {
    pub fn label(self) -> &'static str {
        match self {
            WorkKind::None => "—",
            WorkKind::Graphics => "Graphics",
            WorkKind::Compute => "Compute",
            WorkKind::Video => "Video",
            WorkKind::Copy => "Copy",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Gpu {
    /// "card0".
    pub card: String,
    /// Indirizzo PCI, es. "0000:03:00.0" (chiave del dispositivo).
    pub pdev: String,
    pub vendor: u32,
    pub device: u32,
    pub driver: String,
    pub name: String,
    pub integrated: bool,
    pub state: GpuState,
    /// `power_state` è davvero D3cold (altrimenti è una sospensione di runtime PM).
    pub d3cold: bool,
    /// % 0–100.
    pub usage: Option<f64>,
    /// Occupazione per motore (nome come in fdinfo), % 0–100.
    pub engines: Vec<(String, f64)>,
    pub enc: Option<f64>,
    pub dec: Option<f64>,
    /// Motore video generico (i915: video / video-enhance).
    pub video: Option<f64>,
    pub vram_used: Option<u64>,
    pub vram_total: Option<u64>,
    pub gtt_used: Option<u64>,
    pub gtt_total: Option<u64>,
    /// Memoria di sistema residente usata dai client (i915 / xe).
    pub sys_mem: Option<u64>,
    pub clock_mhz: Option<u64>,
    pub clock_max_mhz: Option<u64>,
    pub mem_clock_mhz: Option<u64>,
    pub temps: Vec<(String, f64)>,
    /// Temperatura principale (giunzione, poi edge, poi la prima).
    pub temp: Option<f64>,
    pub power_w: Option<f64>,
    pub power_cap_w: Option<f64>,
    pub fan_rpm: Option<u64>,
    pub fan_pct: Option<f64>,
    /// "4.0 ×16".
    pub pcie: Option<String>,
    pub pcie_max: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct GpuProc {
    pub pid: u32,
    pub pdev: String,
    pub gpu_pct: f64,
    pub enc_pct: f64,
    pub dec_pct: f64,
    pub kind: WorkKind,
    /// Memoria GPU in byte (VRAM; per le integrate la memoria di sistema residente).
    pub mem: u64,
}

#[derive(Debug, Clone, Default)]
pub struct GpuInfo {
    pub devices: Vec<Gpu>,
    pub procs: Vec<GpuProc>,
}

// ---------------------------------------------------------------- fdinfo

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Counter {
    /// Nanosecondi di lavoro cumulati.
    Ns(u64),
    /// Cicli usati e cicli totali (xe).
    Cycles { used: u64, total: u64 },
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ClientRaw {
    pub driver: String,
    pub pdev: String,
    pub client_id: u64,
    pub engines: HashMap<String, Counter>,
    pub capacity: HashMap<String, u64>,
    /// VRAM: `drm-resident-vram*`/`local*` se presente, altrimenti `drm-memory-vram`.
    pub vram: u64,
    pub gtt: u64,
    pub system: u64,
}

/// "123 KiB" → byte; senza unità → byte.
fn parse_size(v: &str) -> u64 {
    let mut it = v.split_whitespace();
    let n: u64 = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
    let mult = match it.next() {
        Some("KiB") | Some("kB") => 1024,
        Some("MiB") => 1024 * 1024,
        Some("GiB") => 1024 * 1024 * 1024,
        _ => 1,
    };
    n.saturating_mul(mult)
}

fn first_num(v: &str) -> u64 {
    v.split_whitespace()
        .next()
        .and_then(|x| x.parse().ok())
        .unwrap_or(0)
}

/// Interpreta un fdinfo DRM; `None` se non è di un client DRM con indirizzo PCI.
pub fn parse_fdinfo(text: &str) -> Option<ClientRaw> {
    let mut c = ClientRaw::default();
    let mut cycles: HashMap<String, u64> = HashMap::new();
    let mut total_cycles: HashMap<String, u64> = HashMap::new();
    let mut seen_id = false;
    // Le due famiglie di chiavi (resident / memory) non vanno sommate: si usa resident se c'è.
    let mut res_vram: Option<u64> = None;
    let mut mem_vram = 0u64;
    for l in text.lines() {
        let Some((k, v)) = l.split_once(':') else {
            continue;
        };
        let v = v.trim();
        match k {
            "drm-driver" => c.driver = v.to_string(),
            "drm-pdev" => c.pdev = v.to_string(),
            "drm-client-id" => {
                c.client_id = v.parse().ok()?;
                seen_id = true;
            }
            "drm-memory-vram" => mem_vram = mem_vram.saturating_add(parse_size(v)),
            "drm-memory-gtt" => c.gtt = c.gtt.saturating_add(parse_size(v)),
            _ => {
                if let Some(e) = k.strip_prefix("drm-engine-capacity-") {
                    c.capacity.insert(e.to_string(), first_num(v).max(1));
                } else if let Some(e) = k.strip_prefix("drm-engine-") {
                    c.engines.insert(e.to_string(), Counter::Ns(first_num(v)));
                } else if let Some(e) = k.strip_prefix("drm-total-cycles-") {
                    total_cycles.insert(e.to_string(), first_num(v));
                } else if let Some(e) = k.strip_prefix("drm-cycles-") {
                    cycles.insert(e.to_string(), first_num(v));
                } else if let Some(r) = k.strip_prefix("drm-resident-") {
                    if r.starts_with("vram") || r.starts_with("local") {
                        res_vram = Some(res_vram.unwrap_or(0).saturating_add(parse_size(v)));
                    } else if r.starts_with("system") {
                        c.system = c.system.saturating_add(parse_size(v));
                    }
                }
            }
        }
    }
    c.vram = res_vram.unwrap_or(mem_vram);
    for (e, used) in cycles {
        let total = total_cycles.get(&e).copied().unwrap_or(0);
        c.engines.insert(e, Counter::Cycles { used, total });
    }
    (seen_id && !c.pdev.is_empty()).then_some(c)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineKind {
    Gfx,
    Compute,
    Copy,
    Enc,
    Dec,
    Video,
}

pub fn engine_kind(name: &str) -> EngineKind {
    let n = name.to_ascii_lowercase();
    if n.starts_with("enc") {
        EngineKind::Enc
    } else if n.starts_with("dec") || n.starts_with("vcn") || n.starts_with("jpeg") {
        EngineKind::Dec
    } else if n.starts_with("video") || n.starts_with("vcs") || n.starts_with("vecs") {
        EngineKind::Video
    } else if n.starts_with("compute") || n.starts_with("ccs") {
        EngineKind::Compute
    } else if n.starts_with("copy")
        || n.starts_with("dma")
        || n.starts_with("bcs")
        || n.starts_with("sdma")
    {
        EngineKind::Copy
    } else {
        EngineKind::Gfx
    }
}

/// Occupazione % di un motore tra due campioni dello stesso client.
fn engine_pct(prev: Counter, now: Counter, dt: f64, capacity: u64) -> f64 {
    let v = match (prev, now) {
        (Counter::Ns(a), Counter::Ns(b)) if dt > 0.0 => {
            b.saturating_sub(a) as f64 / (dt * 1e9) / capacity.max(1) as f64 * 100.0
        }
        (
            Counter::Cycles {
                used: ua,
                total: ta,
            },
            Counter::Cycles {
                used: ub,
                total: tb,
            },
        ) => {
            let dtot = tb.saturating_sub(ta);
            if dtot == 0 {
                0.0
            } else {
                ub.saturating_sub(ua) as f64 / dtot as f64 * 100.0
            }
        }
        _ => 0.0,
    };
    v.clamp(0.0, 100.0)
}

fn counter_total(c: Counter) -> u64 {
    match c {
        Counter::Ns(n) => n,
        Counter::Cycles { used, .. } => used,
    }
}

// ---------------------------------------------------------------- sysfs

fn rd_u64(p: &Path) -> Option<u64> {
    read_text(p)?.trim().parse().ok()
}

fn rd_hex(p: &Path) -> Option<u32> {
    u32::from_str_radix(read_text(p)?.trim().trim_start_matches("0x"), 16).ok()
}

fn uevent_value(text: &str, key: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
        .map(|v| v.trim().to_string())
}

/// Nome dal database pci.ids: "[nome commerciale]" se presente, con il costruttore davanti.
pub fn pci_name(text: &str, vendor: u32, device: u32) -> Option<String> {
    let vkey = format!("{vendor:04x}  ");
    let dkey = format!("{device:04x}  ");
    let mut in_vendor = false;
    let mut vname = String::new();
    for l in text.lines() {
        if l.starts_with('#') || l.is_empty() {
            continue;
        }
        if !l.starts_with('\t') {
            in_vendor = l.starts_with(&vkey);
            if in_vendor {
                vname = l[vkey.len()..].trim().to_string();
            }
        } else if in_vendor && !l.starts_with("\t\t") && l[1..].starts_with(&dkey) {
            let full = l[1 + dkey.len()..].trim();
            let inner = match (full.find('['), full.rfind(']')) {
                (Some(a), Some(b)) if b > a => &full[a + 1..b],
                _ => full,
            };
            let short = match vendor {
                0x1002 | 0x1022 => "AMD",
                0x8086 => "Intel",
                0x10de => "NVIDIA",
                _ => vname.split_whitespace().next().unwrap_or(""),
            };
            let first = inner.split_whitespace().next().unwrap_or("");
            return Some(if first.eq_ignore_ascii_case(short) || short.is_empty() {
                inner.to_string()
            } else {
                format!("{short} {inner}")
            });
        }
    }
    None
}

fn vendor_short(v: u32) -> &'static str {
    match v {
        0x1002 | 0x1022 => "AMD",
        0x8086 => "Intel",
        0x10de => "NVIDIA",
        _ => "GPU",
    }
}

/// Formato "4.0 ×16" da velocità ("16.0 GT/s PCIe") e larghezza ("16").
fn pcie_text(speed: &str, width: &str) -> Option<String> {
    let gts: f64 = speed.split_whitespace().next()?.parse().ok()?;
    let gen_ = match gts as u32 {
        2 => "1.0",
        5 => "2.0",
        8 => "3.0",
        16 => "4.0",
        32 => "5.0",
        64 => "6.0",
        _ => return None,
    };
    let w: u32 = width.trim().parse().ok()?;
    Some(format!("{gen_} ×{w}"))
}

/// Riga con '*' (attuale) e massimo da pp_dpm_sclk/mclk: ("0: 500Mhz", "1: 1800Mhz *").
pub fn parse_dpm(text: &str) -> (Option<u64>, Option<u64>) {
    let mut cur = None;
    let mut max = None;
    for l in text.lines() {
        let Some((_, rest)) = l.split_once(':') else {
            continue;
        };
        let num: String = rest
            .trim()
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        let Ok(mhz) = num.parse::<u64>() else {
            continue;
        };
        max = Some(max.map_or(mhz, |m: u64| m.max(mhz)));
        if l.trim_end().ends_with('*') {
            cur = Some(mhz);
        }
    }
    (cur, max)
}

fn read_hwmon(dev: &Path, g: &mut Gpu) {
    let Some(hw) = fs::read_dir(dev.join("hwmon")).ok().and_then(|rd| {
        let mut v: Vec<PathBuf> = rd
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("hwmon"))
            .map(|e| e.path())
            .collect();
        v.sort();
        v.into_iter().next()
    }) else {
        return;
    };
    for n in 1..=8 {
        let Some(mdeg) = rd_u64(&hw.join(format!("temp{n}_input"))) else {
            continue;
        };
        let label = read_text(&hw.join(format!("temp{n}_label")))
            .map(|l| l.trim().to_string())
            .unwrap_or_else(|| format!("temp{n}"));
        g.temps.push((label, mdeg as f64 / 1000.0));
    }
    g.temp = ["junction", "edge"]
        .iter()
        .find_map(|k| g.temps.iter().find(|(l, _)| l == k).map(|(_, t)| *t))
        .or_else(|| g.temps.first().map(|(_, t)| *t));
    let uw = |n: &str| rd_u64(&hw.join(n)).map(|v| v as f64 / 1e6);
    g.power_w = uw("power1_average").or_else(|| uw("power1_input"));
    g.power_cap_w = uw("power1_cap");
    g.fan_rpm = rd_u64(&hw.join("fan1_input"));
    g.fan_pct = rd_u64(&hw.join("pwm1")).map(|p| p as f64 / 255.0 * 100.0);
}

/// Dati statici (nome, ids, driver): letti una volta sola.
fn read_static(roots: &Roots, card: &str, cache_pci: &mut Option<String>) -> Option<Gpu> {
    let cdir = roots.sys.join("class/drm").join(card);
    let dev = cdir.join("device");
    let vendor = rd_hex(&dev.join("vendor"))?;
    let device = rd_hex(&dev.join("device")).unwrap_or(0);
    let uevent = read_text(&dev.join("uevent")).unwrap_or_default();
    let pdev = uevent_value(&uevent, "PCI_SLOT_NAME")
        .or_else(|| {
            fs::read_link(&dev)
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        })
        .unwrap_or_else(|| card.to_string());
    let driver = uevent_value(&uevent, "DRIVER")
        .or_else(|| {
            fs::read_link(dev.join("driver"))
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        })
        .unwrap_or_default();
    let product = read_text(&dev.join("product_name"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let name = product
        .or_else(|| {
            let ids = cache_pci.get_or_insert_with(|| {
                let usr = roots.etc.parent().unwrap_or(Path::new("/"));
                ["usr/share/hwdata/pci.ids", "usr/share/misc/pci.ids"]
                    .iter()
                    .find_map(|p| read_text(&usr.join(p)))
                    .unwrap_or_default()
            });
            pci_name(ids, vendor, device)
        })
        .unwrap_or_else(|| format!("{} GPU [{vendor:04x}:{device:04x}]", vendor_short(vendor)));
    Some(Gpu {
        card: card.to_string(),
        pdev,
        vendor,
        device,
        driver,
        name,
        ..Gpu::default()
    })
}

/// Letture dinamiche del dispositivo, solo se attivo.
fn read_dynamic(roots: &Roots, g: &mut Gpu) {
    let cdir = roots.sys.join("class/drm").join(&g.card);
    let dev = cdir.join("device");
    match g.driver.as_str() {
        "amdgpu" => {
            g.usage = rd_u64(&dev.join("gpu_busy_percent")).map(|v| v as f64);
            g.vram_used = rd_u64(&dev.join("mem_info_vram_used"));
            g.vram_total = rd_u64(&dev.join("mem_info_vram_total"));
            g.gtt_used = rd_u64(&dev.join("mem_info_gtt_used"));
            g.gtt_total = rd_u64(&dev.join("mem_info_gtt_total"));
            if let Some(t) = read_text(&dev.join("pp_dpm_sclk")) {
                let (cur, max) = parse_dpm(&t);
                g.clock_mhz = cur;
                g.clock_max_mhz = max;
            }
            if let Some(t) = read_text(&dev.join("pp_dpm_mclk")) {
                g.mem_clock_mhz = parse_dpm(&t).0;
            }
            // Integrata se la VRAM dedicata è piccola (UMA).
            g.integrated = g.vram_total.is_some_and(|v| v <= 1 << 30);
            read_hwmon(&dev, g);
        }
        "i915" => {
            g.integrated = igpu_pdev(&g.pdev);
            g.clock_mhz = rd_u64(&cdir.join("gt_act_freq_mhz"))
                .filter(|v| *v > 0)
                .or_else(|| rd_u64(&cdir.join("gt_cur_freq_mhz")));
            g.clock_max_mhz = rd_u64(&cdir.join("gt_max_freq_mhz"));
            read_hwmon(&dev, g);
        }
        "xe" => {
            g.integrated = igpu_pdev(&g.pdev);
            // Si leggono solo tile0/gt0 (le GPU con più tile/gt non sono ancora distinte).
            let f = dev.join("tile0/gt0/freq0");
            g.clock_mhz = rd_u64(&f.join("act_freq"))
                .filter(|v| *v > 0)
                .or_else(|| rd_u64(&f.join("cur_freq")));
            g.clock_max_mhz = rd_u64(&f.join("max_freq"));
            read_hwmon(&dev, g);
        }
        _ => {}
    }
    if !g.integrated || g.driver == "amdgpu" {
        let s = read_text(&dev.join("current_link_speed"));
        let w = read_text(&dev.join("current_link_width"));
        if let (Some(s), Some(w)) = (s, w) {
            g.pcie = pcie_text(&s, &w);
        }
        let s = read_text(&dev.join("max_link_speed"));
        let w = read_text(&dev.join("max_link_width"));
        if let (Some(s), Some(w)) = (s, w) {
            g.pcie_max = pcie_text(&s, &w);
        }
    }
}

/// Intel su bus PCI 0 (0000:00:xx.x) = integrata; qualunque altro bus = discreta (Arc).
fn igpu_pdev(pdev: &str) -> bool {
    pdev.starts_with("0000:00:")
}

fn is_suspended(dev: &Path) -> bool {
    read_text(&dev.join("power/runtime_status")).is_some_and(|s| s.trim() == "suspended")
        || read_text(&dev.join("power_state")).is_some_and(|s| s.trim() == "D3cold")
}

// ---------------------------------------------------------------- collettore

/// Stato tra un giro e l'altro: campioni precedenti dei client, cache dei fd DRM.
#[derive(Default)]
pub struct GpuCollector {
    prev: HashMap<(String, u64), ClientRaw>,
    fd_cache: HashMap<u32, FdEntry>,
    statics: HashMap<String, Gpu>,
    /// AMD: la scheda è integrata? (noto dopo la prima lettura; ignota → trattata da discreta)
    amd_igpu: HashMap<String, bool>,
    round: u64,
}

/// Processo noto, per la scansione dei fd.
#[derive(Debug, Clone, Copy)]
pub struct PidInfo {
    pub pid: u32,
    pub start_ticks: u64,
    /// Thread del kernel: non hanno fd DRM, si saltano.
    pub kernel: bool,
}

/// Fd DRM di un processo, valido finché non cambia l'avvio.
struct FdEntry {
    start: u64,
    fds: Vec<String>,
    /// /proc/pid/fd non leggibile: non si riprova finché il processo è lo stesso.
    denied: bool,
}

/// Fd DRM (/dev/dri/*) di un processo e se la cartella non era leggibile (permesso negato).
fn scan_drm_fds(proc_root: &Path, pid: u32) -> (Vec<String>, bool) {
    let rd = match fs::read_dir(proc_root.join(pid.to_string()).join("fd")) {
        Ok(rd) => rd,
        Err(e) => return (Vec::new(), e.kind() == std::io::ErrorKind::PermissionDenied),
    };
    let mut v: Vec<String> = rd
        .flatten()
        .filter(|e| {
            fs::read_link(e.path())
                .ok()
                .is_some_and(|t| t.to_string_lossy().starts_with("/dev/dri/"))
        })
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    (v, false)
}

struct ClientUse {
    pid: u32,
    pdev: String,
    /// (nome motore, %) di questo giro.
    engines: Vec<(String, f64)>,
    /// Tipo di lavoro dai contatori cumulati.
    kind: WorkKind,
    mem_vram: u64,
    mem_system: u64,
}

fn work_kind(c: &ClientRaw) -> WorkKind {
    let mut gfx = false;
    let mut comp = false;
    let mut vid = false;
    let mut copy = false;
    for (n, ctr) in &c.engines {
        if counter_total(*ctr) == 0 {
            continue;
        }
        match engine_kind(n) {
            EngineKind::Gfx => gfx = true,
            EngineKind::Compute => comp = true,
            EngineKind::Copy => copy = true,
            EngineKind::Enc | EngineKind::Dec | EngineKind::Video => vid = true,
        }
    }
    if gfx {
        WorkKind::Graphics
    } else if comp {
        WorkKind::Compute
    } else if vid {
        WorkKind::Video
    } else if copy {
        WorkKind::Copy
    } else {
        WorkKind::None
    }
}

impl GpuCollector {
    /// Aggiorna `info`. `pids` sono i processi noti (già letti da /proc); `dt` in secondi.
    pub fn refresh(&mut self, info: &mut GpuInfo, roots: &Roots, pids: &[PidInfo], dt: f64) {
        self.round += 1;
        let mut devices = self.discover(roots);

        // 1. client DRM, deduplicati per (pdev, client-id) con il pid più basso.
        let mut sorted: Vec<PidInfo> = pids.iter().copied().filter(|p| !p.kernel).collect();
        sorted.sort_unstable_by_key(|p| p.pid);
        let mut clients: Vec<(u32, ClientRaw)> = Vec::new();
        let mut seen: HashMap<(String, u64), ()> = HashMap::new();
        let mut live: HashMap<u32, ()> = HashMap::new();
        for pi in &sorted {
            let pid = pi.pid;
            live.insert(pid, ());
            // Pid nuovi (o riusati) subito; con fd DRM ogni 5 giri; senza ogni 20; negati mai.
            let rescan = match self.fd_cache.get(&pid) {
                None => true,
                Some(e) if e.start != pi.start_ticks => true,
                Some(e) if e.denied => false,
                Some(e) => {
                    let every = if e.fds.is_empty() { 20 } else { 5 };
                    (self.round + u64::from(pid)).is_multiple_of(every)
                }
            };
            if rescan {
                let (fds, denied) = scan_drm_fds(&roots.proc, pid);
                self.fd_cache.insert(
                    pid,
                    FdEntry {
                        start: pi.start_ticks,
                        fds,
                        denied,
                    },
                );
            }
            let Some(entry) = self.fd_cache.get(&pid) else {
                continue;
            };
            for fd in &entry.fds {
                let path = roots.proc.join(pid.to_string()).join("fdinfo").join(fd);
                let Some(c) = read_text(&path).and_then(|t| parse_fdinfo(&t)) else {
                    continue;
                };
                let key = (c.pdev.clone(), c.client_id);
                if seen.insert(key, ()).is_none() {
                    clients.push((pid, c));
                }
            }
        }
        self.fd_cache.retain(|pid, _| live.contains_key(pid));

        // 2. uso dei client rispetto al campione precedente.
        let mut uses: Vec<ClientUse> = Vec::new();
        for (pid, c) in &clients {
            let prev = self.prev.get(&(c.pdev.clone(), c.client_id));
            let engines: Vec<(String, f64)> = c
                .engines
                .iter()
                .map(|(n, now)| {
                    let pct = match prev.and_then(|p| p.engines.get(n)) {
                        Some(before) => {
                            engine_pct(*before, *now, dt, c.capacity.get(n).copied().unwrap_or(1))
                        }
                        None => 0.0,
                    };
                    (n.clone(), pct)
                })
                .collect();
            uses.push(ClientUse {
                pid: *pid,
                pdev: c.pdev.clone(),
                engines,
                kind: work_kind(c),
                mem_vram: c.vram,
                mem_system: c.system,
            });
        }
        self.prev = clients
            .into_iter()
            .map(|(_, c)| ((c.pdev.clone(), c.client_id), c))
            .collect();

        // 3. per processo: GPU% = motore più occupato (come per il dispositivo), non la somma.
        let mut procs: HashMap<(u32, String), (GpuProc, HashMap<String, f64>)> = HashMap::new();
        for u in &uses {
            let (p, per_engine) = procs.entry((u.pid, u.pdev.clone())).or_insert_with(|| {
                (
                    GpuProc {
                        pid: u.pid,
                        pdev: u.pdev.clone(),
                        ..GpuProc::default()
                    },
                    HashMap::new(),
                )
            });
            for (n, pct) in &u.engines {
                *per_engine.entry(n.clone()).or_insert(0.0) += pct;
            }
            if p.kind == WorkKind::None {
                p.kind = u.kind;
            }
            p.mem += if u.mem_vram > 0 {
                u.mem_vram
            } else {
                u.mem_system
            };
        }
        let mut procs: Vec<GpuProc> = procs
            .into_values()
            .map(|(mut p, engines)| {
                for (n, pct) in &engines {
                    match engine_kind(n) {
                        EngineKind::Gfx | EngineKind::Compute | EngineKind::Copy => {
                            p.gpu_pct = p.gpu_pct.max(*pct)
                        }
                        EngineKind::Enc => p.enc_pct += pct,
                        EngineKind::Dec | EngineKind::Video => p.dec_pct += pct,
                    }
                }
                p.gpu_pct = p.gpu_pct.min(100.0);
                p.enc_pct = p.enc_pct.min(100.0);
                p.dec_pct = p.dec_pct.min(100.0);
                p
            })
            .collect();
        procs.sort_by(|a, b| a.pid.cmp(&b.pid).then(a.pdev.cmp(&b.pdev)));

        // 4. per dispositivo
        for g in &mut devices {
            let mut per_engine: HashMap<String, f64> = HashMap::new();
            let mut sys_mem = 0u64;
            let mut any = false;
            for u in uses.iter().filter(|u| u.pdev == g.pdev) {
                any = true;
                sys_mem += u.mem_system;
                for (n, pct) in &u.engines {
                    *per_engine.entry(n.clone()).or_insert(0.0) += pct;
                }
            }
            let mut engines: Vec<(String, f64)> = per_engine
                .into_iter()
                .map(|(n, v)| (n, v.min(100.0)))
                .collect();
            engines.sort_by(|a, b| a.0.cmp(&b.0));
            let sum_kind = |f: fn(EngineKind) -> bool| {
                let v: Vec<f64> = engines
                    .iter()
                    .filter(|(n, _)| f(engine_kind(n)))
                    .map(|(_, p)| *p)
                    .collect();
                (!v.is_empty()).then(|| v.iter().sum::<f64>().min(100.0))
            };
            g.enc = sum_kind(|k| k == EngineKind::Enc);
            g.dec = sum_kind(|k| k == EngineKind::Dec);
            g.video = sum_kind(|k| k == EngineKind::Video);
            if g.state != GpuState::Unsupported {
                if g.usage.is_none() && any && g.state != GpuState::Suspended {
                    g.usage = engines.iter().map(|(_, p)| *p).reduce(f64::max);
                }
                if matches!(g.driver.as_str(), "i915" | "xe") {
                    // Per Intel l'uso è il massimo dei motori (anche se sospesa: dai soli fdinfo).
                    g.usage = engines
                        .iter()
                        .map(|(_, p)| *p)
                        .reduce(f64::max)
                        .or(Some(0.0));
                    if sys_mem > 0 {
                        g.sys_mem = Some(sys_mem);
                    }
                }
            }
            g.engines = engines;
        }
        info.devices = devices;
        info.procs = procs;
    }

    fn discover(&mut self, roots: &Roots) -> Vec<Gpu> {
        let dir = roots.sys.join("class/drm");
        let mut cards: Vec<(u32, String)> = fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let n: u32 = name.strip_prefix("card")?.parse().ok()?;
                Some((n, name))
            })
            .collect();
        cards.sort();
        // pci.ids si legge solo se compare una scheda nuova e si libera subito dopo.
        let mut pci_ids: Option<String> = None;
        let mut out = Vec::new();
        for (_, card) in cards {
            if !self.statics.contains_key(&card)
                && let Some(g) = read_static(roots, &card, &mut pci_ids)
            {
                self.statics.insert(card.clone(), g);
            }
            let Some(mut g) = self.statics.get(&card).cloned() else {
                continue;
            };
            let dev = dir.join(&card).join("device");
            let suspended = is_suspended(&dev);
            // La regola "non svegliare" vale solo per le discrete: le integrate si leggono sempre
            // (AMD ignota → trattata da discreta finché non si è letta almeno una volta).
            let igpu = match g.driver.as_str() {
                "i915" | "xe" => igpu_pdev(&g.pdev),
                "amdgpu" => self.amd_igpu.get(&card).copied().unwrap_or(false),
                _ => false,
            };
            if !matches!(g.driver.as_str(), "amdgpu" | "i915" | "xe") {
                g.state = GpuState::Unsupported;
            } else if suspended && !igpu {
                g.state = GpuState::Suspended;
                g.d3cold =
                    read_text(&dev.join("power_state")).is_some_and(|s| s.trim() == "D3cold");
                g.integrated = false;
            } else {
                read_dynamic(roots, &mut g);
                if g.driver == "amdgpu" {
                    self.amd_igpu.insert(card.clone(), g.integrated);
                }
                if suspended {
                    g.state = GpuState::Idle;
                    if g.clock_mhz == Some(0) {
                        g.clock_mhz = None;
                    }
                }
            }
            out.push(g);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PCI: &str = "# comment\n8086  Intel Corporation\n\t46d1  AlderLake-N [UHD Graphics]\n\t\t1234 5678  sub\n1002  Advanced Micro Devices, Inc. [AMD/ATI]\n\t73bf  Navi 21 [Radeon RX 6800/6800 XT / 6900 XT]\n10de  NVIDIA Corporation\n\t2204  GA102 [GeForce RTX 3090]\n";

    #[test]
    fn nomi_da_pci_ids() {
        assert_eq!(
            pci_name(PCI, 0x8086, 0x46d1).as_deref(),
            Some("Intel UHD Graphics")
        );
        assert_eq!(
            pci_name(PCI, 0x1002, 0x73bf).as_deref(),
            Some("AMD Radeon RX 6800/6800 XT / 6900 XT")
        );
        assert_eq!(
            pci_name(PCI, 0x10de, 0x2204).as_deref(),
            Some("NVIDIA GeForce RTX 3090")
        );
        assert_eq!(pci_name(PCI, 0x8086, 0x9999), None);
        assert_eq!(pci_name("", 0x8086, 0x46d1), None);
    }

    #[test]
    fn fdinfo_amdgpu_i915_xe() {
        let amd = "pos:\t0\ndrm-driver:\tamdgpu\ndrm-client-id:\t7\ndrm-pdev:\t0000:03:00.0\ndrm-memory-vram:\t2048 KiB\ndrm-memory-gtt:\t10 KiB\ndrm-engine-gfx:\t5000 ns\ndrm-engine-compute:\t0 ns\ndrm-engine-enc:\t100 ns\n";
        let c = parse_fdinfo(amd).unwrap();
        assert_eq!((c.client_id, c.vram, c.gtt), (7, 2048 * 1024, 10 * 1024));
        assert_eq!(c.engines["gfx"], Counter::Ns(5000));
        assert_eq!(c.pdev, "0000:03:00.0");
        let i = "drm-driver:\ti915\ndrm-client-id:\t19\ndrm-pdev:\t0000:00:02.0\ndrm-resident-system0:\t212860 KiB\ndrm-resident-stolen-system0:\t5 KiB\ndrm-engine-render:\t99 ns\ndrm-engine-video:\t10 ns\ndrm-engine-capacity-video:\t2\n";
        let c = parse_fdinfo(i).unwrap();
        // stolen-system0 non è "system": resta fuori
        assert_eq!(c.system, 212860 * 1024);
        assert_eq!(c.capacity["video"], 2);
        assert!(!c.engines.contains_key("capacity-video"));
        let x = "drm-driver:\txe\ndrm-client-id:\t3\ndrm-pdev:\t0000:00:02.0\ndrm-cycles-rcs:\t50\ndrm-total-cycles-rcs:\t100\ndrm-resident-vram0:\t4 KiB\n";
        let c = parse_fdinfo(x).unwrap();
        assert_eq!(
            c.engines["rcs"],
            Counter::Cycles {
                used: 50,
                total: 100
            }
        );
        assert_eq!(c.vram, 4096);
        assert!(parse_fdinfo("drm-driver: x\n").is_none());
        assert!(parse_fdinfo("").is_none());
        assert!(parse_fdinfo("drm-client-id: zz\ndrm-pdev: a\n").is_none());
    }

    #[test]
    fn vram_resident_e_memory_non_si_sommano() {
        let t = "drm-driver: amdgpu\ndrm-client-id: 1\ndrm-pdev: 0000:03:00.0\ndrm-memory-vram: 1000 KiB\ndrm-resident-vram: 800 KiB\n";
        assert_eq!(parse_fdinfo(t).unwrap().vram, 800 * 1024);
        // solo memory: si usa quella
        let t = "drm-client-id: 1\ndrm-pdev: a\ndrm-memory-vram: 1000 KiB\n";
        assert_eq!(parse_fdinfo(t).unwrap().vram, 1000 * 1024);
        // più regioni resident (vram0 + local0) si sommano tra loro, non con memory
        let t = "drm-client-id: 1\ndrm-pdev: a\ndrm-resident-vram0: 10 KiB\ndrm-resident-local1: 5 KiB\ndrm-memory-vram: 999 KiB\n";
        assert_eq!(parse_fdinfo(t).unwrap().vram, 15 * 1024);
    }

    #[test]
    fn calcolo_percentuali() {
        // 0.5 s di lavoro in 1 s → 50 %
        assert!((engine_pct(Counter::Ns(0), Counter::Ns(500_000_000), 1.0, 1) - 50.0).abs() < 1e-9);
        // due istanze del motore: 50 % del totale
        assert!(
            (engine_pct(Counter::Ns(0), Counter::Ns(1_000_000_000), 1.0, 2) - 50.0).abs() < 1e-9
        );
        assert_eq!(engine_pct(Counter::Ns(5), Counter::Ns(1), 1.0, 1), 0.0);
        assert_eq!(engine_pct(Counter::Ns(0), Counter::Ns(9), 0.0, 1), 0.0);
        let a = Counter::Cycles {
            used: 10,
            total: 100,
        };
        let b = Counter::Cycles {
            used: 40,
            total: 200,
        };
        assert!((engine_pct(a, b, 1.0, 1) - 30.0).abs() < 1e-9);
        assert_eq!(engine_pct(a, a, 1.0, 1), 0.0);
        assert_eq!(
            engine_pct(Counter::Ns(0), Counter::Ns(u64::MAX), 1.0, 1),
            100.0
        );
    }

    #[test]
    fn motori_dpm_pcie() {
        assert_eq!(engine_kind("render"), EngineKind::Gfx);
        assert_eq!(engine_kind("gfx"), EngineKind::Gfx);
        assert_eq!(engine_kind("compute"), EngineKind::Compute);
        assert_eq!(engine_kind("dma"), EngineKind::Copy);
        assert_eq!(engine_kind("copy"), EngineKind::Copy);
        assert_eq!(engine_kind("enc"), EngineKind::Enc);
        assert_eq!(engine_kind("dec"), EngineKind::Dec);
        assert_eq!(engine_kind("video-enhance"), EngineKind::Video);
        let (c, m) = parse_dpm("0: 500Mhz\n1: 1800Mhz *\n2: 2400Mhz\n");
        assert_eq!((c, m), (Some(1800), Some(2400)));
        assert_eq!(parse_dpm("niente"), (None, None));
        assert_eq!(
            pcie_text("16.0 GT/s PCIe", "16").as_deref(),
            Some("4.0 ×16")
        );
        assert_eq!(pcie_text("8.0 GT/s PCIe", "4").as_deref(), Some("3.0 ×4"));
        assert_eq!(pcie_text("boh", "4"), None);
        assert_eq!(parse_size("12"), 12);
        assert_eq!(parse_size("1 MiB"), 1 << 20);
    }
}
