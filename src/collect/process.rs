// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Processi: lettura di /proc/[pid], CPU% e I/O al secondo, albero, ordinamento.

use std::borrow::Cow;
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use super::read_text;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Process {
    pub pid: u32,
    pub ppid: u32,
    pub name: String,
    pub state: char,
    pub uid: u32,
    pub user: String,
    pub cmdline: String,
    pub threads: u32,
    pub nice: i64,
    pub priority: i64,
    /// Memoria residente in byte.
    pub rss: u64,
    /// Tempo CPU totale (utente + sistema) in tick.
    pub cpu_ticks: u64,
    /// Tempo CPU totale in secondi.
    pub cpu_time: f64,
    /// Avvio in tick dal boot (identifica il processo contro il riuso dei pid).
    pub start_ticks: u64,
    /// Avvio in secondi dal boot.
    pub start_secs: f64,
    /// Byte letti/scritti dal disco; `None` se /proc/[pid]/io non è leggibile.
    pub io_read: Option<u64>,
    pub io_write: Option<u64>,
    /// % della macchina intera (0–100).
    pub cpu_percent: f64,
    /// Byte al secondo.
    pub io_read_rate: Option<f64>,
    pub io_write_rate: Option<f64>,
    /// % di GPU (somma sui dispositivi) e memoria GPU in byte, riempiti da `System`.
    pub gpu_percent: f64,
    pub gpu_mem: u64,
}

/// Campione precedente di un processo, per le differenze.
#[derive(Debug, Clone)]
pub struct ProcPrev {
    start_ticks: u64,
    cpu_ticks: u64,
    io_read: Option<u64>,
    io_write: Option<u64>,
    name: String,
    /// Proprietario, riletto da status solo per processi nuovi (stesso avvio e nome).
    uid: u32,
    /// Oggi inutilizzato (`want_io` è sempre falso; serve al passo 8).
    /// /proc/[pid]/io non era leggibile: non si riprova finché il processo è lo stesso.
    io_denied: bool,
    /// Riga di comando letta con successo (`None` se vuota o illeggibile: si ritenta).
    cmdline: Option<String>,
}

impl ProcPrev {
    /// `io_tried`: in questo giro si è provato a leggere io (altrimenti non si sa se è negato).
    pub fn from_process(p: &Process, io_tried: bool) -> Self {
        ProcPrev {
            io_denied: io_tried && p.io_read.is_none(),
            ..ProcPrev::from(p)
        }
    }
}

impl From<&Process> for ProcPrev {
    fn from(p: &Process) -> Self {
        ProcPrev {
            start_ticks: p.start_ticks,
            cpu_ticks: p.cpu_ticks,
            io_read: p.io_read,
            io_write: p.io_write,
            name: p.name.clone(),
            uid: p.uid,
            io_denied: false,
            cmdline: Some(p.cmdline.clone()).filter(|c| *c != format!("[{}]", p.name)),
        }
    }
}

/// Campi utili di /proc/[pid]/stat.
#[derive(Debug, Clone, PartialEq)]
pub struct StatInfo {
    pub name: String,
    pub state: char,
    pub ppid: u32,
    pub utime: u64,
    pub stime: u64,
    pub priority: i64,
    pub nice: i64,
    pub threads: u32,
    pub starttime: u64,
    pub rss_pages: u64,
}

/// Il nome (comm) è tra parentesi e può contenere spazi e ')': si usa l'ULTIMA ')'.
pub fn parse_stat(text: &str) -> Option<StatInfo> {
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    if close < open {
        return None;
    }
    let name = text[open + 1..close].to_string();
    let f: Vec<&str> = text[close + 1..].split_whitespace().collect();
    let u = |i: usize| f.get(i).and_then(|x| x.parse::<u64>().ok());
    let s = |i: usize| f.get(i).and_then(|x| x.parse::<i64>().ok());
    Some(StatInfo {
        name,
        state: f.first()?.chars().next()?,
        ppid: u(1)? as u32,
        utime: u(11)?,
        stime: u(12)?,
        priority: s(15)?,
        nice: s(16)?,
        threads: u(17)? as u32,
        starttime: u(19)?,
        rss_pages: s(21)?.max(0) as u64,
    })
}

/// Primo valore della riga "Uid:" di /proc/[pid]/status.
pub fn parse_status_uid(text: &str) -> Option<u32> {
    text.lines()
        .find_map(|l| l.strip_prefix("Uid:"))
        .and_then(|r| r.split_whitespace().next())
        .and_then(|x| x.parse().ok())
}

/// (read_bytes, write_bytes) da /proc/[pid]/io.
pub fn parse_io(text: &str) -> Option<(u64, u64)> {
    let mut r = None;
    let mut w = None;
    for l in text.lines() {
        if let Some(v) = l.strip_prefix("read_bytes:") {
            r = v.trim().parse().ok();
        } else if let Some(v) = l.strip_prefix("write_bytes:") {
            w = v.trim().parse().ok();
        }
    }
    Some((r?, w?))
}

/// Mappa uid → nome da un file passwd.
pub fn parse_passwd(text: &str) -> HashMap<u32, String> {
    let mut m = HashMap::new();
    for l in text.lines() {
        let f: Vec<&str> = l.split(':').collect();
        if let (Some(name), Some(uid)) = (f.first(), f.get(2).and_then(|x| x.parse::<u32>().ok()))
            && !name.is_empty()
        {
            m.entry(uid).or_insert_with(|| name.to_string());
        }
    }
    m
}

pub fn load_users(path: &Path) -> HashMap<u32, String> {
    read_text(path)
        .map(|t| parse_passwd(&t))
        .unwrap_or_default()
}

fn cmdline_text(raw: &[u8], name: &str) -> String {
    let s = String::from_utf8_lossy(raw);
    let s = s.trim_end_matches('\0').replace('\0', " ");
    let s = s.trim();
    if s.is_empty() {
        format!("[{name}]")
    } else {
        s.to_string()
    }
}

/// Legge un file piccolo in un buffer riusato (niente allocazioni per file).
fn read_into<'a>(path: &Path, buf: &'a mut Vec<u8>) -> Option<Cow<'a, str>> {
    buf.clear();
    File::open(path).ok()?.read_to_end(buf).ok()?;
    Some(String::from_utf8_lossy(buf))
}

/// Legge un singolo processo (`dir` = /proc/<pid>, usata come percorso riusabile);
/// `None` se è sparito o illeggibile.
#[allow(clippy::too_many_arguments)]
fn read_one(
    dir: &mut PathBuf,
    pid: u32,
    hz: u64,
    page_size: u64,
    users: &HashMap<u32, String>,
    prev: Option<&ProcPrev>,
    want_io: bool,
    round: u64,
    buf: &mut Vec<u8>,
) -> Option<Process> {
    dir.push("stat");
    let text = read_into(dir, buf);
    dir.pop();
    let st = parse_stat(&text?)?;
    // Dopo un exec cambia il comm: la cache vale solo se avvio e nome coincidono.
    let same = prev.filter(|o| o.start_ticks == st.starttime && o.name == st.name);
    // Il proprietario si rilegge solo per processi nuovi; se status non è leggibile si usa
    // il proprietario della cartella, altrimenti si salta.
    // Anche per i processi noti lo si rilegge ogni 15 giri (sfalsato per pid): un demone può
    // cambiare utente senza exec.
    let recheck = (round + u64::from(pid)).is_multiple_of(15);
    let uid = match same {
        Some(o) if !recheck => o.uid,
        _ => {
            dir.push("status");
            let t = read_into(dir, buf).and_then(|t| parse_status_uid(&t));
            dir.pop();
            t.or_else(|| fs::metadata(&*dir).ok().map(|m| m.uid()))?
        }
    };
    let user = users.get(&uid).cloned().unwrap_or_else(|| uid.to_string());
    // La riga di comando si rilegge solo per processi nuovi o dopo un exec.
    let cmdline = match same.and_then(|o| o.cmdline.clone()) {
        Some(c) => c,
        None => {
            dir.push("cmdline");
            let raw = fs::read(&*dir).unwrap_or_default();
            dir.pop();
            cmdline_text(&raw, &st.name)
        }
    };
    // I/O solo se richiesto, e senza riprovare dopo un permesso negato.
    let io = if want_io && !same.is_some_and(|o| o.io_denied) {
        dir.push("io");
        let r = read_into(dir, buf).and_then(|t| parse_io(&t));
        dir.pop();
        r
    } else {
        None
    };
    let hzf = hz.max(1) as f64;
    let cpu_ticks = st.utime + st.stime;
    Some(Process {
        pid,
        ppid: st.ppid,
        name: st.name,
        state: st.state,
        uid,
        user,
        cmdline,
        threads: st.threads,
        nice: st.nice,
        priority: st.priority,
        rss: st.rss_pages.saturating_mul(page_size),
        cpu_ticks,
        cpu_time: cpu_ticks as f64 / hzf,
        start_ticks: st.starttime,
        start_secs: st.starttime as f64 / hzf,
        io_read: io.map(|x| x.0),
        io_write: io.map(|x| x.1),
        ..Process::default()
    })
}

/// Legge tutti i processi e calcola CPU% e velocità I/O rispetto al campione precedente.
#[allow(clippy::too_many_arguments)]
pub fn read_all(
    proc_root: &Path,
    hz: u64,
    page_size: u64,
    users: &HashMap<u32, String>,
    prev: &HashMap<u32, ProcPrev>,
    dt: f64,
    ncores: f64,
    want_io: bool,
    round: u64,
) -> Vec<Process> {
    let mut out = Vec::new();
    let Ok(rd) = fs::read_dir(proc_root) else {
        return out;
    };
    let mut buf = Vec::with_capacity(4096);
    let mut path = proc_root.to_path_buf();
    for e in rd.flatten() {
        let name = e.file_name();
        let Some(pid) = name.to_str().and_then(|n| n.parse::<u32>().ok()) else {
            continue;
        };
        let old = prev.get(&pid);
        path.push(&name);
        let p = read_one(
            &mut path, pid, hz, page_size, users, old, want_io, round, &mut buf,
        );
        path.pop();
        let Some(mut p) = p else {
            continue;
        };
        if dt > 0.0
            && let Some(old) = old.filter(|o| o.start_ticks == p.start_ticks)
        {
            let dticks = p.cpu_ticks.saturating_sub(old.cpu_ticks) as f64;
            p.cpu_percent =
                (dticks / hz.max(1) as f64 / dt / ncores.max(1.0) * 100.0).clamp(0.0, 100.0);
            let rate = |now: Option<u64>, before: Option<u64>| match (now, before) {
                (Some(n), Some(b)) => Some(n.saturating_sub(b) as f64 / dt),
                _ => None,
            };
            p.io_read_rate = rate(p.io_read, old.io_read);
            p.io_write_rate = rate(p.io_write, old.io_write);
        }
        out.push(p);
    }
    out.sort_by_key(|p| p.pid);
    out
}

/// Indici dei processi in ordine di albero con il prefisso grafico.
/// Figli ordinati per pid; gli orfani (ppid inesistente) sono radici.
pub fn tree_order(procs: &[Process]) -> Vec<(usize, String)> {
    let by_pid: HashMap<u32, usize> = procs.iter().enumerate().map(|(i, p)| (p.pid, i)).collect();
    let mut children: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut roots = Vec::new();
    for (i, p) in procs.iter().enumerate() {
        match by_pid.get(&p.ppid) {
            Some(&parent) if parent != i => children.entry(parent).or_default().push(i),
            _ => roots.push(i),
        }
    }
    let by_pid_order = |v: &mut Vec<usize>| v.sort_by_key(|&i| procs[i].pid);
    by_pid_order(&mut roots);
    for v in children.values_mut() {
        by_pid_order(v);
    }

    let mut out = Vec::with_capacity(procs.len());
    let mut seen = HashSet::new();
    // Pila: (indice, prefisso proprio, base del prefisso per i figli).
    let walk = |root: usize, out: &mut Vec<(usize, String)>, seen: &mut HashSet<usize>| {
        let mut stack = vec![(root, String::new(), String::new())];
        while let Some((i, own, base)) = stack.pop() {
            if !seen.insert(i) {
                continue;
            }
            out.push((i, own));
            if let Some(kids) = children.get(&i) {
                for (n, &k) in kids.iter().enumerate().rev() {
                    let last = n + 1 == kids.len();
                    let (conn, cont) = if last {
                        ("└─ ", "   ")
                    } else {
                        ("├─ ", "│  ")
                    };
                    stack.push((k, format!("{base}{conn}"), format!("{base}{cont}")));
                }
            }
        }
    };
    for r in roots {
        walk(r, &mut out, &mut seen);
    }
    // Cicli di ppid (impossibili in pratica): i rimasti diventano radici.
    let mut rest: Vec<usize> = (0..procs.len()).filter(|i| !seen.contains(i)).collect();
    rest.sort_by_key(|&i| procs[i].pid);
    for r in rest {
        walk(r, &mut out, &mut seen);
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Pid,
    Name,
    Cpu,
    Mem,
    User,
    Gpu,
    Vram,
}

/// Ordina per colonna; a parità, per pid crescente.
pub fn sort_processes(procs: &mut [Process], key: SortKey, descending: bool) {
    match key {
        SortKey::Name | SortKey::User => {
            let text = |p: &Process| {
                if key == SortKey::Name {
                    p.name.to_lowercase()
                } else {
                    p.user.to_lowercase()
                }
            };
            if descending {
                procs.sort_by_cached_key(|p| (Reverse(text(p)), p.pid));
            } else {
                procs.sort_by_cached_key(|p| (text(p), p.pid));
            }
        }
        _ => procs.sort_by(|a, b| {
            let ord = match key {
                SortKey::Pid => a.pid.cmp(&b.pid),
                SortKey::Cpu => a.cpu_percent.total_cmp(&b.cpu_percent),
                SortKey::Gpu => a.gpu_percent.total_cmp(&b.gpu_percent),
                SortKey::Vram => a.gpu_mem.cmp(&b.gpu_mem),
                _ => a.rss.cmp(&b.rss),
            };
            let ord = if descending { ord.reverse() } else { ord };
            ord.then(a.pid.cmp(&b.pid))
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, ppid: u32) -> Process {
        Process {
            pid,
            ppid,
            name: format!("p{pid}"),
            ..Process::default()
        }
    }

    #[test]
    fn stat_con_nome_difficile() {
        let t = "42 (a) b)) S 1 2 3 4 5 6 7 8 9 10 100 20 0 0 25 5 3 0 777 1000 55 rest";
        let s = parse_stat(t).unwrap();
        assert_eq!(s.name, "a) b)");
        assert_eq!((s.state, s.ppid, s.utime, s.stime), ('S', 1, 100, 20));
        assert_eq!((s.priority, s.nice, s.threads), (25, 5, 3));
        assert_eq!((s.starttime, s.rss_pages), (777, 55));
        let s = parse_stat("1 (Web Content) R 0 0 0 0 0 0 0 0 0 0 1 2 0 0 20 0 1 0 5 0 0").unwrap();
        assert_eq!(s.name, "Web Content");
        assert!(parse_stat("").is_none());
        assert!(parse_stat("1 (x) S 1").is_none());
        assert!(parse_stat(") (").is_none());
    }

    #[test]
    fn status_io_passwd() {
        assert_eq!(
            parse_status_uid("Name:\tx\nUid:\t1000\t1000\t1000\t1000\n"),
            Some(1000)
        );
        assert_eq!(parse_status_uid("Name: x"), None);
        assert_eq!(
            parse_io("rchar: 1\nread_bytes: 10\nwrite_bytes: 20\n"),
            Some((10, 20))
        );
        assert_eq!(parse_io("read_bytes: 10\n"), None);
        let m = parse_passwd("root:x:0:0:r:/root:/bin/sh\nbad\nnic:x:1000:1000::/h:/bin/sh\n");
        assert_eq!(m.get(&1000).map(String::as_str), Some("nic"));
        assert_eq!(m.len(), 2);
    }

    fn r1(dir: &std::path::Path, users: &HashMap<u32, String>, prev: Option<&ProcPrev>) -> Process {
        let mut d = dir.to_path_buf();
        read_one(&mut d, 5, 100, 4096, users, prev, true, 1, &mut Vec::new()).unwrap()
    }

    #[test]
    fn cache_cmdline_invalidata_da_exec() {
        let dir = std::env::temp_dir().join(format!("ngtop-t-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap_or(());
        let stat =
            |name: &str| format!("5 ({name}) S 1 1 1 0 -1 0 0 0 0 0 1 1 0 0 20 0 1 0 77 0 0");
        fs::write(dir.join("stat"), stat("vecchio")).unwrap_or(());
        fs::write(dir.join("status"), "Uid:\t0\t0\t0\t0\n").unwrap_or(());
        fs::write(dir.join("cmdline"), b"vecchio\0-a\0").unwrap_or(());
        let users = HashMap::new();
        let p1 = r1(&dir, &users, None);
        assert_eq!(p1.cmdline, "vecchio -a");
        let prev = ProcPrev::from(&p1);
        // stesso pid e avvio, ma cmdline e comm cambiati (exec)
        fs::write(dir.join("stat"), stat("nuovo")).unwrap_or(());
        fs::write(dir.join("cmdline"), b"nuovo\0-b\0").unwrap_or(());
        let p2 = r1(&dir, &users, Some(&prev));
        assert_eq!(p2.cmdline, "nuovo -b");
        // senza cambio la cache viene riusata (anche se il file cambia)
        fs::write(dir.join("cmdline"), b"altro\0").unwrap_or(());
        let p3 = r1(&dir, &users, Some(&ProcPrev::from(&p2)));
        assert_eq!(p3.cmdline, "nuovo -b");
        // lettura fallita (ripiego "[comm]"): non in cache, si ritenta
        fs::write(dir.join("cmdline"), b"").unwrap_or(());
        let p4 = r1(&dir, &users, None);
        assert_eq!(p4.cmdline, "[nuovo]");
        fs::write(dir.join("cmdline"), b"ora\0").unwrap_or(());
        let p5 = r1(&dir, &users, Some(&ProcPrev::from(&p4)));
        assert_eq!(p5.cmdline, "ora");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn io_negato_non_si_riprova_e_uid_in_cache() {
        let dir = std::env::temp_dir().join(format!("ngtop-io-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap_or(());
        fs::write(
            dir.join("stat"),
            "5 (x) S 1 1 1 0 -1 0 0 0 0 0 1 1 0 0 20 0 1 0 77 0 0",
        )
        .unwrap_or(());
        fs::write(dir.join("status"), "Uid:\t7\t7\t7\t7\n").unwrap_or(());
        let users = HashMap::new();
        let p1 = r1(&dir, &users, None);
        assert_eq!((p1.uid, p1.io_read), (7, None));
        let prev = ProcPrev::from_process(&p1, true);
        // ora io esiste e status cambia: con la cache non si rilegge niente di tutto ciò
        fs::write(dir.join("io"), "read_bytes: 5\nwrite_bytes: 6\n").unwrap_or(());
        fs::write(dir.join("status"), "Uid:\t9\t9\t9\t9\n").unwrap_or(());
        let p2 = r1(&dir, &users, Some(&prev));
        assert_eq!((p2.uid, p2.io_read), (7, None));
        // processo nuovo (senza cache): si legge tutto
        let p3 = r1(&dir, &users, None);
        assert_eq!((p3.uid, p3.io_read), (9, Some(5)));
        // senza il flag non si legge io
        let mut d = dir.clone();
        let p4 = read_one(
            &mut d,
            5,
            100,
            4096,
            &users,
            None,
            false,
            1,
            &mut Vec::new(),
        )
        .unwrap();
        // ogni 15 giri (sfalsato per pid: round 10 per il pid 5) lo status si rilegge anche se noto
        fs::write(dir.join("status"), "Uid:\t11\t11\t11\t11\n").unwrap_or(());
        let mut d = dir.clone();
        let p5 = read_one(
            &mut d,
            5,
            100,
            4096,
            &users,
            Some(&prev),
            true,
            10,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(p5.uid, 11);
        assert_eq!(p4.io_read, None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn cmdline() {
        assert_eq!(cmdline_text(b"a\0-b\0", "x"), "a -b");
        assert_eq!(cmdline_text(b"", "kthread"), "[kthread]");
    }

    #[test]
    fn albero_prefissi_e_orfani() {
        let v = vec![p(1, 0), p(2, 1), p(3, 2), p(4, 2), p(5, 1), p(9, 777)];
        let t: Vec<(u32, String)> = tree_order(&v)
            .into_iter()
            .map(|(i, s)| (v[i].pid, s))
            .collect();
        let want = [
            (1, ""),
            (2, "├─ "),
            (3, "│  ├─ "),
            (4, "│  └─ "),
            (5, "└─ "),
            (9, ""),
        ];
        assert_eq!(t.len(), want.len());
        for (a, b) in t.iter().zip(want) {
            assert_eq!((a.0, a.1.as_str()), b);
        }
    }

    #[test]
    fn albero_con_ciclo_non_si_blocca() {
        let v = vec![p(1, 2), p(2, 1), p(3, 3)];
        assert_eq!(tree_order(&v).len(), 3);
        assert!(tree_order(&[]).is_empty());
    }

    #[test]
    fn ordinamento() {
        let mut v = vec![p(3, 1), p(1, 1), p(2, 1)];
        v[0].cpu_percent = 5.0;
        v[1].cpu_percent = 9.0;
        sort_processes(&mut v, SortKey::Cpu, true);
        assert_eq!(v.iter().map(|x| x.pid).collect::<Vec<_>>(), [1, 3, 2]);
        sort_processes(&mut v, SortKey::Pid, false);
        assert_eq!(v.iter().map(|x| x.pid).collect::<Vec<_>>(), [1, 2, 3]);
        sort_processes(&mut v, SortKey::Name, true);
        assert_eq!(v[0].pid, 3);
    }
}
