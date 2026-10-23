// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Dettaglio di UN processo (quello selezionato): letto a ogni tick solo per quel pid.
//! Le schede pesanti (thread, file, rete, ambiente, limiti, cgroup) si leggono solo se attive.

use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::path::Path;

use super::Roots;
use super::process::parse_stat;
use crate::source::Source;

/// Massimo di righe dell'elenco dei file aperti e delle variabili d'ambiente.
pub const MAX_FILES: usize = 200;
pub const MAX_ENV: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailTab {
    Generale,
    Thread,
    File,
    Rete,
    Ambiente,
    Limiti,
    Cgroup,
}

impl DetailTab {
    pub const ALL: [DetailTab; 7] = [
        DetailTab::Generale,
        DetailTab::Thread,
        DetailTab::File,
        DetailTab::Rete,
        DetailTab::Ambiente,
        DetailTab::Limiti,
        DetailTab::Cgroup,
    ];

    pub fn title(self) -> &'static str {
        match self {
            DetailTab::Generale => "General",
            DetailTab::Thread => "Threads",
            DetailTab::File => "Files",
            DetailTab::Rete => "Net",
            DetailTab::Ambiente => "Env",
            DetailTab::Limiti => "Limits",
            DetailTab::Cgroup => "cgroup",
        }
    }

    fn index(self) -> usize {
        DetailTab::ALL.iter().position(|&t| t == self).unwrap_or(0)
    }

    pub fn next(self) -> DetailTab {
        DetailTab::ALL[(self.index() + 1) % DetailTab::ALL.len()]
    }

    pub fn prev(self) -> DetailTab {
        DetailTab::ALL[(self.index() + DetailTab::ALL.len() - 1) % DetailTab::ALL.len()]
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ThreadInfo {
    pub tid: u32,
    pub name: String,
    pub state: char,
    /// % di un core (delta tra due letture); 0 alla prima.
    pub cpu: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocketInfo {
    pub proto: &'static str,
    pub local: String,
    pub remote: String,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LimitRow {
    pub name: String,
    pub soft: String,
    pub hard: String,
    pub units: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CgroupInfo {
    pub path: String,
    pub memory_current: Option<u64>,
    /// `None` se illeggibile; `Some(None)` = "max" (nessun limite).
    pub memory_max: Option<Option<u64>>,
    pub usage_usec: Option<u64>,
    pub nr_throttled: Option<u64>,
    pub throttled_usec: Option<u64>,
    pub pids_current: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProcDetail {
    pub pid: u32,
    pub exe: Option<String>,
    pub cwd: Option<String>,
    pub policy: Option<u32>,
    pub last_core: Option<u32>,
    pub affinity: Option<String>,
    /// Percorso cgroup (riga "0::" di /proc/pid/cgroup).
    pub cgroup_path: Option<String>,
    /// Byte.
    pub pss: Option<u64>,
    pub swap: Option<u64>,
    pub shared: Option<u64>,
    /// Totali letti/scritti dal disco e al secondo.
    pub io: Option<(u64, u64)>,
    pub io_rate: Option<(f64, f64)>,
    pub fd_count: Option<usize>,
    pub fd_limit: Option<String>,
    pub threads: Vec<ThreadInfo>,
    pub files: Vec<String>,
    pub sockets: Vec<SocketInfo>,
    pub env: Vec<String>,
    pub limits: Vec<LimitRow>,
    pub cgroup: Option<CgroupInfo>,
}

/// Nome della politica di scheduling (campo `policy` di stat).
pub fn policy_name(p: u32) -> &'static str {
    match p {
        0 => "OTHER",
        1 => "FIFO",
        2 => "RR",
        3 => "BATCH",
        5 => "IDLE",
        6 => "DEADLINE",
        _ => "?",
    }
}

/// (ultimo core, politica) da /proc/pid/stat: campi 39 e 41 (dopo la ')' sono gli indici 36 e 38).
pub fn parse_stat_extra(text: &str) -> (Option<u32>, Option<u32>) {
    let Some(close) = text.rfind(')') else {
        return (None, None);
    };
    let f: Vec<&str> = text[close + 1..].split_whitespace().collect();
    let g = |i: usize| f.get(i).and_then(|x| x.parse::<u32>().ok());
    (g(36), g(38))
}

/// Valore (in byte) di una riga "Chiave:   123 kB" di status / smaps_rollup.
fn kb_field(text: &str, key: &str) -> Option<u64> {
    text.lines()
        .find_map(|l| l.strip_prefix(key)?.strip_prefix(':'))
        .and_then(|r| r.split_whitespace().next())
        .and_then(|x| x.parse::<u64>().ok())
        .map(|kb| kb * 1024)
}

/// (pss, swap, condivisa) in byte da smaps_rollup.
pub fn parse_smaps_rollup(text: &str) -> (Option<u64>, Option<u64>, Option<u64>) {
    let shared = match (
        kb_field(text, "Shared_Clean"),
        kb_field(text, "Shared_Dirty"),
    ) {
        (None, None) => None,
        (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
    };
    (kb_field(text, "Pss"), kb_field(text, "Swap"), shared)
}

pub fn parse_vmswap(status: &str) -> Option<u64> {
    kb_field(status, "VmSwap")
}

pub fn parse_affinity(status: &str) -> Option<String> {
    status
        .lines()
        .find_map(|l| l.strip_prefix("Cpus_allowed_list:"))
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Percorso cgroup: riga "0::/percorso" (v2); in ripiego la prima riga (v1).
pub fn parse_cgroup_path(text: &str) -> Option<String> {
    let line = text
        .lines()
        .find(|l| l.starts_with("0::"))
        .or_else(|| text.lines().next())?;
    let p = line.splitn(3, ':').nth(2)?.trim();
    (!p.is_empty()).then(|| p.to_string())
}

/// Righe di /proc/pid/limits: colonne separate da due o più spazi.
pub fn parse_limits(text: &str) -> Vec<LimitRow> {
    let mut out = Vec::new();
    for l in text.lines().skip(1) {
        let cols: Vec<&str> = l
            .split("  ")
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .collect();
        if cols.len() >= 3 {
            out.push(LimitRow {
                name: cols[0].to_string(),
                soft: cols[1].to_string(),
                hard: cols[2].to_string(),
                units: cols.get(3).unwrap_or(&"").to_string(),
            });
        }
    }
    out
}

/// "N" o "illimitato"/"unlimited" → testo della soglia "Max open files".
pub fn fd_limit_of(limits: &[LimitRow]) -> Option<String> {
    limits
        .iter()
        .find(|r| r.name == "Max open files")
        .map(|r| r.soft.clone())
}

/// Variabili d'ambiente: una per riga, al massimo `MAX_ENV`.
pub fn parse_environ(raw: &[u8]) -> Vec<String> {
    raw.split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .take(MAX_ENV)
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect()
}

/// Inode di un collegamento fd del tipo "socket:[12345]".
pub fn socket_inode(link: &str) -> Option<u64> {
    link.strip_prefix("socket:[")?
        .strip_suffix(']')?
        .parse()
        .ok()
}

fn fmt_addr(hex: &str) -> Option<String> {
    let (a, p) = hex.split_once(':')?;
    let port = u16::from_str_radix(p, 16).ok()?;
    let ip = match a.len() {
        8 => {
            let v = u32::from_str_radix(a, 16).ok()?;
            // Scritto in ordine di byte della macchina (little endian).
            Ipv4Addr::from(v.swap_bytes()).to_string()
        }
        32 => {
            let mut b = [0u8; 16];
            for i in 0..4 {
                let w = u32::from_str_radix(a.get(i * 8..i * 8 + 8)?, 16).ok()?;
                b[i * 4..i * 4 + 4].copy_from_slice(&w.swap_bytes().to_be_bytes());
            }
            format!("[{}]", Ipv6Addr::from(b))
        }
        _ => return None,
    };
    Some(format!("{ip}:{port}"))
}

fn tcp_state(code: &str) -> &'static str {
    match code {
        "01" => "ESTABLISHED",
        "02" => "SYN_SENT",
        "03" => "SYN_RECV",
        "04" => "FIN_WAIT1",
        "05" => "FIN_WAIT2",
        "06" => "TIME_WAIT",
        "07" => "CLOSE",
        "08" => "CLOSE_WAIT",
        "09" => "LAST_ACK",
        "0A" => "LISTEN",
        "0B" => "CLOSING",
        _ => "?",
    }
}

/// Socket di /proc/net/{tcp,udp}[6] il cui inode è in `inodes`.
pub fn parse_net_table(
    text: &str,
    proto: &'static str,
    inodes: &std::collections::HashSet<u64>,
) -> Vec<SocketInfo> {
    let udp = proto.starts_with("udp");
    let mut out = Vec::new();
    for l in text.lines().skip(1) {
        let f: Vec<&str> = l.split_whitespace().collect();
        let Some(inode) = f.get(9).and_then(|x| x.parse::<u64>().ok()) else {
            continue;
        };
        if !inodes.contains(&inode) {
            continue;
        }
        let (Some(local), Some(remote)) = (
            f.get(1).and_then(|x| fmt_addr(x)),
            f.get(2).and_then(|x| fmt_addr(x)),
        ) else {
            continue;
        };
        let st = f.get(3).copied().unwrap_or("");
        let state = if udp {
            match st {
                "01" => "CONNECTED".to_string(),
                _ => "—".to_string(),
            }
        } else {
            tcp_state(st).to_string()
        };
        out.push(SocketInfo {
            proto,
            local,
            remote,
            state,
        });
    }
    out
}

fn read_u64(s: &dyn Source, path: &Path) -> Option<u64> {
    s.read_text(path)?.trim().parse().ok()
}

/// Valori "chiave numero" di cpu.stat.
pub fn parse_kv(text: &str) -> HashMap<String, u64> {
    text.lines()
        .filter_map(|l| {
            let (k, v) = l.split_once(' ')?;
            Some((k.to_string(), v.trim().parse().ok()?))
        })
        .collect()
}

fn read_cgroup(roots: &Roots, path: &str) -> CgroupInfo {
    let s = &*roots.src;
    // Il percorso viene da /proc: niente ".." per non uscire dalla radice.
    let rel = path.trim_start_matches('/');
    let mut info = CgroupInfo {
        path: path.to_string(),
        ..CgroupInfo::default()
    };
    if rel.split('/').any(|c| c == "..") {
        return info;
    }
    let dir = roots.sys.join("fs/cgroup").join(rel);
    info.memory_current = read_u64(s, &dir.join("memory.current"));
    info.memory_max = s.read_text(&dir.join("memory.max")).and_then(|t| {
        let t = t.trim();
        if t == "max" {
            Some(None)
        } else {
            t.parse().ok().map(Some)
        }
    });
    if let Some(kv) = s.read_text(&dir.join("cpu.stat")).map(|t| parse_kv(&t)) {
        info.usage_usec = kv.get("usage_usec").copied();
        info.nr_throttled = kv.get("nr_throttled").copied();
        info.throttled_usec = kv.get("throttled_usec").copied();
    }
    info.pids_current = read_u64(s, &dir.join("pids.current"));
    info
}

fn link_text(s: &dyn Source, path: &Path) -> Option<String> {
    s.read_link(path)
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
}

/// Raccoglitore con lo stato per le differenze (thread, I/O) del pid selezionato.
#[derive(Debug, Default)]
pub struct DetailCollector {
    pid: Option<u32>,
    prev_io: Option<(u64, u64)>,
    prev_threads: HashMap<u32, u64>,
    /// smaps_rollup è costoso (decine di ms su processi grandi): in cache, riletto ogni 5 giri.
    smaps: Option<(Option<u64>, Option<u64>, Option<u64>)>,
    smaps_age: u32,
}

impl DetailCollector {
    /// Legge il dettaglio di `pid`; `dt` = secondi dall'ultima lettura (<= 0 → velocità a zero).
    pub fn refresh(
        &mut self,
        roots: &Roots,
        pid: u32,
        tab: DetailTab,
        clock_ticks: u64,
        dt: f64,
    ) -> ProcDetail {
        let s = &*roots.src;
        if self.pid != Some(pid) {
            self.pid = Some(pid);
            self.prev_io = None;
            self.prev_threads.clear();
            self.smaps = None;
        }
        let dir = roots.proc.join(pid.to_string());
        let mut d = ProcDetail {
            pid,
            exe: link_text(s, &dir.join("exe")),
            cwd: link_text(s, &dir.join("cwd")),
            ..ProcDetail::default()
        };
        if let Some(t) = s.read_text(&dir.join("stat")) {
            (d.last_core, d.policy) = parse_stat_extra(&t);
        }
        let status = s.read_text(&dir.join("status")).unwrap_or_default();
        d.affinity = parse_affinity(&status);
        d.cgroup_path = s
            .read_text(&dir.join("cgroup"))
            .and_then(|t| parse_cgroup_path(&t));
        // Solo con la scheda Generale, al cambio di processo e poi ogni 5 giri.
        if tab == DetailTab::Generale && (self.smaps.is_none() || self.smaps_age >= 5) {
            self.smaps = Some(
                s.read_text(&dir.join("smaps_rollup"))
                    .map_or((None, None, None), |t| parse_smaps_rollup(&t)),
            );
            self.smaps_age = 0;
        } else {
            self.smaps_age += 1;
        }
        if let Some((pss, swap, shared)) = self.smaps {
            (d.pss, d.swap, d.shared) = (pss, swap, shared);
        }
        if d.swap.is_none() {
            d.swap = parse_vmswap(&status);
        }
        d.io = s
            .read_text(&dir.join("io"))
            .and_then(|t| super::process::parse_io(&t));
        d.io_rate = match (d.io, self.prev_io) {
            (Some((r, w)), Some((pr, pw))) if dt > 0.0 => Some((
                r.saturating_sub(pr) as f64 / dt,
                w.saturating_sub(pw) as f64 / dt,
            )),
            (Some(_), _) => Some((0.0, 0.0)),
            _ => None,
        };
        self.prev_io = d.io;
        d.fd_count = s.read_dir(&dir.join("fd")).ok().map(|r| r.len());
        match tab {
            DetailTab::Thread => d.threads = self.read_threads(s, &dir, clock_ticks, dt),
            DetailTab::File => {
                d.files = read_files(s, &dir);
                d.fd_limit = s
                    .read_text(&dir.join("limits"))
                    .and_then(|t| fd_limit_of(&parse_limits(&t)));
            }
            DetailTab::Rete => d.sockets = read_sockets(roots, &dir),
            DetailTab::Ambiente => {
                d.env = s
                    .read(&dir.join("environ"))
                    .map(|b| parse_environ(&b))
                    .unwrap_or_default();
            }
            DetailTab::Limiti => {
                d.limits = s
                    .read_text(&dir.join("limits"))
                    .map(|t| parse_limits(&t))
                    .unwrap_or_default();
            }
            DetailTab::Cgroup => {
                d.cgroup = d.cgroup_path.as_deref().map(|p| read_cgroup(roots, p));
            }
            DetailTab::Generale => {}
        }
        d
    }

    fn read_threads(&mut self, s: &dyn Source, dir: &Path, hz: u64, dt: f64) -> Vec<ThreadInfo> {
        let mut out = Vec::new();
        let mut now = HashMap::new();
        if let Ok(rd) = s.read_dir(&dir.join("task")) {
            for e in rd {
                let Ok(tid) = e.to_string_lossy().parse::<u32>() else {
                    continue;
                };
                let Some(st) = s
                    .read_text(&dir.join("task").join(&e).join("stat"))
                    .and_then(|t| parse_stat(&t))
                else {
                    continue;
                };
                let ticks = st.utime + st.stime;
                let cpu = match self.prev_threads.get(&tid) {
                    Some(&p) if dt > 0.0 => {
                        (ticks.saturating_sub(p) as f64 / (dt * hz.max(1) as f64) * 100.0)
                            .min(100.0)
                    }
                    _ => 0.0,
                };
                now.insert(tid, ticks);
                out.push(ThreadInfo {
                    tid,
                    name: st.name,
                    state: st.state,
                    cpu,
                });
            }
        }
        self.prev_threads = now;
        out.sort_by(|a, b| b.cpu.total_cmp(&a.cpu).then(a.tid.cmp(&b.tid)));
        out
    }
}

/// Elenco fd → destinazione (al massimo `MAX_FILES`), ordinato per numero di fd.
fn read_files(s: &dyn Source, dir: &Path) -> Vec<String> {
    let mut fds: Vec<(u32, String)> = Vec::new();
    if let Ok(rd) = s.read_dir(&dir.join("fd")) {
        for e in rd {
            let Ok(n) = e.to_string_lossy().parse::<u32>() else {
                continue;
            };
            let target = link_text(s, &dir.join("fd").join(&e)).unwrap_or_else(|| "—".to_string());
            fds.push((n, target));
        }
    }
    fds.sort();
    fds.into_iter()
        .take(MAX_FILES)
        .map(|(n, t)| format!("{n:>4}  {t}"))
        .collect()
}

fn read_sockets(roots: &Roots, dir: &Path) -> Vec<SocketInfo> {
    let s = &*roots.src;
    let mut inodes = std::collections::HashSet::new();
    if let Ok(rd) = s.read_dir(&dir.join("fd")) {
        for e in rd {
            if let Some(i) = link_text(s, &dir.join("fd").join(&e))
                .as_deref()
                .and_then(socket_inode)
            {
                inodes.insert(i);
            }
        }
    }
    let mut out = Vec::new();
    if inodes.is_empty() {
        return out;
    }
    for (file, proto) in [
        ("tcp", "tcp"),
        ("tcp6", "tcp6"),
        ("udp", "udp"),
        ("udp6", "udp6"),
    ] {
        if let Some(t) = s.read_text(&roots.proc.join("net").join(file)) {
            out.extend(parse_net_table(&t, proto, &inodes));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::fs;
    use std::path::PathBuf;

    fn roots() -> Roots {
        let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/snap_a");
        Roots::with_dirs(d.join("proc"), d.join("sys"), d.join("etc"))
    }

    #[test]
    fn stat_extra_e_politica() {
        let t = "100 (a b) c) S 50 1 1 0 -1 4194304 0 0 0 0 1 2 0 0 20 0 2 0 1000 100000 2560 0 0 0 0 0 0 0 0 0 0 0 0 0 17 5 0 1 0 0 0";
        assert_eq!(parse_stat_extra(t), (Some(5), Some(1)));
        assert_eq!(parse_stat_extra("100 (x) S 1"), (None, None));
        assert_eq!(parse_stat_extra("rotto"), (None, None));
        assert_eq!(policy_name(0), "OTHER");
        assert_eq!(policy_name(2), "RR");
        assert_eq!(policy_name(99), "?");
    }

    #[test]
    fn smaps_status_cgroup_limiti() {
        let sm =
            "Rss: 2000 kB\nPss: 1500 kB\nShared_Clean: 300 kB\nShared_Dirty: 100 kB\nSwap: 50 kB\n";
        assert_eq!(
            parse_smaps_rollup(sm),
            (Some(1500 * 1024), Some(50 * 1024), Some(400 * 1024))
        );
        assert_eq!(parse_smaps_rollup("niente"), (None, None, None));
        assert_eq!(parse_vmswap("VmSwap:\t  8 kB\n"), Some(8192));
        assert_eq!(
            parse_affinity("Cpus_allowed_list:\t0-3,6\n"),
            Some("0-3,6".to_string())
        );
        assert_eq!(
            parse_cgroup_path("0::/user.slice/app.scope\n"),
            Some("/user.slice/app.scope".to_string())
        );
        assert_eq!(
            parse_cgroup_path("12:cpu:/old\n1:name=x:/y\n"),
            Some("/old".to_string())
        );
        assert_eq!(parse_cgroup_path("0::/\n"), Some("/".to_string()));
        let lim = "Limit                     Soft Limit           Hard Limit           Units     \nMax cpu time              unlimited            unlimited            seconds   \nMax open files            1024                 524288               files     \n";
        let rows = parse_limits(lim);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].name, "Max open files");
        assert_eq!(rows[1].hard, "524288");
        assert_eq!(rows[0].units, "seconds");
        assert_eq!(fd_limit_of(&rows), Some("1024".to_string()));
        assert_eq!(
            parse_environ(b"A=1\0B=due parole\0\0"),
            ["A=1", "B=due parole"]
        );
    }

    #[test]
    fn tabelle_di_rete() {
        let tcp = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n   0: 0100007F:0035 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 5555 1 0\n   1: 0100007F:1F90 0201A8C0:C350 01 00000000:00000000 00:00000000 00000000     0        0 9999 1 0\n";
        let inodes: HashSet<u64> = [5555].into_iter().collect();
        let v = parse_net_table(tcp, "tcp", &inodes);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].local, "127.0.0.1:53");
        assert_eq!(v[0].remote, "0.0.0.0:0");
        assert_eq!(v[0].state, "LISTEN");
        let both: HashSet<u64> = [5555, 9999].into_iter().collect();
        let v = parse_net_table(tcp, "tcp", &both);
        assert_eq!(v[1].remote, "192.168.1.2:50000");
        assert_eq!(v[1].state, "ESTABLISHED");
        let t6 = "hdr\n 0: 00000000000000000000000001000000:0016 00000000000000000000000000000000:0000 0A 0:0 0:0 0 0 0 4444 1\n";
        let i6: HashSet<u64> = [4444].into_iter().collect();
        let v = parse_net_table(t6, "tcp6", &i6);
        assert_eq!(v[0].local, "[::1]:22");
        assert_eq!(socket_inode("socket:[77]"), Some(77));
        assert_eq!(socket_inode("/dev/null"), None);
        // righe rotte: ignorate
        assert!(parse_net_table("hdr\nxx\n 0: zz\n", "udp", &both).is_empty());
    }

    #[test]
    fn lettura_da_fixture() {
        let r = roots();
        let mut c = DetailCollector::default();
        let d = c.refresh(&r, 100, DetailTab::Generale, 100, 1.0);
        assert_eq!(d.pid, 100);
        assert_eq!(d.affinity.as_deref(), Some("0-3"));
        assert_eq!(d.cgroup_path.as_deref(), Some("/user.slice/app.scope"));
        assert_eq!(d.pss, Some(1500 * 1024));
        assert_eq!(d.shared, Some(400 * 1024));
        assert_eq!(d.io_rate, Some((0.0, 0.0)));
        // pid inesistente: tutto None/vuoto, niente panic
        let n = c.refresh(&r, 99999, DetailTab::Thread, 100, 1.0);
        assert!(n.exe.is_none() && n.threads.is_empty() && n.io.is_none());
    }

    #[test]
    fn schede_da_fixture() {
        let r = roots();
        let mut c = DetailCollector::default();
        let t = c.refresh(&r, 100, DetailTab::Thread, 100, 1.0).threads;
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].cpu, 0.0); // prima lettura
        assert!(t.iter().any(|x| x.name == "worker"));
        let f = c.refresh(&r, 100, DetailTab::File, 100, 1.0);
        assert_eq!(f.files.len(), 3);
        assert!(f.files[0].contains("/dev/null"));
        assert_eq!(f.fd_limit.as_deref(), Some("1024"));
        let n = c.refresh(&r, 100, DetailTab::Rete, 100, 1.0);
        assert_eq!(n.sockets.len(), 1);
        assert_eq!(n.sockets[0].local, "127.0.0.1:53");
        let e = c.refresh(&r, 100, DetailTab::Ambiente, 100, 1.0);
        assert_eq!(e.env, ["HOME=/home/x", "LANG=it_IT.UTF-8"]);
        let l = c.refresh(&r, 100, DetailTab::Limiti, 100, 1.0);
        assert_eq!(l.limits.len(), 2);
        let g = c
            .refresh(&r, 100, DetailTab::Cgroup, 100, 1.0)
            .cgroup
            .unwrap();
        assert_eq!(g.path, "/user.slice/app.scope");
        assert_eq!(g.memory_current, Some(1048576));
        assert_eq!(g.memory_max, Some(None));
        assert_eq!(g.usage_usec, Some(5000000));
        assert_eq!(g.nr_throttled, Some(3));
        assert_eq!(g.pids_current, Some(12));
    }

    #[test]
    fn delta_thread_e_io() {
        let dir = std::env::temp_dir().join(format!("ngtop-pd-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let p = dir.join("proc/7");
        fs::create_dir_all(p.join("task/7")).unwrap();
        let stat = |ticks: u64| {
            format!(
                "7 (t) R 1 1 1 0 -1 0 0 0 0 0 {ticks} 0 0 0 20 0 1 0 5 100 10 0 0 0 0 0 0 0 0 0 0 0 0 0 17 0 0 0\n"
            )
        };
        let io = |r: u64| format!("read_bytes: {r}\nwrite_bytes: 0\n");
        let r = Roots::with_dirs(dir.join("proc"), dir.join("sys"), dir.join("etc"));
        let mut c = DetailCollector::default();
        fs::write(p.join("task/7/stat"), stat(100)).unwrap();
        fs::write(p.join("io"), io(1000)).unwrap();
        c.refresh(&r, 7, DetailTab::Thread, 100, 1.0);
        fs::write(p.join("task/7/stat"), stat(150)).unwrap();
        fs::write(p.join("io"), io(3000)).unwrap();
        let d = c.refresh(&r, 7, DetailTab::Thread, 100, 2.0);
        // 50 tick in 2 s a 100 Hz = 25%
        assert!((d.threads[0].cpu - 25.0).abs() < 1e-9);
        assert_eq!(d.io_rate, Some((1000.0, 0.0)));
        // cambio pid → differenze azzerate
        let d = c.refresh(&r, 99, DetailTab::Thread, 100, 1.0);
        assert!(d.threads.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }
}
