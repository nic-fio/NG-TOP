// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Dischi: I/O da /proc/diskstats e filesystem montati da /proc/self/mountinfo + statvfs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use super::{Roots, read_text};

/// Contatori grezzi di una riga di /proc/diskstats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiskStat {
    pub reads: u64,
    pub sectors_read: u64,
    pub read_ticks: u64,
    pub writes: u64,
    pub sectors_written: u64,
    pub write_ticks: u64,
    pub in_flight: u64,
    /// Millisecondi in cui il disco ha avuto I/O in corso.
    pub io_ticks: u64,
}

/// Valori derivati tra due istantanee di `DiskStat`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DiskRates {
    pub read_iops: f64,
    pub write_iops: f64,
    /// Frazione di tempo occupato, 0..1.
    pub util: f64,
    /// Latenza media per operazione in ms; `None` senza operazioni.
    pub latency_ms: Option<f64>,
}

impl DiskStat {
    pub fn rates_since(&self, prev: &DiskStat, dt: f64) -> DiskRates {
        if dt <= 0.0 {
            return DiskRates::default();
        }
        let d = |a: u64, b: u64| a.saturating_sub(b);
        let (dr, dw) = (d(self.reads, prev.reads), d(self.writes, prev.writes));
        let ios = dr + dw;
        DiskRates {
            read_iops: dr as f64 / dt,
            write_iops: dw as f64 / dt,
            util: (d(self.io_ticks, prev.io_ticks) as f64 / (dt * 1000.0)).clamp(0.0, 1.0),
            latency_ms: (ios > 0).then(|| {
                (d(self.read_ticks, prev.read_ticks) + d(self.write_ticks, prev.write_ticks)) as f64
                    / ios as f64
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DiskIo {
    pub name: String,
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub read_rate: f64,
    pub write_rate: f64,
    pub raw: DiskStat,
    pub read_iops: f64,
    pub write_iops: f64,
    /// Frazione di tempo occupato (delta io_ticks / dt), 0..1.
    pub util: f64,
    pub latency_ms: Option<f64>,
    /// Richieste in corso (coda).
    pub in_flight: u64,
    pub model: String,
    /// "HDD", "SSD", "NVMe" oppure vuoto se ignoto.
    pub kind: &'static str,
    /// Dimensione in byte (0 se ignota).
    pub size: u64,
    pub temp: Option<f64>,
    temp_src: Option<Option<PathBuf>>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Filesystem {
    pub mountpoint: String,
    pub source: String,
    pub fstype: String,
    pub total: u64,
    pub used: u64,
    pub avail: u64,
    /// Opzioni di mount (da mountinfo).
    pub options: String,
    /// Frazione di inode usati; `None` se il filesystem non li espone.
    pub inode_frac: Option<f64>,
    /// La risposta di statvfs manca da più del tempo massimo (es. NFS bloccato).
    pub unresponsive: bool,
}

impl Filesystem {
    /// Frazione usata come in `df` (usato / (usato + disponibile)).
    pub fn used_frac(&self) -> f64 {
        let d = self.used + self.avail;
        if d == 0 {
            0.0
        } else {
            self.used as f64 / d as f64
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DiskInfo {
    pub io: Vec<DiskIo>,
    /// Totali dei soli dispositivi fisici (byte/s).
    pub read_rate: f64,
    pub write_rate: f64,
    pub filesystems: Vec<Filesystem>,
}

/// (nome, settori letti, settori scritti) da /proc/diskstats.
pub fn parse_diskstats(text: &str) -> Vec<(String, u64, u64)> {
    text.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            Some((
                f.get(2)?.to_string(),
                f.get(5)?.parse().ok()?,
                f.get(9)?.parse().ok()?,
            ))
        })
        .collect()
}

/// Righe complete di /proc/diskstats con i contatori usati per IOPS, uso e latenza.
pub fn parse_diskstats_full(text: &str) -> Vec<(String, DiskStat)> {
    text.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            let n = |i: usize| f.get(i).and_then(|x| x.parse::<u64>().ok());
            Some((
                f.get(2)?.to_string(),
                DiskStat {
                    reads: n(3)?,
                    sectors_read: n(5)?,
                    read_ticks: n(6).unwrap_or(0),
                    writes: n(7)?,
                    sectors_written: n(9)?,
                    write_ticks: n(10).unwrap_or(0),
                    in_flight: n(11).unwrap_or(0),
                    io_ticks: n(12).unwrap_or(0),
                },
            ))
        })
        .collect()
}

/// Dispositivo fisico intero (non partizione, loop, ram, device-mapper…).
pub fn is_physical(name: &str, sys: &Path) -> bool {
    const SKIP: [&str; 7] = ["loop", "ram", "zram", "dm-", "md", "sr", "fd"];
    if SKIP.iter().any(|p| name.starts_with(p)) {
        return false;
    }
    let block = sys.join("block");
    if block.is_dir() {
        // Le partizioni compaiono solo dentro la cartella del disco, non in /sys/block.
        block.join(name).exists()
    } else {
        // Ripiego sul nome: sdXN, nvme0n1pN, mmcblk0pN sono partizioni.
        !is_partition_name(name)
    }
}

fn is_partition_name(name: &str) -> bool {
    let digits = name.trim_end_matches(|c: char| c.is_ascii_digit());
    if digits.len() == name.len() {
        return false;
    }
    if name.starts_with("nvme") || name.starts_with("mmcblk") {
        digits.ends_with('p')
    } else {
        true
    }
}

/// Disco intero a cui appartiene la sorgente di un mount ("/dev/nvme0n1p2" → "nvme0n1").
pub fn disk_of_source(source: &str) -> Option<String> {
    let name = source.strip_prefix("/dev/")?;
    if name.starts_with("mapper/") || name.starts_with("dm-") || name.contains('/') {
        return None;
    }
    if !is_partition_name(name) {
        return Some(name.to_string());
    }
    let digits = name.trim_end_matches(|c: char| c.is_ascii_digit());
    let base = if name.starts_with("nvme") || name.starts_with("mmcblk") {
        digits.trim_end_matches('p')
    } else {
        digits
    };
    Some(base.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    pub devno: String,
    pub root: String,
    pub mountpoint: String,
    pub fstype: String,
    pub source: String,
    pub options: String,
}

const REAL_FS: [&str; 20] = [
    "ext2", "ext3", "ext4", "btrfs", "xfs", "f2fs", "vfat", "exfat", "zfs", "nfs", "nfs4", "cifs",
    "smb3", "ntfs", "ntfs3", "fuseblk", "jfs", "reiserfs", "bcachefs", "hfsplus",
];

const SKIP_PREFIXES: [&str; 4] = [
    "/var/lib/docker",
    "/var/lib/containers",
    "/snap/",
    "/var/snap/",
];

fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\'
            && i + 3 < b.len()
            && s.is_char_boundary(i + 4)
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 4], 8)
        {
            out.push(v);
            i += 4;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Mount di filesystem reali, senza duplicati dello stesso (dispositivo, radice).
pub fn parse_mountinfo(text: &str) -> Vec<Mount> {
    let mut out: Vec<Mount> = Vec::new();
    for l in text.lines() {
        let f: Vec<&str> = l.split_whitespace().collect();
        let Some(dash) = f.iter().position(|x| *x == "-") else {
            continue;
        };
        let (Some(devno), Some(root), Some(mp), Some(fstype), Some(source)) = (
            f.get(2),
            f.get(3),
            f.get(4),
            f.get(dash + 1),
            f.get(dash + 2),
        ) else {
            continue;
        };
        if !REAL_FS.contains(fstype) {
            continue;
        }
        let mountpoint = unescape(mp);
        if SKIP_PREFIXES.iter().any(|p| mountpoint.starts_with(p)) {
            continue;
        }
        if out
            .iter()
            .any(|m| m.mountpoint == mountpoint || (m.devno == *devno && m.root == unescape(root)))
        {
            continue;
        }
        out.push(Mount {
            devno: devno.to_string(),
            root: unescape(root),
            mountpoint,
            fstype: fstype.to_string(),
            source: unescape(source),
            options: f.get(5).map(|o| o.to_string()).unwrap_or_default(),
        });
    }
    out
}

/// Mount dei filesystem reali da /proc/self/mountinfo.
pub fn read_mounts(roots: &Roots) -> Vec<Mount> {
    read_text(&roots.proc.join("self/mountinfo"))
        .map(|t| parse_mountinfo(&t))
        .unwrap_or_default()
}

/// Spazio e inode di un filesystem (risultato di statvfs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FsSpace {
    pub total: u64,
    pub used: u64,
    pub avail: u64,
    pub files: u64,
    pub ffree: u64,
}

/// Misura di un filesystem; astratta per poterla sostituire nei test.
pub trait FsProbe: Send + Sync + 'static {
    /// `None` se non misurabile (errore o dimensione zero).
    fn probe(&self, mountpoint: &str) -> Option<FsSpace>;
}

/// statvfs reale (può bloccarsi a lungo su NFS: va chiamata solo da un thread dedicato).
pub struct StatvfsProbe;

impl FsProbe for StatvfsProbe {
    fn probe(&self, mountpoint: &str) -> Option<FsSpace> {
        let st = rustix::fs::statvfs(mountpoint).ok()?;
        let fr = st.f_frsize;
        let total = st.f_blocks.saturating_mul(fr);
        if total == 0 {
            return None;
        }
        Some(FsSpace {
            total,
            used: st.f_blocks.saturating_sub(st.f_bfree).saturating_mul(fr),
            avail: st.f_bavail.saturating_mul(fr),
            files: st.f_files,
            ffree: st.f_ffree,
        })
    }
}

/// Misura da un file `statvfs.json` (usata con `--roots`: non si interroga la macchina vera).
/// Formato: `{"/": {"total": N, "free": N, "files": N, "ffree": N}, ...}`; mount assente → `None`.
pub struct FileProbe {
    pub path: PathBuf,
}

impl FsProbe for FileProbe {
    fn probe(&self, mountpoint: &str) -> Option<FsSpace> {
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&self.path).ok()?).ok()?;
        let e = v.get(mountpoint)?;
        let n = |k: &str| e.get(k).and_then(serde_json::Value::as_u64);
        let (total, free) = (n("total")?, n("free")?);
        if total == 0 {
            return None;
        }
        Some(FsSpace {
            total,
            used: total.saturating_sub(free),
            avail: free,
            files: n("files").unwrap_or(0),
            ffree: n("ffree").unwrap_or(0),
        })
    }
}

/// Elenco dei filesystem con lo spazio, in modo sincrono; quelli non misurabili vengono scartati.
pub fn read_filesystems(roots: &Roots) -> Vec<Filesystem> {
    read_mounts(roots)
        .into_iter()
        .filter_map(|m| {
            let sp = StatvfsProbe.probe(&m.mountpoint)?;
            Some(make_fs(&m, Some(sp), false))
        })
        .collect()
}

fn make_fs(m: &Mount, sp: Option<FsSpace>, unresponsive: bool) -> Filesystem {
    let sp = sp.unwrap_or(FsSpace {
        total: 0,
        used: 0,
        avail: 0,
        files: 0,
        ffree: 0,
    });
    Filesystem {
        mountpoint: m.mountpoint.clone(),
        source: m.source.clone(),
        fstype: m.fstype.clone(),
        total: sp.total,
        used: sp.used,
        avail: sp.avail,
        options: m.options.clone(),
        inode_frac: (sp.files > 0)
            .then(|| sp.files.saturating_sub(sp.ffree) as f64 / sp.files as f64),
        unresponsive,
    }
}

/// Tempo oltre il quale un filesystem senza risposta è dichiarato "non risponde".
pub const FS_TIMEOUT: Duration = Duration::from_secs(5);

struct FsAnswer {
    mountpoint: String,
    space: Option<FsSpace>,
    at: Instant,
}

struct FsWorker {
    req: Sender<()>,
    /// Quando è partita la richiesta ancora senza risposta.
    pending_since: Option<Instant>,
    /// Ultima risposta (istante, spazio).
    last: Option<(Instant, Option<FsSpace>)>,
}

/// Misura i filesystem in thread separati (uno per mount): un mount bloccato non ferma
/// l'interfaccia né gli altri mount, e non accumula richieste.
pub struct FsMonitor {
    probe: Arc<dyn FsProbe>,
    timeout: Duration,
    tx: Sender<FsAnswer>,
    rx: Receiver<FsAnswer>,
    workers: HashMap<String, FsWorker>,
    mounts: Vec<Mount>,
}

impl FsMonitor {
    pub fn new(probe: Arc<dyn FsProbe>, timeout: Duration) -> Self {
        let (tx, rx) = mpsc::channel();
        FsMonitor {
            probe,
            timeout,
            tx,
            rx,
            workers: HashMap::new(),
            mounts: Vec::new(),
        }
    }

    /// Monitor che legge `statvfs.json` (vedi `FileProbe`) invece del sistema vero.
    pub fn from_file(path: PathBuf) -> Self {
        Self::new(Arc::new(FileProbe { path }), FS_TIMEOUT)
    }

    /// Monitor con statvfs reale e il tempo massimo standard.
    pub fn real() -> Self {
        Self::new(Arc::new(StatvfsProbe), FS_TIMEOUT)
    }

    /// Imposta l'elenco dei mount: avvia i thread dei nuovi e chiude quelli spariti.
    pub fn set_mounts(&mut self, mounts: Vec<Mount>) {
        self.workers
            .retain(|k, _| mounts.iter().any(|m| &m.mountpoint == k));
        for m in &mounts {
            if self.workers.contains_key(&m.mountpoint) {
                continue;
            }
            let (req, req_rx) = mpsc::channel::<()>();
            let (tx, probe, mp) = (
                self.tx.clone(),
                Arc::clone(&self.probe),
                m.mountpoint.clone(),
            );
            let spawned = std::thread::Builder::new()
                .name("ngtop-statvfs".into())
                .spawn(move || {
                    while req_rx.recv().is_ok() {
                        let space = probe.probe(&mp);
                        let at = Instant::now();
                        let answer = FsAnswer {
                            mountpoint: mp.clone(),
                            space,
                            at,
                        };
                        if tx.send(answer).is_err() {
                            break;
                        }
                    }
                });
            if spawned.is_ok() {
                self.workers.insert(
                    m.mountpoint.clone(),
                    FsWorker {
                        req,
                        pending_since: None,
                        last: None,
                    },
                );
            }
        }
        self.mounts = mounts;
    }

    /// Chiede una nuova misura a ogni mount che non ne ha già una in corso.
    pub fn request(&mut self) {
        let now = Instant::now();
        for w in self.workers.values_mut() {
            if w.pending_since.is_none() && w.req.send(()).is_ok() {
                w.pending_since = Some(now);
            }
        }
    }

    fn take(&mut self, a: FsAnswer) {
        if let Some(w) = self.workers.get_mut(&a.mountpoint) {
            w.pending_since = None;
            w.last = Some((a.at, a.space));
        }
    }

    /// Raccoglie le risposte arrivate; se `wait` > 0 aspetta (al massimo) quel tempo
    /// finché tutte le richieste in corso hanno risposto.
    pub fn poll(&mut self, wait: Duration) {
        while let Ok(a) = self.rx.try_recv() {
            self.take(a);
        }
        let deadline = Instant::now() + wait;
        while self.workers.values().any(|w| w.pending_since.is_some()) {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            match self.rx.recv_timeout(left) {
                Ok(a) => self.take(a),
                Err(_) => break,
            }
        }
    }

    /// Filesystem noti, nell'ordine di mountinfo. Quelli senza risposta da più di `timeout`
    /// compaiono con `unresponsive`; quelli non misurabili (errore) sono omessi.
    pub fn snapshot(&self, now: Instant) -> Vec<Filesystem> {
        self.mounts
            .iter()
            .filter_map(|m| {
                let w = self.workers.get(&m.mountpoint)?;
                let stuck = w
                    .pending_since
                    .is_some_and(|t| now.saturating_duration_since(t) > self.timeout);
                match w.last {
                    Some((_, Some(sp))) => Some(make_fs(m, Some(sp), stuck)),
                    Some((_, None)) if !stuck => None,
                    None if !stuck => None,
                    _ => Some(make_fs(m, None, true)),
                }
            })
            .collect()
    }
}

fn first_file(dir: &Path, prefix: &str, file: &str) -> Option<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(prefix))
        .map(|e| e.path().join(file))
        .filter(|p| p.exists())
        .collect();
    v.sort();
    v.into_iter().next()
}

/// File di temperatura hwmon del disco (nvme, drivetemp), se esiste.
fn find_disk_temp(sys: &Path, name: &str) -> Option<PathBuf> {
    let dev = sys.join("block").join(name).join("device");
    first_file(&dev.join("hwmon"), "hwmon", "temp1_input")
        .or_else(|| first_file(&dev, "hwmon", "temp1_input"))
}

/// Modello, tipo e dimensione di un disco da /sys/block.
fn read_disk_static(sys: &Path, name: &str) -> (String, &'static str, u64) {
    let base = sys.join("block").join(name);
    let model = read_text(&base.join("device/model"))
        .map(|m| m.split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_default();
    let rotational =
        read_text(&base.join("queue/rotational")).and_then(|t| t.trim().parse::<u8>().ok());
    let kind = if name.starts_with("nvme") {
        "NVMe"
    } else {
        match rotational {
            Some(1) => "HDD",
            Some(_) => "SSD",
            None => "",
        }
    };
    let size = read_text(&base.join("size"))
        .and_then(|t| t.trim().parse::<u64>().ok())
        .unwrap_or(0)
        .saturating_mul(512);
    (model, kind, size)
}

/// Aggiorna le velocità di I/O dei dischi fisici.
pub fn refresh_io(info: &mut DiskInfo, roots: &Roots, dt: f64) {
    let stats = read_text(&roots.proc.join("diskstats"))
        .map(|t| parse_diskstats_full(&t))
        .unwrap_or_default();
    let old = std::mem::take(&mut info.io);
    info.io = stats
        .into_iter()
        .filter(|(n, _)| is_physical(n, &roots.sys))
        .map(|(name, st)| {
            let (rb, wb) = (
                st.sectors_read.saturating_mul(512),
                st.sectors_written.saturating_mul(512),
            );
            let prev = old.iter().find(|o| o.name == name);
            let rate = |now: u64, before: Option<u64>| match before {
                Some(b) if dt > 0.0 => now.saturating_sub(b) as f64 / dt,
                _ => 0.0,
            };
            let rates = prev.map(|p| st.rates_since(&p.raw, dt)).unwrap_or_default();
            let (model, kind, size) = match prev {
                Some(p) => (p.model.clone(), p.kind, p.size),
                None => read_disk_static(&roots.sys, &name),
            };
            let temp_src = match prev.and_then(|p| p.temp_src.clone()) {
                Some(src) => Some(src),
                None => Some(find_disk_temp(&roots.sys, &name)),
            };
            let temp = temp_src
                .as_ref()
                .and_then(|p| p.as_deref())
                .and_then(super::sensors::read_temp);
            DiskIo {
                read_rate: rate(rb, prev.map(|p| p.read_bytes)),
                write_rate: rate(wb, prev.map(|p| p.write_bytes)),
                read_iops: rates.read_iops,
                write_iops: rates.write_iops,
                util: rates.util,
                latency_ms: rates.latency_ms,
                in_flight: st.in_flight,
                raw: st,
                model,
                kind,
                size,
                temp,
                temp_src,
                name,
                read_bytes: rb,
                write_bytes: wb,
            }
        })
        .collect();
    info.read_rate = info.io.iter().map(|d| d.read_rate).sum();
    info.write_rate = info.io.iter().map(|d| d.write_rate).sum();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diskstats_e_nomi() {
        let t = "   8       0 sda 100 0 2000 5 50 0 800 3 0 1 1\n   8       1 sda1 90 0 1800 5 40 0 700 3 0 1 1\n";
        let s = parse_diskstats(t);
        assert_eq!(s[0], ("sda".into(), 2000, 800));
        assert_eq!(s.len(), 2);
        let none = Path::new("/nonexistent-ngtop");
        assert!(is_physical("sda", none));
        assert!(!is_physical("sda1", none));
        assert!(is_physical("nvme0n1", none));
        assert!(!is_physical("nvme0n1p2", none));
        assert!(!is_physical("loop3", none));
        assert!(!is_physical("dm-0", none));
        assert!(is_physical("mmcblk0", none));
        assert!(!is_physical("mmcblk0p1", none));
        assert_eq!(disk_of_source("/dev/nvme0n1p2").as_deref(), Some("nvme0n1"));
        assert_eq!(disk_of_source("/dev/sda1").as_deref(), Some("sda"));
        assert_eq!(disk_of_source("/dev/sdb").as_deref(), Some("sdb"));
        assert_eq!(disk_of_source("/dev/mapper/x"), None);
        assert_eq!(disk_of_source("server:/export"), None);
    }

    #[test]
    fn mountinfo_filtra_e_deduplica() {
        let t = "\
22 1 8:2 / / rw,relatime shared:1 - ext4 /dev/sda2 rw\n\
23 22 0:5 / /proc rw - proc proc rw\n\
24 22 0:20 / /run rw - tmpfs tmpfs rw\n\
25 22 8:1 / /boot/efi rw - vfat /dev/sda1 rw\n\
26 22 8:2 /bind /mnt/bind rw - ext4 /dev/sda2 rw\n\
27 22 8:2 / /dup rw - ext4 /dev/sda2 rw\n\
28 22 0:30 /@home /home rw - btrfs /dev/sdb1 rw,subvol=/@home\n\
29 22 0:30 /@ /snapshots rw - btrfs /dev/sdb1 rw\n\
30 22 8:3 / /mnt/con\\040spazio rw - xfs /dev/sdc1 rw\n\
31 22 0:40 / /var/lib/docker/x rw - ext4 /dev/sdd rw\n\
garbage\n";
        let m = parse_mountinfo(t);
        let mps: Vec<&str> = m.iter().map(|x| x.mountpoint.as_str()).collect();
        assert_eq!(
            mps,
            [
                "/",
                "/boot/efi",
                "/mnt/bind",
                "/home",
                "/snapshots",
                "/mnt/con spazio"
            ]
        );
        assert_eq!(m[0].source, "/dev/sda2");
    }

    #[test]
    fn statvfs_reale_su_radice() {
        let roots = Roots {
            proc: std::env::temp_dir().join("ngtop-nessuna-proc"),
            ..Roots::default()
        };
        assert!(read_filesystems(&roots).is_empty());
        let fs = Filesystem {
            used: 25,
            avail: 75,
            total: 100,
            ..Filesystem::default()
        };
        assert!((fs.used_frac() - 0.25).abs() < 1e-9);
        assert_eq!(Filesystem::default().used_frac(), 0.0);
    }

    #[test]
    fn calcoli_disco_da_due_istantanee() {
        let a = parse_diskstats_full("   8       0 sda 100 0 2000 50 50 0 800 30 2 1000 1\n");
        let b = parse_diskstats_full("   8       0 sda 200 0 4000 150 100 0 1600 80 3 1500 1\n");
        assert_eq!(a[0].1.in_flight, 2);
        let r = b[0].1.rates_since(&a[0].1, 2.0);
        assert!((r.read_iops - 50.0).abs() < 1e-9);
        assert!((r.write_iops - 25.0).abs() < 1e-9);
        // 500 ms occupato su 2000 ms
        assert!((r.util - 0.25).abs() < 1e-9);
        // (100 + 50) ms su 150 operazioni
        assert!((r.latency_ms.unwrap() - 1.0).abs() < 1e-9);
        // nessuna operazione → nessuna latenza; dt nullo → tutto a zero
        let z = a[0].1.rates_since(&a[0].1, 1.0);
        assert_eq!(z.latency_ms, None);
        assert_eq!(b[0].1.rates_since(&a[0].1, 0.0), DiskRates::default());
        // contatore che torna indietro: saturato
        assert_eq!(a[0].1.rates_since(&b[0].1, 1.0).read_iops, 0.0);
        // uso oltre il 100% (clock irregolare) → limitato a 1
        assert_eq!(b[0].1.rates_since(&a[0].1, 0.1).util, 1.0);
        assert!(parse_diskstats_full("rotto\n 1 2").is_empty());
    }

    fn fake_sys(tag: &str) -> (Roots, PathBuf) {
        let d = std::env::temp_dir().join(format!("ngtop-disk-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let dev = d.join("sys/block/nvme0n1/device");
        std::fs::create_dir_all(dev.join("hwmon/hwmon3")).unwrap();
        std::fs::create_dir_all(d.join("sys/block/nvme0n1/queue")).unwrap();
        std::fs::create_dir_all(d.join("sys/block/sdb/device")).unwrap();
        std::fs::create_dir_all(d.join("sys/block/sdb/queue")).unwrap();
        std::fs::write(dev.join("model"), "Samsung SSD 990   PRO 2TB \n").unwrap();
        std::fs::write(dev.join("hwmon/hwmon3/temp1_input"), "41850\n").unwrap();
        std::fs::write(d.join("sys/block/nvme0n1/queue/rotational"), "0\n").unwrap();
        std::fs::write(d.join("sys/block/nvme0n1/size"), "1953525168\n").unwrap();
        std::fs::write(d.join("sys/block/sdb/device/model"), "WDC WD40EFRX\n").unwrap();
        std::fs::write(d.join("sys/block/sdb/queue/rotational"), "1\n").unwrap();
        std::fs::create_dir_all(d.join("proc")).unwrap();
        let roots = Roots {
            proc: d.join("proc"),
            sys: d.join("sys"),
            etc: d.join("etc"),
        };
        (roots, d)
    }

    #[test]
    fn refresh_io_modello_tipo_temperatura_e_iops() {
        let (roots, d) = fake_sys("io");
        let snap = |n: u64| {
            format!(
                "259 0 nvme0n1 {r} 0 {rs} {rt} {w} 0 {ws} {wt} 4 {it} 1\n  8 16 sdb 10 0 80 5 10 0 80 5 0 10 1\n 259 1 nvme0n1p1 1 0 1 0 1 0 1 0 0 0 0\n",
                r = 100 * n,
                rs = 2000 * n,
                rt = 50 * n,
                w = 50 * n,
                ws = 1000 * n,
                wt = 25 * n,
                it = 500 * n,
            )
        };
        let mut info = DiskInfo::default();
        std::fs::write(roots.proc.join("diskstats"), snap(1)).unwrap();
        refresh_io(&mut info, &roots, 0.0);
        assert_eq!(info.io.len(), 2);
        assert_eq!(info.io[0].read_iops, 0.0);
        std::fs::write(roots.proc.join("diskstats"), snap(2)).unwrap();
        refresh_io(&mut info, &roots, 1.0);
        let n = &info.io[0];
        assert_eq!(n.name, "nvme0n1");
        assert_eq!(n.model, "Samsung SSD 990 PRO 2TB");
        assert_eq!(n.kind, "NVMe");
        assert_eq!(n.size, 1953525168 * 512);
        assert_eq!(n.temp, Some(41.85));
        assert!((n.read_iops - 100.0).abs() < 1e-9);
        assert!((n.write_iops - 50.0).abs() < 1e-9);
        assert!((n.util - 0.5).abs() < 1e-9);
        assert!((n.latency_ms.unwrap() - 0.5).abs() < 1e-9);
        assert_eq!(n.in_flight, 4);
        assert!((n.read_rate - 2000.0 * 512.0).abs() < 1e-6);
        let s = &info.io[1];
        assert_eq!(
            (s.kind, s.model.as_str(), s.temp),
            ("HDD", "WDC WD40EFRX", None)
        );
        assert_eq!(s.latency_ms, None);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Sonda finta: "/lento" impiega molto, "/manca" fallisce, il resto risponde subito.
    struct FakeProbe;

    impl FsProbe for FakeProbe {
        fn probe(&self, mp: &str) -> Option<FsSpace> {
            match mp {
                "/lento" => {
                    std::thread::sleep(Duration::from_millis(700));
                    Some(sp(100))
                }
                "/manca" => None,
                _ => Some(sp(1000)),
            }
        }
    }

    fn sp(total: u64) -> FsSpace {
        FsSpace {
            total,
            used: total / 4,
            avail: total / 2,
            files: 100,
            ffree: 90,
        }
    }

    fn mount(mp: &str) -> Mount {
        Mount {
            devno: "0:1".into(),
            root: "/".into(),
            mountpoint: mp.into(),
            fstype: "nfs4".into(),
            source: "srv:/x".into(),
            options: "rw,vers=4.2".into(),
        }
    }

    #[test]
    fn statvfs_in_thread_non_blocca_e_segnala_non_risponde() {
        let mut m = FsMonitor::new(Arc::new(FakeProbe), Duration::from_millis(100));
        m.set_mounts(vec![mount("/ok"), mount("/lento"), mount("/manca")]);
        m.request();
        let t0 = Instant::now();
        m.poll(Duration::from_millis(60));
        // la richiesta lenta non ha bloccato più dell'attesa concessa
        assert!(t0.elapsed() < Duration::from_millis(400));
        let v = m.snapshot(Instant::now());
        // /ok presente; /lento ancora in attesa (non ancora "non risponde"); /manca omesso
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].mountpoint, "/ok");
        assert_eq!(v[0].total, 1000);
        assert_eq!(v[0].options, "rw,vers=4.2");
        assert!((v[0].inode_frac.unwrap() - 0.1).abs() < 1e-9);
        assert!(!v[0].unresponsive);
        // oltre il tempo massimo: /lento compare come non risponde, ordine di mountinfo
        std::thread::sleep(Duration::from_millis(150));
        m.poll(Duration::ZERO);
        let v = m.snapshot(Instant::now());
        let mps: Vec<&str> = v.iter().map(|f| f.mountpoint.as_str()).collect();
        assert_eq!(mps, ["/ok", "/lento"]);
        assert!(v[1].unresponsive && !v[0].unresponsive);
        // una nuova richiesta non si accumula finché la precedente è pendente
        m.request();
        // quando risponde, torna normale con i dati
        m.poll(Duration::from_millis(1500));
        let v = m.snapshot(Instant::now());
        assert_eq!(v[1].total, 100);
        assert!(!v[1].unresponsive);
        // un mount sparito dall'elenco viene dimenticato
        m.set_mounts(vec![mount("/ok")]);
        assert_eq!(m.snapshot(Instant::now()).len(), 1);
    }
}

#[cfg(test)]
mod file_probe_tests {
    use super::*;

    #[test]
    fn statvfs_da_file_e_mount_assente() {
        let dir = std::env::temp_dir().join(format!("ngtop-fp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("statvfs.json");
        std::fs::write(
            &f,
            r#"{"/": {"total": 1000, "free": 250, "files": 10, "ffree": 4}}"#,
        )
        .unwrap();
        let p = FileProbe { path: f };
        let sp = p.probe("/").unwrap();
        assert_eq!((sp.total, sp.used, sp.avail), (1000, 750, 250));
        assert_eq!((sp.files, sp.ffree), (10, 4));
        assert!(p.probe("/home").is_none());
        let missing = FileProbe {
            path: dir.join("nope.json"),
        };
        assert!(missing.probe("/").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
