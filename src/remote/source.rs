// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! `RemoteSource` e `RemoteRunner` sopra la sessione condivisa.
//!
//! Cache per aggiornamento: tutto ciò che è stato letto tra due `begin_refresh` forma il "manifesto"
//! del successivo, richiesto in UNA andata e ritorno. I percorsi per processo (`/proc/<pid>/stat`) si
//! generalizzano in modelli (`/proc/[0-9]*/stat`) quando almeno `PATTERN_MIN` pid leggono lo stesso file:
//! così i processi nati dopo sono già inclusi, e un percorso coperto da un modello ma assente nella
//! risposta è per definizione inesistente (niente richiesta).

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::io;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::Duration;

use super::session::{Rec, Session, Transport, rc_error, sh_quote};
use crate::source::{CommandOutput, CommandRunner, FsStat, RemoteState, RemoteStatus, Source};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const PATTERN_MIN: usize = 3;
/// Link, uid e cartelle fuori da /proc (es. /sys/block) cambiano di rado: si rileggono ogni tanti giri.
const STICKY_LAPS: u32 = 10;
/// I percorsi non per-pid letti di recente restano nel manifesto (letture periodiche ogni pochi giri).
const RECENT_LAPS: u32 = 8;

fn sticky(k: &Key) -> bool {
    matches!(k.0, Kind::Link | Kind::Uid | Kind::Dir) && !k.1.starts_with(b"/proc")
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Kind {
    File,
    Dir,
    Link,
    Uid,
    Stat,
}

impl Kind {
    fn code(self) -> u8 {
        match self {
            Kind::File => b'F',
            Kind::Dir => b'D',
            Kind::Link => b'L',
            Kind::Uid => b'U',
            Kind::Stat => b'S',
        }
    }
    fn from_code(c: u8) -> Option<Kind> {
        Some(match c {
            b'F' => Kind::File,
            b'D' => Kind::Dir,
            b'L' => Kind::Link,
            b'U' => Kind::Uid,
            b'S' => Kind::Stat,
            _ => return None,
        })
    }
    fn generalizable(self) -> bool {
        matches!(self, Kind::File | Kind::Dir | Kind::Link | Kind::Uid)
    }
}

type Key = (Kind, Vec<u8>);

/// Modello di un percorso per processo: i componenti numerici di `/proc/<pid>` (e `/task/<tid>`)
/// diventano `[0-9]*`. `None` se il percorso non ne contiene.
pub(super) fn template(path: &[u8]) -> Option<Vec<u8>> {
    let rest = path.strip_prefix(b"/proc/")?;
    let comps: Vec<&[u8]> = rest.split(|&b| b == b'/').collect();
    let num = |c: &[u8]| !c.is_empty() && c.iter().all(u8::is_ascii_digit);
    if !num(comps[0]) {
        return None;
    }
    let mut out = b"/proc/[0-9]*".to_vec();
    for (i, c) in comps.iter().enumerate().skip(1) {
        out.push(b'/');
        if i == 2 && (comps[1] == b"task" || comps[1] == b"fd") && num(c) {
            out.extend_from_slice(b"[0-9]*");
        } else {
            out.extend_from_slice(c);
        }
    }
    Some(out)
}

#[derive(Default)]
struct Cache {
    entries: HashMap<Key, (i32, Vec<u8>)>,
    /// Modelli richiesti nell'ultimo aggiornamento (copertura negativa).
    covered: HashSet<Key>,
    /// Tutto ciò che è stato letto dall'ultimo `begin_refresh`.
    touched: HashSet<Key>,
    /// Letture del ciclo precedente: un percorso esatto entra nel manifesto solo se letto in due cicli di fila.
    prev_touched: HashSet<Key>,
    /// Pid presenti all'ultimo aggiornamento e quelli comparsi rispetto al precedente.
    pids: HashSet<Vec<u8>>,
    new_pids: Vec<Vec<u8>>,
    /// Letture fuori cache per modello nel ciclo corrente (alla seconda si carica per tutti i pid).
    misses: HashMap<Key, u32>,
    /// Aggiornamenti riusciti finora (nei primi due il manifesto iniziale si somma a quello appreso).
    laps: u32,
    /// Cartelle fd i cui link sono già stati caricati in blocco: un link assente è inesistente.
    link_dirs: HashSet<Vec<u8>>,
    /// Ultimo giro in cui è stato letto ciascun percorso non per-pid: resta nel manifesto per `RECENT_LAPS` giri.
    last_seen: HashMap<Key, u32>,
}

#[derive(Default)]
struct StatusInfo {
    hostname: Option<String>,
    latency_ms: Option<u32>,
    last_bytes: u64,
    state: Option<RemoteState>,
}

struct Shared {
    transport: Transport,
    target: String,
    sess: Mutex<Option<Session>>,
    status: Mutex<StatusInfo>,
    reconnecting: AtomicBool,
}

pub struct RemoteSource {
    sh: Arc<Shared>,
    cache: Mutex<Cache>,
}

pub struct RemoteRunner {
    sh: Arc<Shared>,
}

pub(super) fn open(
    tr: Transport,
    target: String,
) -> io::Result<(Arc<RemoteSource>, Arc<RemoteRunner>)> {
    let sess = Session::start(&tr, true)?;
    let sh = Arc::new(Shared {
        transport: tr,
        target,
        sess: Mutex::new(Some(sess)),
        status: Mutex::new(StatusInfo {
            state: Some(RemoteState::Connected),
            ..Default::default()
        }),
        reconnecting: AtomicBool::new(false),
    });
    let src = Arc::new(RemoteSource {
        sh: sh.clone(),
        cache: Mutex::new(Cache::default()),
    });
    let _ = src.sh.refresh_hostname();
    // manifesto iniziale subito: le prime letture (prima di ogni aggiornamento) trovano già i dati in cache
    src.begin_refresh();
    Ok((src.clone(), Arc::new(RemoteRunner { sh })))
}

impl Shared {
    fn connected(&self) -> bool {
        matches!(lock(&self.status).state, Some(RemoteState::Connected))
    }

    fn not_connected(&self) -> io::Error {
        let msg = match &lock(&self.status).state {
            Some(RemoteState::Reconnecting(m)) | Some(RemoteState::Failed(m)) => m.clone(),
            _ => "not connected".into(),
        };
        io::Error::new(io::ErrorKind::NotConnected, msg)
    }

    /// Una richiesta sulla sessione (serializzata dal Mutex). Se fallisce la sessione si scarta e parte
    /// la riconnessione in sottofondo.
    fn request(self: &Arc<Self>, lines: &[u8], timeout: Duration) -> io::Result<(Vec<Rec>, u64)> {
        let mut g = lock(&self.sess);
        let Some(s) = g.as_mut() else {
            return Err(self.not_connected());
        };
        let before = s.bytes_in.load(Ordering::Relaxed);
        match s.exchange(lines, timeout) {
            Ok((recs, rtt)) => {
                let bytes = s.bytes_in.load(Ordering::Relaxed) - before;
                lock(&self.status).latency_ms = Some(rtt.as_millis().min(u32::MAX as u128) as u32);
                Ok((recs, bytes))
            }
            Err(e) => {
                *g = None;
                drop(g);
                self.lost(e.to_string());
                Err(e)
            }
        }
    }

    fn lost(self: &Arc<Self>, msg: String) {
        lock(&self.status).state = Some(RemoteState::Reconnecting(msg));
        if self.reconnecting.swap(true, Ordering::SeqCst) {
            return;
        }
        let weak: Weak<Shared> = Arc::downgrade(self);
        std::thread::spawn(move || {
            let mut wait = Duration::from_millis(500);
            loop {
                std::thread::sleep(wait);
                let Some(sh) = weak.upgrade() else { return };
                match Session::start(&sh.transport, false) {
                    Ok(s) => {
                        *lock(&sh.sess) = Some(s);
                        let _ = sh.refresh_hostname();
                        lock(&sh.status).state = Some(RemoteState::Connected);
                        sh.reconnecting.store(false, Ordering::SeqCst);
                        return;
                    }
                    Err(e) => {
                        lock(&sh.status).state = Some(RemoteState::Reconnecting(e.to_string()));
                        wait = (wait * 2).min(Duration::from_secs(10));
                    }
                }
            }
        });
    }

    fn refresh_hostname(self: &Arc<Self>) -> io::Result<()> {
        let mut req = Vec::new();
        req.extend_from_slice(b"f ");
        sh_quote(b"/proc/sys/kernel/hostname", &mut req);
        req.push(b'\n');
        let (recs, _) = self.request(&req, REQUEST_TIMEOUT)?;
        if let Some(r) = recs.first().filter(|r| r.rc == 0) {
            lock(&self.status).hostname = Some(String::from_utf8_lossy(&r.body).trim().to_string());
        }
        Ok(())
    }
}

/// Modo di lettura di un file: testo con i builtin della shell (niente fork, ma solo senza NUL),
/// batch binario (wc + cat) o `cat` per file.
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Text,
    Batch,
    Cat,
}

fn file_mode(path: &[u8]) -> Mode {
    let norm = if path.starts_with(b"/proc/[0-9]*/") {
        path.to_vec()
    } else {
        template(path).unwrap_or(path.to_vec())
    };
    let base = norm.rsplit(|&b| b == b'/').next().unwrap_or(&[]);
    if let Some(rest) = norm.strip_prefix(b"/proc/[0-9]*/") {
        let rest = rest.strip_prefix(b"task/[0-9]*/").unwrap_or(rest);
        if rest.starts_with(b"fdinfo/") {
            return Mode::Text;
        }
        return match rest {
            b"cmdline" | b"environ" => Mode::Batch,
            b"stat" | b"statm" | b"status" | b"comm" | b"io" | b"cgroup" | b"limits"
            | b"schedstat" | b"oom_score" | b"oom_score_adj" | b"loginuid" | b"sessionid"
            | b"wchan" => Mode::Text,
            _ => Mode::Cat,
        };
    }
    if norm.starts_with(b"/sys/") {
        return match base {
            b"config" | b"rom" | b"vpd" | b"resource" => Mode::Cat,
            _ => Mode::Text,
        };
    }
    match norm.as_slice() {
        b"/proc/stat"
        | b"/proc/meminfo"
        | b"/proc/loadavg"
        | b"/proc/uptime"
        | b"/proc/net/dev"
        | b"/proc/diskstats"
        | b"/proc/mounts"
        | b"/proc/self/mountinfo"
        | b"/proc/version"
        | b"/proc/cpuinfo"
        | b"/proc/net/route"
        | b"/proc/net/if_inet6"
        | b"/proc/net/fib_trie"
        | b"/proc/net/arp"
        | b"/etc/resolv.conf"
        | b"/etc/passwd"
        | b"/etc/hostname"
        | b"/etc/os-release"
        | b"/proc/sys/kernel/hostname" => Mode::Text,
        _ => Mode::Cat,
    }
}

/// File di testo leggibile con il grep in blocco (percorso esatto, senza metacaratteri né ':').
fn grep_exact(path: &[u8]) -> bool {
    file_mode(path) == Mode::Text && !path.iter().any(|b| b"*?[]\\:'".contains(b))
}

fn request_line(out: &mut Vec<u8>, kind: Kind, glob: bool, path: &[u8]) -> bool {
    if kind == Kind::File && glob && file_mode(path) == Mode::Text && !path.contains(&b':') {
        out.extend_from_slice(b"G ");
        let ok = sh_quote(path, out);
        out.push(b'\n');
        return ok;
    }
    if kind == Kind::Link && glob && path.ends_with(b"/fd/[0-9]*") {
        // link di tutti i descrittori: un solo `ls -l` per tutte le cartelle fd
        out.extend_from_slice(b"M ");
        let ok = sh_quote(&path[..path.len() - 7], out);
        out.push(b'\n');
        return ok;
    }
    let c = match (kind, glob) {
        (Kind::File, g) => match (file_mode(path), g) {
            (Mode::Text, false) => b't',
            (Mode::Text, true) => b'T',
            (Mode::Batch, true) => b'B',
            (Mode::Batch | Mode::Cat, false) => b'f',
            (Mode::Cat, true) => b'F',
        },
        (Kind::Link, true) => {
            // modello a due glob: cartella e voce
            let i = path.iter().rposition(|&b| b == b'/').unwrap_or(0);
            out.extend_from_slice(b"K ");
            let ok = sh_quote(&path[..i], out);
            out.push(b' ');
            let ok2 = sh_quote(&path[i + 1..], out);
            out.push(b'\n');
            return ok && ok2;
        }
        (k, g) => {
            let c = k.code();
            if g { c } else { c.to_ascii_lowercase() }
        }
    };
    out.push(c);
    out.push(b' ');
    let ok = sh_quote(path, out);
    out.push(b'\n');
    ok
}

/// Lo scarto di un solo "\n" finale del corpo dei link.
fn trim_nl(mut v: Vec<u8>) -> Vec<u8> {
    if v.last() == Some(&b'\n') {
        v.pop();
    }
    v
}

impl RemoteSource {
    /// Manifesto iniziale ragionevole (primo aggiornamento).
    fn initial_manifest() -> Vec<(Key, bool)> {
        let f = |p: &str| ((Kind::File, p.as_bytes().to_vec()), false);
        let mut m = vec![
            f("/proc/stat"),
            f("/proc/meminfo"),
            f("/proc/loadavg"),
            f("/proc/uptime"),
            f("/proc/net/dev"),
            f("/proc/diskstats"),
            f("/proc/mounts"),
            f("/proc/self/mountinfo"),
            f("/proc/sys/kernel/hostname"),
            f("/etc/passwd"),
        ];
        for n in ["stat", "status", "cmdline", "comm"] {
            m.push(((Kind::File, format!("/proc/[0-9]*/{n}").into_bytes()), true));
        }
        m.push(((Kind::Dir, b"/proc".to_vec()), false));
        // /sys: voci che i raccoglitori leggono al primo giro (rete, cpu, dischi, gpu, sensori)
        for d in [
            "/sys/class/net",
            "/sys/block",
            "/sys/class/drm",
            "/sys/class/hwmon",
            "/sys/class/thermal",
        ] {
            m.push(((Kind::Dir, d.as_bytes().to_vec()), false));
        }
        for g in [
            "/sys/class/net/*/operstate",
            "/sys/class/net/*/speed",
            "/sys/class/net/*/duplex",
            "/sys/class/net/*/address",
            "/sys/class/net/*/mtu",
            "/sys/devices/system/cpu/cpu[0-9]*/cpufreq/scaling_cur_freq",
            "/sys/block/*/size",
            "/sys/block/*/queue/rotational",
            "/sys/block/*/device/model",
            "/sys/class/drm/card[0-9]/device/vendor",
            "/sys/class/drm/card[0-9]/device/device",
            "/sys/class/drm/card[0-9]/device/power_state",
            "/sys/class/drm/card[0-9]/device/power/runtime_status",
            "/sys/class/drm/card[0-9]/device/uevent",
        ] {
            m.push(((Kind::File, g.as_bytes().to_vec()), true));
        }
        m.push(((Kind::File, b"/proc/cpuinfo".to_vec()), false));
        m.push(((Kind::Uid, b"/sys/block/*".to_vec()), true));
        for g in [
            "/sys/block/*",
            "/sys/class/net/*/device",
            "/sys/class/drm/card[0-9]/device",
            "/sys/class/drm/card[0-9]/device/driver",
        ] {
            m.push(((Kind::Link, g.as_bytes().to_vec()), true));
        }
        m.push(((Kind::Uid, b"/proc/[0-9]*".to_vec()), true));
        m
    }

    /// Manifesto dai percorsi letti: i modelli con almeno `PATTERN_MIN` pid, il resto esatto.
    fn manifest_from(
        touched: &HashSet<Key>,
        prev: &HashSet<Key>,
        npids: usize,
    ) -> Vec<(Key, bool)> {
        let mut groups: HashMap<Key, Vec<&Key>> = HashMap::new();
        let mut out = Vec::new();
        // percorsi esatti: solo se letti anche nel ciclo prima (o se non c'è un ciclo prima)
        let stable = |k: &Key| prev.is_empty() || prev.contains(k);
        for k in touched {
            match template(&k.1).filter(|_| k.0.generalizable()) {
                Some(t) => groups.entry((k.0, t)).or_default().push(k),
                None if stable(k) => out.push((k.clone(), false)),
                None => {}
            }
        }
        // modello solo per i file letti per (quasi) tutti i pid: in pratica `stat`
        let ref_n = groups
            .iter()
            .filter(|((k, _), _)| *k == Kind::File)
            .map(|(_, v)| v.len())
            .max()
            .unwrap_or(0)
            .max(npids);
        let min = PATTERN_MIN.max(ref_n * 4 / 5);
        for (t, ks) in groups {
            if ks.len() >= min {
                out.push((t, true));
            } else {
                out.extend(
                    ks.into_iter()
                        .filter(|k| stable(k))
                        .map(|k| (k.clone(), false)),
                );
            }
        }
        if out
            .iter()
            .any(|((k, p), g)| *g && *k == Kind::Link && p.ends_with(b"/fd/[0-9]*"))
        {
            // l'`ls -l` dei link porta già anche l'elenco delle cartelle fd
            out.retain(|((k, p), g)| {
                !(*g && *k == Kind::Dir && p.as_slice() == b"/proc/[0-9]*/fd")
            });
        }
        out.sort_by(|a, b| a.0.1.cmp(&b.0.1));
        out
    }

    /// Righe di richiesta per il manifesto. I percorsi esatti dello stesso tipo (file di testo, link, uid, statvfs)
    /// vanno tutti in un solo comando (un processo remoto per gruppo, non uno per percorso).
    /// Restituisce anche le chiavi raggruppate: quelle assenti dall'esito non esistono (o sono vuote).
    fn build_request(items: &[(Key, bool)]) -> (Vec<u8>, Vec<Key>) {
        let mut req = Vec::new();
        let mut groups: [(u8, Vec<Vec<u8>>); 4] = [
            (b'G', Vec::new()),
            (b'W', Vec::new()),
            (b'U', Vec::new()),
            (b'H', Vec::new()),
        ];
        let mut grouped: Vec<Key> = Vec::new();
        for ((kind, path), glob) in items {
            let plain = !path.iter().any(|b| b"*?[]\\' \n:".contains(b));
            let slot = match kind {
                Kind::File if !*glob && grep_exact(path) => Some(0),
                Kind::Link if !*glob && plain => Some(1),
                Kind::Uid if !*glob && plain => Some(2),
                Kind::Stat if plain => Some(3),
                _ => None,
            };
            if let Some(i) = slot {
                groups[i].1.push(path.clone());
                grouped.push((*kind, path.clone()));
                continue;
            }
            // modelli di file di testo: nello stesso `G` dei percorsi esatti (un solo grep per giro)
            if *kind == Kind::File
                && *glob
                && file_mode(path) == Mode::Text
                && !path.contains(&b':')
            {
                groups[0].1.push(path.clone());
                continue;
            }
            let mut line = Vec::new();
            if request_line(&mut line, *kind, *glob, path) {
                req.extend_from_slice(&line);
            }
        }
        for (cmd, paths) in &groups {
            if !paths.is_empty() {
                req.extend_from_slice(&Self::multi_line(*cmd, paths));
            }
        }
        (req, grouped)
    }

    fn multi_line(cmd: u8, paths: &[Vec<u8>]) -> Vec<u8> {
        let mut l = vec![cmd];
        for p in paths {
            l.push(b' ');
            sh_quote(p, &mut l);
        }
        l.push(b'\n');
        l
    }

    /// Righe "percorso:riga" del grep: si ricompongono i file (righe unite da "\n", newline finale).
    fn absorb_grep(cache: &mut Cache, body: &[u8]) {
        let mut cur: Option<(Vec<u8>, Vec<u8>)> = None;
        let flush = |cur: &mut Option<(Vec<u8>, Vec<u8>)>, cache: &mut Cache| {
            if let Some((p, c)) = cur.take() {
                cache.entries.insert((Kind::File, p), (0, c));
            }
        };
        let body = body.strip_suffix(b"\n").unwrap_or(body);
        if body.is_empty() {
            return;
        }
        for line in body.split(|&b| b == b'\n') {
            let Some(i) = line.iter().position(|&b| b == b':') else {
                continue;
            };
            let (path, text) = (&line[..i], &line[i + 1..]);
            match &mut cur {
                Some((p, c)) if p.as_slice() == path => {
                    c.extend_from_slice(text);
                    c.push(b'\n');
                }
                _ => {
                    flush(&mut cur, cache);
                    let mut c = text.to_vec();
                    c.push(b'\n');
                    cur = Some((path.to_vec(), c));
                }
            }
        }
        flush(&mut cur, cache);
    }

    /// Esito di `ls -l` su molte cartelle fd: link e, di riflesso, l'elenco delle cartelle.
    fn absorb_ls(cache: &mut Cache, body: &[u8]) {
        let mut dir: Option<Vec<u8>> = None;
        let mut names: Vec<u8> = Vec::new();
        let close = |dir: &mut Option<Vec<u8>>, names: &mut Vec<u8>, cache: &mut Cache| {
            if let Some(d) = dir.take() {
                cache
                    .entries
                    .insert((Kind::Dir, d), (0, std::mem::take(names)));
            }
        };
        for line in body.split(|&b| b == b'\n') {
            if line.starts_with(b"/")
                && line.ends_with(b":")
                && !line.windows(4).any(|w| w == b" -> ")
            {
                close(&mut dir, &mut names, cache);
                dir = Some(line[..line.len() - 1].to_vec());
            } else if let Some(i) = line.windows(4).position(|w| w == b" -> ")
                && let Some(d) = &dir
            {
                let name = line[..i].rsplit(|&b| b == b' ').next().unwrap_or(&[]);
                if name.is_empty() {
                    continue;
                }
                let mut path = d.clone();
                path.push(b'/');
                path.extend_from_slice(name);
                cache
                    .entries
                    .insert((Kind::Link, path), (0, line[i + 4..].to_vec()));
                names.extend_from_slice(name);
                names.push(0);
            }
        }
        close(&mut dir, &mut names, cache);
    }

    /// Inserisce i record di una risposta nella cache. Restituisce ciò che va rifatto per file:
    /// (modelli di file da rileggere con `cat`, link da rileggere uno a uno).
    fn absorb(cache: &mut Cache, recs: Vec<Rec>) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
        let (mut redo_files, mut redo_links) = (Vec::new(), Vec::new());
        let mut it = recs.into_iter().peekable();
        while let Some(r) = it.next() {
            match r.kind {
                b'V' => {
                    // "uid percorso" per riga: risultato di un modello di uid
                    for line in r.body.split(|&b| b == b'\n') {
                        if let Some(sp) = line.iter().position(|&b| b == b' ') {
                            cache.entries.insert(
                                (Kind::Uid, line[sp + 1..].to_vec()),
                                (0, line[..sp].to_vec()),
                            );
                        }
                    }
                }
                b'G' => Self::absorb_grep(cache, &r.body),
                b'W' => {
                    // "ls -ld": la riga "... percorso -> destinazione" (percorsi senza spazi)
                    for line in r.body.split(|&b| b == b'\n') {
                        if let Some(i) = line.windows(4).position(|w| w == b" -> ") {
                            let path = line[..i].rsplit(|&b| b == b' ').next().unwrap_or(&[]);
                            if !path.is_empty() {
                                cache.entries.insert(
                                    (Kind::Link, path.to_vec()),
                                    (0, line[i + 4..].to_vec()),
                                );
                            }
                        }
                    }
                }
                b'H' => {
                    // "frsize blocchi disp inode liberi bliberi percorso"
                    for line in r.body.split(|&b| b == b'\n') {
                        let mut it = line.splitn(7, |&b| b == b' ');
                        let nums: Vec<&[u8]> = it.by_ref().take(6).collect();
                        if let (6, Some(path)) = (nums.len(), it.next()) {
                            cache
                                .entries
                                .insert((Kind::Stat, path.to_vec()), (0, nums.join(&b' ')));
                        }
                    }
                }
                b'M' => Self::absorb_ls(cache, &r.body),
                b'B' => {
                    // lunghezze (wc) + byte concatenati (cat): si accetta solo se le somme tornano
                    let Some(c) = it.next_if(|n| n.kind == b'C') else {
                        continue;
                    };
                    let mut list = Vec::new();
                    for line in String::from_utf8_lossy(&r.body).lines() {
                        let line = line.trim_start();
                        if let Some((n, p)) = line.split_once(' ')
                            && let Ok(n) = n.parse::<usize>()
                            && p.trim_start().starts_with('/')
                        {
                            list.push((n, p.trim_start().as_bytes().to_vec()));
                        }
                    }
                    if list.iter().map(|x| x.0).sum::<usize>() != c.body.len() {
                        redo_files.push(r.arg);
                        continue;
                    }
                    let mut off = 0;
                    for (n, p) in list {
                        cache
                            .entries
                            .insert((Kind::File, p), (0, c.body[off..off + n].to_vec()));
                        off += n;
                    }
                }
                b'N' => {
                    let Some(k) = it.next_if(|n| n.kind == b'K') else {
                        continue;
                    };
                    let names: Vec<&[u8]> = r
                        .body
                        .split(|&b| b == 0)
                        .filter(|n| !n.is_empty())
                        .collect();
                    let mut body = k.body.as_slice();
                    if body.last() == Some(&b'\n') {
                        body = &body[..body.len() - 1];
                    }
                    let targets: Vec<&[u8]> = if names.is_empty() {
                        Vec::new()
                    } else {
                        body.split(|&b| b == b'\n').collect()
                    };
                    if targets.len() != names.len() {
                        redo_links.extend(names.iter().map(|n| n.to_vec()));
                        continue;
                    }
                    for (n, t) in names.iter().zip(targets) {
                        cache
                            .entries
                            .insert((Kind::Link, n.to_vec()), (0, t.to_vec()));
                    }
                }
                _ => {
                    if let Some(kind) = Kind::from_code(r.kind) {
                        cache.entries.insert((kind, r.arg), (r.rc, r.body));
                    }
                }
            }
        }
        (redo_files, redo_links)
    }

    fn do_refresh(&self) -> io::Result<()> {
        let (items, old, refetch_sticky) = {
            let mut c = lock(&self.cache);
            let t = std::mem::take(&mut c.touched);
            c.misses.clear();
            c.link_dirs.clear();
            let empty = HashSet::new();
            let mut items = if t.is_empty() {
                Self::initial_manifest()
            } else {
                Self::manifest_from(
                    &t,
                    // nei primi giri niente selezione: tutto ciò che è stato letto resta
                    if c.laps < 4 { &empty } else { &c.prev_touched },
                    c.pids.len(),
                )
            };
            if c.laps < 2 && !t.is_empty() {
                let have: HashSet<Key> = items.iter().map(|(k, _)| k.clone()).collect();
                items.extend(
                    Self::initial_manifest()
                        .into_iter()
                        .filter(|(k, _)| !have.contains(k)),
                );
            }
            let laps = c.laps;
            for k in &t {
                if template(&k.1).is_none() && (k.0 != Kind::File || file_mode(&k.1) == Mode::Text)
                {
                    c.last_seen.insert(k.clone(), laps);
                }
            }
            c.last_seen.retain(|_, l| laps - *l < RECENT_LAPS);
            if !t.is_empty() {
                let have: HashSet<Key> = items.iter().map(|(k, _)| k.clone()).collect();
                let extra: Vec<Key> = c
                    .last_seen
                    .keys()
                    .filter(|k| !have.contains(*k))
                    .cloned()
                    .collect();
                items.extend(extra.into_iter().map(|k| (k, false)));
            }
            let refetch_sticky = c.laps < 2 || c.laps.is_multiple_of(STICKY_LAPS);
            if !refetch_sticky {
                items.retain(|(k, _)| !sticky(k));
            }
            (items, t, refetch_sticky)
        };
        let (req, grouped) = Self::build_request(&items);
        let (recs, bytes) = match self.sh.request(&req, REQUEST_TIMEOUT) {
            Ok(r) => r,
            Err(e) => {
                // la prossima volta si riparte dallo stesso insieme di letture
                let mut c = lock(&self.cache);
                c.entries.clear();
                c.covered.clear();
                c.touched.extend(old);
                return Err(e);
            }
        };
        let mut c = lock(&self.cache);
        if refetch_sticky {
            c.entries.clear();
        } else {
            c.entries.retain(|k, _| sticky(k));
        }
        c.covered = items
            .iter()
            .filter(|(_, g)| *g)
            .map(|(k, _)| k.clone())
            .collect();
        let (mut redo_files, redo_links) = Self::absorb(&mut c, recs);
        for k in grouped {
            c.entries.entry(k).or_insert((2, Vec::new()));
        }
        if items
            .iter()
            .any(|((k, p), g)| *g && *k == Kind::Link && p.ends_with(b"/fd/[0-9]*"))
        {
            c.covered.insert((Kind::Dir, b"/proc/[0-9]*/fd".to_vec()));
        }
        c.prev_touched = old;
        c.laps += 1;
        // pid presenti e pid comparsi dal giro prima
        let now: HashSet<Vec<u8>> = c
            .entries
            .get(&(Kind::Dir, b"/proc".to_vec()))
            .map(|(_, b)| {
                b.split(|&x| x == 0)
                    .filter(|n| !n.is_empty() && n.iter().all(u8::is_ascii_digit))
                    .map(<[u8]>::to_vec)
                    .collect()
            })
            .unwrap_or_default();
        // al primo giro (e dopo una riconnessione) sono "nuovi" tutti i pid
        c.new_pids = now.difference(&c.pids).cloned().collect();
        c.pids = now;
        drop(c);
        let mut bytes = bytes;
        // rare: lunghezze cambiate tra wc e cat (un cmdline modificato nel frattempo): si ripete il blocco
        // (mai un cat per file: con migliaia di processi costerebbe secondi); poi si rinuncia
        for _ in 0..2 {
            if redo_files.is_empty() {
                break;
            }
            let mut req = Vec::new();
            for pat in &redo_files {
                req.extend_from_slice(b"B ");
                sh_quote(pat, &mut req);
                req.push(b'\n');
            }
            let (recs, b2) = self.sh.request(&req, REQUEST_TIMEOUT)?;
            bytes += b2;
            redo_files = Self::absorb(&mut lock(&self.cache), recs).0;
        }
        if !redo_links.is_empty() {
            let mut req = Vec::new();
            for p in redo_links.iter().take(200) {
                request_line(&mut req, Kind::Link, false, p);
            }
            let (recs, b2) = self.sh.request(&req, REQUEST_TIMEOUT)?;
            bytes += b2;
            Self::absorb(&mut lock(&self.cache), recs);
        }
        lock(&self.sh.status).last_bytes = bytes;
        Ok(())
    }

    /// Lettura generica: cache, copertura dei modelli, altrimenti richiesta immediata.
    fn fetch(&self, kind: Kind, path: &Path) -> io::Result<Vec<u8>> {
        let p = path.as_os_str().as_bytes();
        if !p.starts_with(b"/") || p.contains(&0) || p.contains(&b'\n') {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid path"));
        }
        if !self.sh.connected() {
            return Err(self.sh.not_connected());
        }
        let key: Key = (kind, p.to_vec());
        {
            let mut c = lock(&self.cache);
            c.touched.insert(key.clone());
            if let Some((rc, body)) = c.entries.get(&key) {
                return if *rc == 0 {
                    Ok(body.clone())
                } else {
                    Err(rc_error(*rc))
                };
            }
            if kind == Kind::Link
                && let Some(i) = p.iter().rposition(|&b| b == b'/')
                && c.link_dirs.contains(&p[..i])
            {
                return Err(rc_error(2));
            }
            if kind.generalizable()
                && let Some(t) = template(p)
                && c.covered.contains(&(kind, t))
            {
                return Err(rc_error(2));
            }
        }
        // Lettura fuori cache di /proc/<pid>/X: se il pid è nuovo, o se è la seconda lettura fuori cache dello
        // stesso modello in questo giro, nella stessa richiesta si carica X per tutti i pid nuovi / presenti:
        // un gruppo di letture simili costa una sola andata e ritorno.
        let mut group: Vec<Vec<u8>> = vec![p.to_vec()];
        if kind.generalizable()
            && kind != Kind::Link
            && let Some(rest) = p.strip_prefix(b"/proc/")
            && let Some(t) = template(p)
        {
            let sl = rest.iter().position(|&b| b == b'/').unwrap_or(rest.len());
            // niente lettura di massa per descrittori (fd/N, fdinfo/N) né per file letti con un cat ciascuno
            let bulk_ok = !(rest[sl..].starts_with(b"/fd/") || rest[sl..].starts_with(b"/fdinfo/"))
                && (kind != Kind::File || file_mode(p) != Mode::Cat);
            let mut c = lock(&self.cache);
            let n = c.misses.entry((kind, t)).or_insert(0);
            *n += 1;
            let second = *n >= 2;
            let is_new = c.new_pids.iter().any(|n| n.as_slice() == &rest[..sl]);
            if bulk_ok && (is_new || second) {
                let pids: Vec<Vec<u8>> = if is_new {
                    c.new_pids.clone()
                } else {
                    c.pids.iter().cloned().collect()
                };
                for n in &pids {
                    let mut q = b"/proc/".to_vec();
                    q.extend_from_slice(n);
                    q.extend_from_slice(&rest[sl..]);
                    if q != p && !c.entries.contains_key(&(kind, q.clone())) {
                        group.push(q);
                    }
                }
            }
        }
        let is_fd_dir = kind == Kind::Dir && p.starts_with(b"/proc/") && p.ends_with(b"/fd");
        let grep_group = kind == Kind::File && group.len() > 1 && grep_exact(p);
        let mut req = Vec::new();
        if is_fd_dir {
            // elenco + link di tutti i descrittori con un solo `ls -l`
            req.extend_from_slice(b"M");
            for d in &group {
                req.push(b' ');
                sh_quote(d, &mut req);
            }
            req.push(b'\n');
        } else if grep_group {
            req = Self::multi_line(b'G', &group);
        } else {
            for q in &group {
                request_line(&mut req, kind, false, q);
            }
        }
        let (recs, _) = self.sh.request(&req, REQUEST_TIMEOUT)?;
        let mut c = lock(&self.cache);
        Self::absorb(&mut c, recs);
        if is_fd_dir {
            for d in &group {
                c.entries
                    .entry((Kind::Dir, d.clone()))
                    .or_insert((13, Vec::new()));
                c.link_dirs.insert(d.clone());
            }
        } else if grep_group {
            for q in &group {
                c.entries
                    .entry((Kind::File, q.clone()))
                    .or_insert((2, Vec::new()));
            }
        }
        match c.entries.get(&key) {
            Some((0, body)) => Ok(body.clone()),
            Some((rc, _)) => Err(rc_error(*rc)),
            None => Err(io::Error::new(io::ErrorKind::InvalidData, "missing record")),
        }
    }
}

impl Source for RemoteSource {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.fetch(Kind::File, path)
    }
    fn read_dir(&self, path: &Path) -> io::Result<Vec<OsString>> {
        let b = self.fetch(Kind::Dir, path)?;
        Ok(b.split(|&c| c == 0)
            .filter(|n| !n.is_empty())
            .map(|n| OsString::from_vec(n.to_vec()))
            .collect())
    }
    fn read_link(&self, path: &Path) -> io::Result<PathBuf> {
        Ok(PathBuf::from(OsString::from_vec(trim_nl(
            self.fetch(Kind::Link, path)?,
        ))))
    }
    fn owner_uid(&self, path: &Path) -> io::Result<u32> {
        let b = self.fetch(Kind::Uid, path)?;
        String::from_utf8_lossy(&b)
            .trim()
            .parse()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "bad uid"))
    }
    fn statvfs(&self, mount: &Path) -> io::Result<FsStat> {
        let b = self.fetch(Kind::Stat, mount)?;
        let t = String::from_utf8_lossy(&b);
        let n: Vec<u64> = t
            .split_whitespace()
            .filter_map(|x| x.parse().ok())
            .collect();
        if n.len() < 6 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "bad statvfs"));
        }
        Ok(FsStat {
            total: n[1].saturating_mul(n[0]),
            free: n[2].saturating_mul(n[0]),
            used: n[1].saturating_sub(n[5]).saturating_mul(n[0]),
            files: n[3],
            ffree: n[4],
        })
    }
    fn begin_refresh(&self) {
        if !self.sh.connected() {
            let mut c = lock(&self.cache);
            c.entries.clear();
            c.covered.clear();
            c.pids.clear();
            c.new_pids.clear();
            return;
        }
        let _ = self.do_refresh();
    }
    fn remote_status(&self) -> Option<RemoteStatus> {
        let s = lock(&self.sh.status);
        Some(RemoteStatus {
            target: self.sh.target.clone(),
            hostname: s.hostname.clone(),
            latency_ms: s.latency_ms,
            last_bytes: s.last_bytes,
            state: s.state.clone().unwrap_or(RemoteState::Connected),
        })
    }
}

/// Parole di sicurezza: gli argomenti sono quotati da `sh_quote`, mai concatenati.
impl CommandRunner for RemoteRunner {
    fn run(&self, argv: &[&str], timeout: Duration) -> io::Result<CommandOutput> {
        if argv.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "empty command"));
        }
        let secs = timeout.as_secs_f64().ceil().max(1.0) as u64;
        let mut req = format!("x {secs}").into_bytes();
        for a in argv {
            req.push(b' ');
            if !sh_quote(a.as_bytes(), &mut req) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "NUL in argument",
                ));
            }
        }
        req.push(b'\n');
        let t0 = std::time::Instant::now();
        let (recs, _) = self.sh.request(&req, timeout + Duration::from_secs(5))?;
        let mut it = recs.into_iter();
        let (Some(x), Some(y)) = (it.next(), it.next()) else {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "missing record"));
        };
        // 124 = timeout(1) GNU, 137/143 = KILL/TERM (busybox): vale solo se il tempo è davvero scaduto
        if matches!(x.rc, 124 | 137 | 143) && t0.elapsed() + Duration::from_millis(200) >= timeout {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "command timed out"));
        }
        Ok(CommandOutput {
            status: Some(x.rc),
            stdout: x.body,
            stderr: y.body,
        })
    }
}

impl RemoteSource {
    /// Solo per i test: termina il processo della sessione.
    #[doc(hidden)]
    pub fn kill_session_for_test(&self) {
        if let Some(s) = lock(&self.sh.sess).as_mut() {
            s.kill();
        }
    }
}

impl RemoteSource {
    /// Solo per le misure: pid del processo della sessione.
    #[doc(hidden)]
    pub fn session_pid_for_test(&self) -> Option<u32> {
        lock(&self.sh.sess).as_ref().map(|s| s.pid())
    }
}
