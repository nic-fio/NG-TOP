// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Sezione 3 "Processi": albero a sinistra, dettaglio a schede a destra, segnali e nice.
//! Contiene anche il dialogo "Invia segnale" (comune a Panoramica e Processi).

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use rustix::io::Errno;
use rustix::process::{
    Pid, PidfdFlags, Signal, kill_process, pidfd_open, pidfd_send_signal, setpriority_process,
};

use super::panoramica::{dim, fg, pct_color, span};
use crate::collect::System;
use crate::collect::procdetail::{DetailCollector, DetailTab, ProcDetail, policy_name};
use crate::collect::process::{Process, SortKey};
use crate::ui::app::{App, HISTORY_LEN};
use crate::ui::fmt;
use crate::ui::history::History;
use crate::ui::theme;
use crate::ui::tz::Tz;
use crate::ui::widgets::proctable::follow;
use crate::ui::widgets::{BrailleGraph, ColumnDef, Dialog, Panel, ProcTable, put, putn, width};

pub const KEYS_PROC: &[(&str, &str)] = &[
    ("F3", "Search"),
    ("F4", "Filter"),
    ("F5", "List/Tree"),
    ("Space", "Fold"),
    ("←→", "Tabs"),
    ("F7/F8", "Nice"),
    ("F9", "Signal"),
    ("1-5", "Sections"),
    ("q", "Quit"),
];

/// Quanto resta visibile la riga di stato.
const STATUS_SECS: u64 = 5;

// ---------------------------------------------------------------- segnali

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sig {
    pub num: i32,
    pub name: &'static str,
    pub desc: &'static str,
}

pub const SIGNALS: [Sig; 8] = [
    Sig {
        num: 15,
        name: "SIGTERM",
        desc: "ask to quit (recommended)",
    },
    Sig {
        num: 9,
        name: "SIGKILL",
        desc: "stop now, unsaved work lost",
    },
    Sig {
        num: 19,
        name: "SIGSTOP",
        desc: "pause",
    },
    Sig {
        num: 18,
        name: "SIGCONT",
        desc: "resume",
    },
    Sig {
        num: 1,
        name: "SIGHUP",
        desc: "reload configuration",
    },
    Sig {
        num: 2,
        name: "SIGINT",
        desc: "interrupt (like Ctrl-C)",
    },
    Sig {
        num: 10,
        name: "SIGUSR1",
        desc: "user signal 1",
    },
    Sig {
        num: 12,
        name: "SIGUSR2",
        desc: "user signal 2",
    },
];

/// Funzione che invia un segnale (sostituibile nei test).
/// Argomenti: radice di /proc, pid, avvio atteso (tick), segnale.
pub type KillFn = fn(&std::path::Path, u32, u64, Sig) -> Result<(), String>;

fn errno_text(e: Errno) -> String {
    match e {
        Errno::PERM => "permission denied (EPERM)".to_string(),
        Errno::ACCESS => "access denied (EACCES)".to_string(),
        Errno::SRCH => "the process no longer exists (ESRCH)".to_string(),
        other => format!("error {}", other.raw_os_error()),
    }
}

fn to_pid(pid: u32) -> Option<Pid> {
    i32::try_from(pid).ok().and_then(Pid::from_raw)
}

/// Invio reale con rustix (nessuna libc C). Ordine senza finestre: pidfd_open → si rilegge
/// l'avvio da stat e lo si confronta con quello atteso → pidfd_send_signal (il pidfd resta
/// legato a quel processo anche se il pid viene riusato). Senza pidfd (kernel < 5.3 o permessi):
/// verifica + kill.
pub fn send_signal(
    proc_root: &std::path::Path,
    pid: u32,
    expect: u64,
    sig: Sig,
) -> Result<(), String> {
    let p = to_pid(pid).ok_or("invalid pid")?;
    let s = Signal::from_named_raw(sig.num).ok_or("invalid signal")?;
    let changed = || Err("the pid now belongs to another process (reused)".to_string());
    match pidfd_open(p, PidfdFlags::empty()) {
        Ok(fd) => {
            if live_start(proc_root, pid) != Some(expect) {
                return changed();
            }
            match pidfd_send_signal(&fd, s) {
                Ok(()) => Ok(()),
                Err(Errno::SRCH) => Err(errno_text(Errno::SRCH)),
                Err(_) => kill_process(p, s).map_err(errno_text),
            }
        }
        Err(Errno::SRCH) => Err(errno_text(Errno::SRCH)),
        Err(_) => {
            if live_start(proc_root, pid) != Some(expect) {
                return changed();
            }
            kill_process(p, s).map_err(errno_text)
        }
    }
}

/// Avvio (starttime di stat, in tick) del processo vivo: serve a riconoscere un pid riusato.
pub type StartFn = fn(&std::path::Path, u32) -> Option<u64>;

pub fn live_start(proc_root: &std::path::Path, pid: u32) -> Option<u64> {
    std::fs::read_to_string(proc_root.join(pid.to_string()).join("stat"))
        .ok()
        .and_then(|t| crate::collect::process::parse_stat(&t))
        .map(|st| st.starttime)
}

/// Oltre questa soglia il sottoalbero richiede una seconda conferma.
const BIG_SUBTREE: usize = 50;

/// Imposta il nice di un processo.
pub fn set_nice(pid: u32, nice: i32) -> Result<(), String> {
    let p = to_pid(pid).ok_or("invalid pid")?;
    setpriority_process(Some(p), nice).map_err(errno_text)
}

/// Motivo per cui un pid non va colpito alla leggera (init, ngtop stesso).
pub fn protected(pid: u32) -> Option<&'static str> {
    if pid == 1 {
        Some("PID 1 (init)")
    } else if pid == std::process::id() {
        Some("ngtop itself")
    } else {
        None
    }
}

/// Il pid e tutti i suoi discendenti (preordine, radice per prima).
pub fn subtree_pids(rows: &[Process], root: u32) -> Vec<u32> {
    let mut kids: HashMap<u32, Vec<u32>> = HashMap::new();
    for p in rows {
        if p.pid != p.ppid {
            kids.entry(p.ppid).or_default().push(p.pid);
        }
    }
    for v in kids.values_mut() {
        v.sort_unstable();
    }
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut stack = vec![root];
    while let Some(pid) = stack.pop() {
        if !seen.insert(pid) {
            continue;
        }
        out.push(pid);
        if let Some(k) = kids.get(&pid) {
            stack.extend(k.iter().rev());
        }
    }
    out
}

/// Sottoalbero di `root` (preordine) attraverso soli nodi ancora validi, con l'avvio atteso.
/// Valido: avvio attuale = memorizzato; oppure non memorizzato, genitore valido e nato dopo
/// di lui. Un nodo con avvio diverso (pid riusato) è escluso con tutti i suoi discendenti.
pub fn verified_subtree(
    rows: &[Process],
    root: u32,
    stored: &HashMap<u32, u64>,
) -> Vec<(u32, u64)> {
    let by_pid: HashMap<u32, &Process> = rows.iter().map(|p| (p.pid, p)).collect();
    let mut kids: HashMap<u32, Vec<u32>> = HashMap::new();
    for p in rows.iter().filter(|p| p.pid != p.ppid) {
        kids.entry(p.ppid).or_default().push(p.pid);
    }
    for v in kids.values_mut() {
        v.sort_unstable();
    }
    let mut out = Vec::new();
    let Some(r) = by_pid.get(&root) else {
        return out;
    };
    if stored.get(&root) != Some(&r.start_ticks) {
        return out;
    }
    let mut seen = HashSet::new();
    // (pid, avvio del genitore)
    let mut stack = vec![(root, r.start_ticks)];
    while let Some((pid, _)) = stack.pop() {
        if !seen.insert(pid) {
            continue;
        }
        let Some(me) = by_pid.get(&pid) else { continue };
        out.push((pid, me.start_ticks));
        for &k in kids.get(&pid).into_iter().flatten().rev() {
            let Some(kp) = by_pid.get(&k) else { continue };
            let valid = match stored.get(&k) {
                Some(&st) => st == kp.start_ticks,
                None => kp.start_ticks >= me.start_ticks,
            };
            if valid {
                stack.push((k, me.start_ticks));
            }
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalDialog {
    pub pid: u32,
    pub name: String,
    pub cmdline: String,
    /// Indice in `SIGNALS`.
    pub sig: usize,
    pub subtree: bool,
    /// Radice e discendenti (al momento dell'apertura).
    pub tree_pids: Vec<u32>,
    /// Avvio (tick) di radice e discendenti all'apertura: all'invio si riverifica.
    pub tree_starts: Vec<(u32, u64)>,
    /// Quanti processi del sottoalbero appartengono ad altri utenti.
    pub foreign: usize,
    /// Seconda conferma già data (sottoalbero grande o di altri utenti).
    pub armed: bool,
}

impl SignalDialog {
    pub fn new(p: &Process, rows: &[Process]) -> Self {
        let tree_pids = subtree_pids(rows, p.pid);
        let in_tree: HashSet<u32> = tree_pids.iter().copied().collect();
        let me = rustix::process::geteuid().as_raw();
        let mut tree_starts = vec![(p.pid, p.start_ticks)];
        let mut foreign = 0;
        for q in rows.iter().filter(|q| q.pid != p.pid) {
            if in_tree.contains(&q.pid) {
                tree_starts.push((q.pid, q.start_ticks));
                foreign += usize::from(q.uid != me);
            }
        }
        foreign += usize::from(p.uid != me);
        SignalDialog {
            pid: p.pid,
            name: p.name.clone(),
            cmdline: p.cmdline.clone(),
            sig: 0,
            subtree: false,
            tree_pids,
            tree_starts,
            foreign,
            armed: false,
        }
    }

    /// Il sottoalbero di init (1) o kthreadd (2) è praticamente tutto il sistema: vietato.
    pub fn subtree_allowed(&self) -> bool {
        self.pid > 2
    }

    /// Serve una seconda pressione di Invio.
    pub fn needs_confirm(&self) -> bool {
        self.subtree && (self.tree_pids.len() > BIG_SUBTREE || self.foreign > 0)
    }

    pub fn signal(&self) -> Sig {
        SIGNALS[self.sig.min(SIGNALS.len() - 1)]
    }

    /// Pid a cui il segnale verrà davvero inviato: mai il pid 1; ngtop stesso solo se scelto
    /// direttamente (e con avviso), non come parte di un sottoalbero.
    pub fn targets(&self) -> Vec<u32> {
        if self.subtree && !self.subtree_allowed() {
            Vec::new()
        } else if self.subtree {
            self.tree_pids
                .iter()
                .copied()
                .filter(|&p| protected(p).is_none())
                .collect()
        } else if self.pid == 1 {
            Vec::new()
        } else {
            vec![self.pid]
        }
    }

    pub fn warnings(&self) -> Vec<String> {
        let scope: Vec<u32> = if self.subtree {
            self.tree_pids.clone()
        } else {
            vec![self.pid]
        };
        let mut w = Vec::new();
        if !self.subtree_allowed() {
            w.push(format!(
                "WARNING: the subtree of PID {} is not allowed",
                self.pid
            ));
        }
        for p in scope {
            match protected(p) {
                Some(who) if p == 1 => w.push(format!("WARNING: {who} will not be touched")),
                Some(who) if self.subtree => {
                    w.push(format!("WARNING: {who} (PID {p}) will be skipped"));
                }
                Some(who) => w.push(format!("WARNING: you are signalling {who}")),
                None => {}
            }
        }
        w
    }

    pub fn lines(&self) -> Vec<String> {
        let kids = self.tree_pids.len().saturating_sub(1);
        let mut l = vec![
            if kids > 0 {
                format!(
                    "Process: {} {}   and {} children",
                    self.pid, self.name, kids
                )
            } else {
                format!("Process: {} {}", self.pid, self.name)
            },
            crate::ui::widgets::clip(&self.cmdline, 56),
        ];
        for (i, s) in SIGNALS.iter().enumerate() {
            let mark = if i == self.sig { "▸(•)" } else { "  ( )" };
            l.push(format!("{mark} {:>2} {:<8} {}", s.num, s.name, s.desc));
        }
        l.push(format!(
            "[{}] whole subtree ({} {})  Space",
            if self.subtree { "x" } else { " " },
            self.tree_pids.len(),
            if self.tree_pids.len() == 1 {
                "process"
            } else {
                "processes"
            }
        ));
        l.extend(self.warnings());
        if self.needs_confirm() {
            let why = if self.foreign > 0 {
                format!("{} from other users", self.foreign)
            } else {
                format!("more than {BIG_SUBTREE} processes")
            };
            l.push(if self.armed {
                "CONFIRM: press Enter again to send".to_string()
            } else {
                format!("Warning: large subtree ({why}): press Enter twice")
            });
        }
        l
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        let lines = self.lines();
        let send = format!("Enter: send {}", self.signal().name);
        Dialog {
            title: "Send signal",
            lines: &lines,
            buttons: &["Esc: cancel", &send],
            selected: 1,
        }
        .render(area, buf);
    }
}

/// Apre il dialogo sul processo selezionato (F9 / k); senza processi non fa nulla.
pub fn open_dialog(app: &mut App) {
    if let Some(p) = app.selected_process() {
        app.dialog = Some(SignalDialog::new(p, &app.rows));
    }
}

/// Apre il dialogo su un pid noto (es. la connessione scelta in Rete e dischi).
pub fn open_dialog_pid(app: &mut App, pid: u32) {
    if let Some(p) = app.rows.iter().find(|p| p.pid == pid) {
        app.dialog = Some(SignalDialog::new(p, &app.rows));
    }
}

/// Tasti con il dialogo aperto; `true` se lo stato è cambiato.
pub fn dialog_key(app: &mut App, key: KeyEvent) -> bool {
    let Some(d) = app.dialog.as_mut() else {
        return false;
    };
    match key.code {
        KeyCode::Esc => app.dialog = None,
        KeyCode::Up => {
            d.sig = d.sig.saturating_sub(1);
            d.armed = false;
        }
        KeyCode::Down => {
            d.sig = (d.sig + 1).min(SIGNALS.len() - 1);
            d.armed = false;
        }
        KeyCode::Char(' ') => {
            if d.subtree_allowed() {
                d.subtree = !d.subtree;
            }
            d.armed = false;
        }
        KeyCode::Enter => {
            if d.needs_confirm() && !d.armed {
                d.armed = true;
            } else if let Some(d) = app.dialog.take() {
                send_dialog(app, &d);
            }
        }
        _ => return false,
    }
    true
}

fn name_of(rows: &[Process], pid: u32) -> String {
    rows.iter()
        .find(|p| p.pid == pid)
        .map_or_else(String::new, |p| p.name.clone())
}

/// Invia il segnale scelto ai bersagli, dopo aver riverificato che siano ancora gli stessi
/// processi (pid + avvio); riassume l'esito nella riga di stato.
fn send_dialog(app: &mut App, d: &SignalDialog) {
    // Difesa in profondità: in remoto un pid è di un'altra macchina, mai agire su quello locale.
    if app.remote {
        app.proc.set_status(crate::ui::app::REMOTE_NA.into(), false);
        return;
    }
    let sig = d.signal();
    let opened = d.targets();
    if opened.is_empty() {
        app.proc
            .set_status(format!("{}: no process to signal", sig.name), false);
        return;
    }
    let stored: HashMap<u32, u64> = d.tree_starts.iter().copied().collect();
    let (root, start_fn) = (app.proc.proc_root.clone(), app.proc.start_fn);
    // La radice si verifica per prima: se il pid è stato riusato non si invia nulla.
    if stored
        .get(&d.pid)
        .is_none_or(|&e| start_fn(&root, d.pid) != Some(e))
    {
        app.proc.set_status(
            format!("process {} has exited, no signal sent", d.pid),
            false,
        );
        return;
    }
    let cand: Vec<(u32, u64)> = if d.subtree {
        verified_subtree(&app.rows, d.pid, &stored)
            .into_iter()
            .filter(|&(p, _)| protected(p).is_none())
            .collect()
    } else {
        vec![(d.pid, stored[&d.pid])]
    };
    let cand: Vec<(u32, u64)> = cand
        .into_iter()
        .filter(|&(pid, e)| start_fn(&root, pid) == Some(e))
        .collect();
    let targets: Vec<u32> = cand.iter().map(|&(p, _)| p).collect();
    let skipped = opened.iter().filter(|p| !targets.contains(p)).count();
    let skip_note = if skipped > 0 {
        format!(" ({skipped} no longer exist)")
    } else {
        String::new()
    };
    if targets.is_empty() {
        app.proc.set_status(
            format!("{}: no process to signal{skip_note}", sig.name),
            false,
        );
        return;
    }
    let kill = app.proc.kill;
    let (mut ok, mut fail) = (0usize, 0usize);
    let mut first_err: Option<(u32, String)> = None;
    for &(pid, expect) in &cand {
        match kill(&root, pid, expect, sig) {
            Ok(()) => ok += 1,
            Err(e) => {
                fail += 1;
                first_err.get_or_insert((pid, e));
            }
        }
    }
    let label = |pid: u32| format!("{} {}", pid, name_of(&app.rows, pid).trim().to_owned());
    let (text, good) = match (targets.len(), first_err) {
        (1, None) => (format!("{} sent to {}", sig.name, label(targets[0])), true),
        (1, Some((pid, e))) => (format!("{} to {}: {e}", sig.name, label(pid)), false),
        (n, None) => (
            format!(
                "{} sent to {n} processes (subtree of {})",
                sig.name,
                label(d.pid)
            ),
            true,
        ),
        (_, Some((pid, e))) => (
            format!(
                "{}: {ok} sent, {fail} errors (first: {}: {e})",
                sig.name,
                label(pid)
            ),
            false,
        ),
    };
    app.proc
        .set_status(format!("{}{skip_note}", text.trim()), good);
}

/// F7/F8: nice -1 / +1 sul processo selezionato. Come htop, cambia solo il thread principale
/// (setpriority su un pid agisce sul thread con tid = pid).
pub fn change_nice(app: &mut App, delta: i32) {
    if app.remote {
        app.proc.set_status(crate::ui::app::REMOTE_NA.into(), false);
        return;
    }
    let Some((pid, name, cur)) = app
        .selected_process()
        .map(|p| (p.pid, p.name.clone(), p.nice as i32))
    else {
        return;
    };
    let new = cur + delta;
    if !(-20..=19).contains(&new) {
        app.proc.set_status(
            format!("nice of {pid} {name} already at the limit ({cur})"),
            false,
        );
        return;
    }
    match set_nice(pid, new) {
        Ok(()) => app.proc.set_status(
            format!("nice of {pid} {name} (main thread): {cur} → {new}"),
            true,
        ),
        Err(e) => {
            let why = if delta < 0 && e.contains("EACCES") || e.contains("EPERM") && delta < 0 {
                "root is needed to lower the nice value".to_string()
            } else {
                e
            };
            app.proc
                .set_status(format!("nice of {pid} {name}: {why}"), false);
        }
    }
}

// ---------------------------------------------------------------- albero

/// Pid nascosti perché un loro antenato è piegato.
pub fn hidden_by_fold(rows: &[Process], collapsed: &HashSet<u32>) -> HashSet<u32> {
    let mut out = HashSet::new();
    if collapsed.is_empty() {
        return out;
    }
    let ppid: HashMap<u32, u32> = rows.iter().map(|p| (p.pid, p.ppid)).collect();
    for p in rows {
        let mut cur = p.ppid;
        for _ in 0..rows.len().min(4096) {
            if collapsed.contains(&cur) {
                out.insert(p.pid);
                break;
            }
            match ppid.get(&cur) {
                Some(&n) if n != cur => cur = n,
                _ => break,
            }
        }
    }
    out
}

/// Piega/apre il ramo del processo selezionato (solo in modalità albero).
pub fn toggle_fold(app: &mut App) {
    if !app.tree() {
        return;
    }
    let Some(pid) = app.selected_process().map(|p| p.pid) else {
        return;
    };
    if !app.rows.iter().any(|p| p.ppid == pid && p.pid != pid) {
        return;
    }
    app.select_index(app.selected); // fissa la selezione sul pid
    if !app.proc.collapsed.remove(&pid) {
        app.proc.collapsed.insert(pid);
    }
    app.rebuild_view();
}

// ---------------------------------------------------------------- stato

struct Status {
    text: String,
    ok: bool,
    at: Instant,
}

pub struct ProcState {
    pub tab: DetailTab,
    /// Pid dei rami piegati.
    pub collapsed: HashSet<u32>,
    pub detail: Option<ProcDetail>,
    /// Scheda con cui è stato letto `detail`.
    loaded_tab: DetailTab,
    collector: DetailCollector,
    last_read: Option<Instant>,
    /// Storico CPU% (di un core) del solo processo selezionato.
    pub cpu_hist: History,
    hist_pid: Option<u32>,
    /// Scorrimento del contenuto della scheda.
    pub scroll: usize,
    status: Option<Status>,
    pub kill: KillFn,
    /// Lettura dell'avvio di un pid vivo (sostituibile nei test) e radice di /proc.
    pub start_fn: StartFn,
    pub proc_root: std::path::PathBuf,
    inited: bool,
    /// Zone cliccabili dall'ultimo disegno: (x, larghezza, y, scheda).
    tab_hits: Vec<(u16, u16, u16, DetailTab)>,
    detail_rect: Option<Rect>,
}

impl ProcState {
    pub fn new() -> Self {
        ProcState {
            tab: DetailTab::Generale,
            collapsed: HashSet::new(),
            detail: None,
            loaded_tab: DetailTab::Generale,
            collector: DetailCollector::default(),
            last_read: None,
            cpu_hist: History::raw(HISTORY_LEN),
            hist_pid: None,
            scroll: 0,
            status: None,
            kill: send_signal,
            start_fn: live_start,
            proc_root: std::path::PathBuf::from("/proc"),
            inited: false,
            tab_hits: Vec::new(),
            detail_rect: None,
        }
    }

    pub fn set_status(&mut self, text: String, ok: bool) {
        self.status = Some(Status {
            text,
            ok,
            at: Instant::now(),
        });
    }

    /// Messaggio di stato ancora valido: (testo, riuscito).
    pub fn status_text(&self) -> Option<(&str, bool)> {
        self.status
            .as_ref()
            .filter(|s| s.at.elapsed() < Duration::from_secs(STATUS_SECS))
            .map(|s| (s.text.as_str(), s.ok))
    }

    fn set_tab(&mut self, tab: DetailTab) {
        if self.tab != tab {
            self.tab = tab;
            self.scroll = 0;
        }
    }

    fn load_detail(&mut self, sys: &System, pid: u32) {
        let now = Instant::now();
        let dt = self
            .last_read
            .map_or(0.0, |t| now.duration_since(t).as_secs_f64());
        self.last_read = Some(now);
        self.proc_root.clone_from(&sys.roots.proc);
        self.detail = Some(
            self.collector
                .refresh(&sys.roots, pid, self.tab, sys.clock_ticks, dt),
        );
        self.loaded_tab = self.tab;
    }
}

impl Default for ProcState {
    fn default() -> Self {
        Self::new()
    }
}

/// Dopo ogni `System::refresh`, solo nella sezione Processi.
pub fn update(app: &mut App, sys: &System) {
    if !app.proc.inited {
        app.proc.inited = true;
    }
    let alive: HashSet<u32> = app.rows.iter().map(|p| p.pid).collect();
    let before = app.proc.collapsed.len();
    app.proc.collapsed.retain(|p| alive.contains(p));
    if app.proc.collapsed.len() != before {
        app.rebuild_view();
    }
    let ncores = sys.cpu.cores.len().max(sys.cpu.logical).max(1) as f64;
    let Some((pid, cpu)) = app.selected_process().map(|p| (p.pid, p.cpu_percent)) else {
        app.proc.detail = None;
        app.proc.hist_pid = None;
        return;
    };
    if app.proc.hist_pid != Some(pid) {
        app.proc.cpu_hist = History::raw(HISTORY_LEN);
        app.proc.hist_pid = Some(pid);
        app.proc.scroll = 0;
    }
    app.proc.cpu_hist.push(cpu * ncores);
    app.proc.load_detail(sys, pid);
}

/// Tasti della sezione; `true` se gestiti.
pub fn on_key(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Left => app.proc.set_tab(app.proc.tab.prev()),
        KeyCode::Right => app.proc.set_tab(app.proc.tab.next()),
        KeyCode::Char(' ') => toggle_fold(app),
        KeyCode::Char('k') => open_dialog(app),
        KeyCode::F(7) => change_nice(app, -1),
        KeyCode::F(8) => change_nice(app, 1),
        KeyCode::Char('[') => app.proc.scroll = app.proc.scroll.saturating_sub(3),
        KeyCode::Char(']') => app.proc.scroll += 3,
        _ => return false,
    }
    true
}

/// Clic sulle schede e rotella sul dettaglio; `false` lascia fare alla tabella.
pub fn on_mouse(app: &mut App, m: MouseEvent) -> bool {
    let in_detail = app.proc.detail_rect.is_some_and(|r| {
        m.column >= r.x && m.column < r.x + r.width && m.row >= r.y && m.row < r.y + r.height
    });
    match m.kind {
        MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
            let hit = app
                .proc
                .tab_hits
                .iter()
                .find(|&&(x, w, y, _)| m.row == y && m.column >= x && m.column < x + w)
                .map(|&(_, _, _, t)| t);
            match hit {
                Some(t) => {
                    app.proc.set_tab(t);
                    true
                }
                None => false,
            }
        }
        MouseEventKind::ScrollUp if in_detail => {
            app.proc.scroll = app.proc.scroll.saturating_sub(3);
            true
        }
        MouseEventKind::ScrollDown if in_detail => {
            app.proc.scroll += 3;
            true
        }
        _ => false,
    }
}

// ---------------------------------------------------------------- testo

fn state_text(c: char) -> &'static str {
    match c {
        'R' => "running",
        'S' => "sleeping",
        'D' => "disk wait",
        'T' => "stopped",
        't' => "traced",
        'Z' => "zombie",
        'I' => "idle",
        'X' => "dead",
        _ => "?",
    }
}

/// "1h 21m", "5m 03s", "2d 3h", "45s".
pub fn ago(secs: u64) -> String {
    if secs >= 86400 {
        format!("{}d {}h", secs / 86400, secs % 86400 / 3600)
    } else if secs >= 3600 {
        format!("{}h {:02}m", secs / 3600, secs % 3600 / 60)
    } else if secs >= 60 {
        format!("{}m {:02}s", secs / 60, secs % 60)
    } else {
        format!("{secs}s")
    }
}

/// (anno, mese, giorno) da giorni dal 1970-01-01.
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// "today 07:41 (1h 21m ago)" con l'ora nel fuso `tz`; oltre ieri "2026-10-20 07:30 (...)".
pub fn start_text(now_unix: i64, start_unix: i64, tz: &Tz) -> String {
    let local = |u: i64| u + tz.offset_at(u);
    let (ln, ls) = (local(now_unix), local(start_unix));
    let (dn, ds) = (ln.div_euclid(86400), ls.div_euclid(86400));
    let hm = format!(
        "{:02}:{:02}",
        ls.rem_euclid(86400) / 3600,
        ls.rem_euclid(86400) % 3600 / 60
    );
    let day = match dn - ds {
        0 => "today".to_string(),
        1 => "yesterday".to_string(),
        _ => {
            let (y, m, d) = civil(ds);
            format!("{y}-{m:02}-{d:02}")
        }
    };
    format!(
        "{day} {hm} ({} ago)",
        ago((now_unix - start_unix).max(0) as u64)
    )
}

fn opt_bytes(v: Option<u64>) -> String {
    v.map_or("—".to_string(), fmt::bytes)
}

fn kv(label: &str, value: impl Into<String>) -> Line<'static> {
    Line::from(vec![
        span(format!("{label:<11}"), dim()),
        span(value, fg(theme::FG)),
    ])
}

fn wrap(text: &str, w: usize, max_lines: usize) -> Vec<String> {
    let w = w.max(1);
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<String> = chars
        .chunks(w)
        .take(max_lines)
        .map(|c| c.iter().collect())
        .collect();
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

fn general_lines(
    app: &App,
    sys: &System,
    p: &Process,
    d: Option<&ProcDetail>,
    w: usize,
) -> Vec<Line<'static>> {
    let none = || "—".to_string();
    let mut l = Vec::new();
    let cmd = wrap(&p.cmdline, w.saturating_sub(12), 3);
    for (i, c) in cmd.into_iter().enumerate() {
        l.push(kv(if i == 0 { "Command" } else { "" }, c));
    }
    l.push(kv(
        "Directory",
        d.and_then(|d| d.cwd.clone()).unwrap_or_else(none),
    ));
    l.push(kv(
        "Executable",
        d.and_then(|d| d.exe.clone()).unwrap_or_else(none),
    ));
    let parent = if p.ppid == 0 {
        none()
    } else {
        format!("{} {}", p.ppid, name_of(&app.rows, p.ppid))
    };
    l.push(kv("Parent", parent.trim().to_string()));
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|t| t.as_secs() as i64)
        .unwrap_or(0);
    let start = now - sys.uptime.up_secs as i64 + p.start_secs as i64;
    l.push(kv("Started", start_text(now, start, &app.tz)));
    l.push(kv(
        "State",
        format!("{} ({})", p.state, state_text(p.state)),
    ));
    let sched = d.and_then(|d| d.policy).map_or("—", policy_name);
    l.push(kv(
        "Priority",
        format!("nice {}   prio {}   sched {}", p.nice, p.priority, sched),
    ));
    let aff = d.and_then(|d| d.affinity.clone()).unwrap_or_else(none);
    let core = d
        .and_then(|d| d.last_core)
        .map_or_else(none, |c| c.to_string());
    l.push(kv("Affinity", format!("{aff}    last core {core}")));
    l.push(kv(
        "cgroup",
        d.and_then(|d| d.cgroup_path.clone()).unwrap_or_else(none),
    ));
    l.push(kv("User", format!("{} (uid {})", p.user, p.uid)));
    l.push(kv("Threads", p.threads.to_string()));
    l
}

fn tab_lines(app: &App, sys: &System, p: &Process, w: usize) -> Vec<Line<'static>> {
    let d = app.proc.detail.as_ref().filter(|d| d.pid == p.pid);
    // Dati della scheda non ancora letti (cambio appena avvenuto): attesa.
    if app.proc.tab != DetailTab::Generale && app.proc.loaded_tab != app.proc.tab {
        return vec![Line::styled("reading…", dim())];
    }
    let head = |t: String| Line::styled(t, dim());
    let denied = |t: &str| vec![Line::styled(format!("— {t} (permission denied?)"), dim())];
    match app.proc.tab {
        DetailTab::Generale => general_lines(app, sys, p, d, w),
        DetailTab::Thread => {
            let Some(d) = d else { return denied("threads") };
            let mut l = vec![head(format!(
                "{:>8}  {:<16} {:<5} {:>6}",
                "TID", "Name", "State", "CPU%"
            ))];
            for t in &d.threads {
                l.push(Line::from(vec![
                    span(
                        format!("{:>8}  {:<16} {:<5} ", t.tid, t.name, t.state),
                        fg(theme::FG),
                    ),
                    span(format!("{:>6.1}", t.cpu), fg(pct_color(t.cpu))),
                ]));
            }
            if d.threads.is_empty() {
                return denied("threads");
            }
            l
        }
        DetailTab::File => {
            let Some(d) = d.filter(|d| d.fd_count.is_some()) else {
                return denied("open files");
            };
            let n = d.fd_count.unwrap_or(0);
            let lim = d.fd_limit.clone().unwrap_or_else(|| "—".to_string());
            let mut l = vec![head(format!("{n} open files · limit {lim}"))];
            if n > d.files.len() {
                l[0] = head(format!(
                    "{n} open files (shown {}) · limit {lim}",
                    d.files.len()
                ));
            }
            l.extend(
                d.files
                    .iter()
                    .map(|f| Line::styled(f.clone(), fg(theme::FG))),
            );
            l
        }
        DetailTab::Rete => {
            let Some(d) = d else { return denied("socket") };
            if d.sockets.is_empty() {
                return vec![Line::styled("no TCP/UDP sockets", dim())];
            }
            let lw = (w.saturating_sub(10 + 12 + 2) / 2).clamp(12, 28);
            let mut l = vec![head(format!(
                "{:<5} {:<lw$} {:<lw$} {}",
                "Proto", "Local", "Remote", "State"
            ))];
            for s in &d.sockets {
                l.push(Line::styled(
                    format!(
                        "{:<5} {:<lw$} {:<lw$} {}",
                        s.proto, s.local, s.remote, s.state
                    ),
                    fg(theme::FG),
                ));
            }
            l
        }
        DetailTab::Ambiente => {
            let Some(d) = d.filter(|d| !d.env.is_empty()) else {
                return denied("environment");
            };
            d.env
                .iter()
                .map(|e| match e.split_once('=') {
                    Some((k, v)) => Line::from(vec![
                        span(format!("{k}="), fg(theme::CYAN)),
                        span(v.to_string(), fg(theme::FG)),
                    ]),
                    None => Line::styled(e.clone(), fg(theme::FG)),
                })
                .collect()
        }
        DetailTab::Limiti => {
            let Some(d) = d.filter(|d| !d.limits.is_empty()) else {
                return denied("limits");
            };
            let mut l = vec![head(format!(
                "{:<26} {:>12} {:>12}  {}",
                "Limit", "Soft", "Hard", "Units"
            ))];
            for r in &d.limits {
                l.push(Line::styled(
                    format!("{:<26} {:>12} {:>12}  {}", r.name, r.soft, r.hard, r.units),
                    fg(theme::FG),
                ));
            }
            l
        }
        DetailTab::Cgroup => {
            let Some(g) = d.and_then(|d| d.cgroup.as_ref()) else {
                return denied("cgroup");
            };
            let mem = match g.memory_max {
                Some(None) => format!("{} / max", opt_bytes(g.memory_current)),
                Some(Some(m)) => format!("{} / {}", opt_bytes(g.memory_current), fmt::bytes(m)),
                None => opt_bytes(g.memory_current),
            };
            let num = |v: Option<u64>| v.map_or("—".to_string(), |x| x.to_string());
            vec![
                kv("Path", g.path.clone()),
                kv("Memory", mem),
                kv(
                    "CPU used",
                    g.usage_usec
                        .map_or("—".to_string(), |u| format!("{:.2} s", u as f64 / 1e6)),
                ),
                kv(
                    "Throttling",
                    format!(
                        "{} times, {}",
                        num(g.nr_throttled),
                        g.throttled_usec
                            .map_or("—".to_string(), |u| format!("{:.3} s", u as f64 / 1e6))
                    ),
                ),
                kv("Processes", num(g.pids_current)),
            ]
        }
    }
}

// ---------------------------------------------------------------- disegno

/// Scrive una riga di span entro `w` celle.
fn put_spans(buf: &mut Buffer, x: u16, y: u16, w: u16, line: &Line) {
    let mut cx = x;
    let end = x.saturating_add(w);
    for s in &line.spans {
        if cx >= end {
            return;
        }
        let room = (end - cx) as usize;
        putn(buf, cx, y, &s.content, room, line.style.patch(s.style));
        cx = cx.saturating_add(width(&s.content).min(room) as u16);
    }
}

fn table_columns(w: u16, gpu: bool) -> Vec<ColumnDef> {
    let mut v = vec![ColumnDef::new("PID", 7, true, Some(SortKey::Pid))];
    if w >= 50 {
        v.push(ColumnDef::new("User", 9, false, Some(SortKey::User)));
    }
    v.push(ColumnDef::new("CPU%", 6, true, Some(SortKey::Cpu)));
    v.push(ColumnDef::new("Mem", 7, true, Some(SortKey::Mem)));
    if gpu && w >= 60 {
        v.push(ColumnDef::new("GPU%", 5, true, Some(SortKey::Gpu)));
    }
    v.push(ColumnDef::new("Name", 0, false, Some(SortKey::Name)));
    v
}

fn draw_table(buf: &mut Buffer, area: Rect, app: &mut App, sys: &System) {
    let threads: u64 = sys.processes.iter().map(|p| p.threads as u64).sum();
    let right = format!(
        "filter: {}{}{}",
        app.filter,
        if app.filtering { "_" } else { "" },
        if area.width >= 76 {
            "  ▸ closed ▾ open · Space folds"
        } else {
            ""
        }
    );
    let bottom = format!(
        "{} processes · {} threads · sel. {}/{}",
        sys.processes.len(),
        threads,
        if app.view.is_empty() {
            0
        } else {
            app.selected + 1
        },
        app.view.len()
    );
    let title = if app.tree() {
        "process tree"
    } else {
        "process list"
    };
    let inner = Panel::new(title, theme::rgb(theme::PROC[1]))
        .right(&right)
        .bottom(&bottom)
        .render(area, buf);
    if inner.height < 2 || inner.width == 0 {
        return;
    }
    let cols = table_columns(inner.width, !sys.gpu.devices.is_empty());
    app.page = (inner.height - 1) as usize;
    app.offset = follow(app.selected, app.offset, app.page, app.view.len());
    app.hits.table = Some(inner);
    app.hits.cols = cols.clone();
    let parents: HashSet<u32> = app.rows.iter().map(|p| p.ppid).collect();
    let end = (app.offset + app.page).min(app.view.len());
    let rows: Vec<Vec<Line<'static>>> = app.view[app.offset..end]
        .iter()
        .map(|(idx, prefix)| {
            let p = &app.rows[*idx];
            // Il marcatore ▸/▾ prende il posto di "─ " nel connettore (resta allineato);
            // le radici hanno una colonna di 2 celle dedicata.
            let sym = if parents.contains(&p.pid) && app.tree() {
                Some(if app.proc.collapsed.contains(&p.pid) {
                    "▸ "
                } else {
                    "▾ "
                })
            } else {
                None
            };
            let (head, marker) = match (sym, prefix.is_empty()) {
                (Some(m), false) => {
                    let keep = prefix.chars().count().saturating_sub(2);
                    (prefix.chars().take(keep).collect::<String>(), m)
                }
                (Some(m), true) => (String::new(), m),
                (None, true) if app.tree() => (String::new(), "  "),
                (None, _) => (prefix.clone(), ""),
            };
            cols.iter()
                .map(|c| match c.title {
                    "PID" => Line::from(p.pid.to_string()),
                    "User" => Line::styled(p.user.clone(), dim()),
                    "CPU%" => Line::styled(
                        format!("{:.1}", p.cpu_percent),
                        fg(pct_color(p.cpu_percent)),
                    ),
                    "Mem" => Line::from(fmt::bytes(p.rss)),
                    "GPU%" => Line::styled(
                        format!("{:.1}", p.gpu_percent),
                        fg(theme::grad(theme::GPU, p.gpu_percent / 100.0)),
                    ),
                    _ => Line::from(vec![
                        span(head.clone(), dim()),
                        span(marker, fg(theme::BLUE)),
                        span(p.name.clone(), fg(theme::WHITE)),
                    ]),
                })
                .collect()
        })
        .collect();
    ProcTable {
        cols: &cols,
        rows: &rows,
        selected: app
            .selected
            .checked_sub(app.offset)
            .filter(|&i| i < rows.len()),
        sort: Some((app.sort, app.descending)),
    }
    .render(inner, buf);
}

fn draw_tabs(buf: &mut Buffer, area: Rect, app: &mut App, p: &Process) {
    let d = app.proc.detail.as_ref().filter(|d| d.pid == p.pid);
    let label = |t: DetailTab| match t {
        DetailTab::Thread => format!("Threads {}", p.threads),
        DetailTab::File => match d.and_then(|d| d.fd_count) {
            Some(n) => format!("Files {n}"),
            None => "Files".to_string(),
        },
        _ => t.title().to_string(),
    };
    let labels: Vec<(DetailTab, String)> = DetailTab::ALL.iter().map(|&t| (t, label(t))).collect();
    let active = labels
        .iter()
        .position(|(t, _)| *t == app.proc.tab)
        .unwrap_or(0);
    let wid = |i: usize| width(&labels[i].1) as u16 + 2;
    let mut start = 0;
    while start < active
        && (start..=active).map(wid).sum::<u16>() + (active - start) as u16 > area.width
    {
        start += 1;
    }
    let mut x = area.x;
    app.proc.tab_hits.clear();
    #[allow(clippy::needless_range_loop)]
    for i in start..labels.len() {
        let w = wid(i);
        if x + w > area.x + area.width {
            break;
        }
        let style = if i == active {
            Style::default()
                .fg(theme::WHITE)
                .bg(theme::SEL_BG)
                .add_modifier(Modifier::BOLD)
        } else {
            dim()
        };
        put(buf, x, area.y, format!(" {} ", labels[i].1), style);
        app.proc.tab_hits.push((x, w, area.y, labels[i].0));
        x += w + 1;
    }
}

fn draw_detail(buf: &mut Buffer, area: Rect, app: &mut App, sys: &System) {
    app.proc.detail_rect = Some(area);
    app.proc.tab_hits.clear();
    let color = theme::rgb(theme::NET[1]);
    let Some(p) = app.selected_process().cloned() else {
        let inner = Panel::new("details", color).render(area, buf);
        if inner.height > 0 {
            putn(
                buf,
                inner.x + 1,
                inner.y,
                "no process selected",
                inner.width.saturating_sub(1) as usize,
                dim(),
            );
        }
        return;
    };
    // Dati mancanti (selezione appena cambiata o scheda nuova): lettura subito.
    let stale = app.proc.detail.as_ref().is_none_or(|d| d.pid != p.pid)
        || app.proc.loaded_tab != app.proc.tab;
    if stale {
        app.proc.load_detail(sys, p.pid);
    }
    let h = area.height;
    let mem_h = if h >= 20 { 7 } else { 0 };
    let cpu_h = if h >= 28 { 6 } else { 0 };
    let [main, cpu_a, mem_a] = Layout::vertical([
        Constraint::Min(3),
        Constraint::Length(cpu_h),
        Constraint::Length(mem_h),
    ])
    .areas(area);

    // Schede + contenuto.
    let title = format!("{} · PID {}", p.name, p.pid);
    let inner = Panel::new(&title, color).right(&p.user).render(main, buf);
    if inner.height >= 2 && inner.width > 2 {
        let tabs = Rect::new(inner.x + 1, inner.y, inner.width - 2, 1);
        draw_tabs(buf, tabs, app, &p);
        let body = Rect::new(inner.x + 1, inner.y + 2, inner.width - 2, inner.height - 2);
        if body.height > 0 {
            let lines = tab_lines(app, sys, &p, body.width as usize);
            let max_scroll = lines.len().saturating_sub(body.height as usize);
            app.proc.scroll = app.proc.scroll.min(max_scroll);
            for (i, l) in lines
                .iter()
                .skip(app.proc.scroll)
                .take(body.height as usize)
                .enumerate()
            {
                put_spans(buf, body.x, body.y + i as u16, body.width, l);
            }
            if max_scroll > 0 {
                let pos = format!("{}/{}", app.proc.scroll + 1, max_scroll + 1);
                put(
                    buf,
                    (inner.x + inner.width).saturating_sub(pos.len() as u16 + 1),
                    inner.y,
                    pos,
                    dim(),
                );
            }
        }
    }

    // CPU del solo processo selezionato.
    if cpu_a.height >= 3 {
        let hist = &app.proc.cpu_hist;
        let w = cpu_a.width.saturating_sub(2) as usize;
        let vals = hist.last_n(w * 2);
        let peak = vals.iter().copied().fold(0.0, f64::max);
        let scale = ((peak / 100.0).ceil().max(1.0)) * 100.0;
        let norm: Vec<f64> = vals.iter().map(|v| v / scale).collect();
        let cur = vals.last().copied().unwrap_or(0.0);
        let right = format!("{cur:.1}% of one core · scale {scale:.0}%");
        let inner = Panel::new("CPU", theme::rgb(theme::CPU[1]))
            .right(&right)
            .render(cpu_a, buf);
        BrailleGraph {
            values: &norm,
            gradient: theme::CPU,
            down: false,
        }
        .render(inner, buf);
    }

    // Memoria e I/O.
    if mem_a.height >= 3 {
        let inner = Panel::new("memory & I/O", theme::rgb(theme::MEM[1])).render(mem_a, buf);
        let d = app.proc.detail.as_ref().filter(|d| d.pid == p.pid);
        let rate = |v: Option<f64>| v.map_or("—".to_string(), fmt::rate);
        let left = [
            ("RSS", fmt::bytes(p.rss)),
            ("PSS", opt_bytes(d.and_then(|d| d.pss))),
            ("Shared", opt_bytes(d.and_then(|d| d.shared))),
            ("Swap", opt_bytes(d.and_then(|d| d.swap))),
            (
                "VRAM",
                if p.gpu_mem > 0 {
                    fmt::bytes(p.gpu_mem)
                } else {
                    "—".to_string()
                },
            ),
        ];
        let io = d.and_then(|d| d.io);
        let rt = d.and_then(|d| d.io_rate);
        let right = [
            ("Read", opt_bytes(io.map(|x| x.0))),
            ("Write", opt_bytes(io.map(|x| x.1))),
            ("R/s", rate(rt.map(|x| x.0))),
            ("W/s", rate(rt.map(|x| x.1))),
        ];
        let half = inner.width / 2;
        for (i, (k, v)) in left.iter().enumerate() {
            let y = inner.y + i as u16;
            if y >= inner.y + inner.height {
                break;
            }
            let line = kv(k, v.clone());
            put_spans(buf, inner.x + 1, y, half.saturating_sub(1), &line);
        }
        for (i, (k, v)) in right.iter().enumerate() {
            let y = inner.y + i as u16;
            if y >= inner.y + inner.height {
                break;
            }
            let line = kv(k, v.clone());
            put_spans(buf, inner.x + half, y, inner.width - half, &line);
        }
    }
}

pub fn draw(buf: &mut Buffer, area: Rect, app: &mut App, sys: &System) {
    if area.width < 4 || area.height < 3 {
        return;
    }
    if area.width >= 100 {
        let lw = area.width * 55 / 100;
        let [l, r] = Layout::horizontal([Constraint::Length(lw), Constraint::Min(1)]).areas(area);
        draw_table(buf, l, app, sys);
        draw_detail(buf, r, app, sys);
    } else {
        let th = (area.height * 45 / 100)
            .max(4)
            .min(area.height.saturating_sub(3));
        let [t, d] = Layout::vertical([Constraint::Length(th), Constraint::Min(1)]).areas(area);
        draw_table(buf, t, app, sys);
        draw_detail(buf, d, app, sys);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collect::Roots;
    use crate::ui::app::Section;
    use crate::ui::snapshot::{render_app, to_text};
    use crossterm::event::{KeyEventKind, KeyEventState, KeyModifiers};
    use std::path::PathBuf;
    use std::sync::Mutex;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn proc(pid: u32, ppid: u32, name: &str) -> Process {
        Process {
            pid,
            ppid,
            name: name.into(),
            user: "u".into(),
            cmdline: format!("/bin/{name}"),
            ..Process::default()
        }
    }

    /// 1 → 10 → (11, 12 → 13); 20 isolato.
    fn rows() -> Vec<Process> {
        vec![
            proc(1, 0, "init"),
            proc(10, 1, "sh"),
            proc(11, 10, "a"),
            proc(12, 10, "b"),
            proc(13, 12, "c"),
            proc(20, 0, "x"),
        ]
    }

    #[test]
    fn sottoalbero() {
        let r = rows();
        assert_eq!(subtree_pids(&r, 10), [10, 11, 12, 13]);
        assert_eq!(subtree_pids(&r, 13), [13]);
        assert_eq!(subtree_pids(&r, 999), [999]);
        let mut cyc = rows();
        cyc[1].ppid = 13; // ciclo: nessun loop infinito
        assert!(subtree_pids(&cyc, 10).len() <= 6);
    }

    #[test]
    fn piegamento() {
        let r = rows();
        let c: HashSet<u32> = [12].into_iter().collect();
        let h = hidden_by_fold(&r, &c);
        assert_eq!(h, [13].into_iter().collect());
        let c: HashSet<u32> = [10].into_iter().collect();
        let h = hidden_by_fold(&r, &c);
        assert_eq!(h, [11, 12, 13].into_iter().collect());
        assert!(hidden_by_fold(&r, &HashSet::new()).is_empty());
    }

    fn app_albero() -> App {
        let mut a = App::new((120, 40), "h".into());
        a.section = Section::Processi;
        a.tree_proc = true;
        a.set_processes(rows());
        a
    }

    #[test]
    fn spazio_piega_e_apre() {
        let mut a = app_albero();
        let n = a.view.len();
        let pos = a
            .view
            .iter()
            .position(|(i, _)| a.rows[*i].pid == 10)
            .unwrap();
        a.select_index(pos);
        a.on_key(key(KeyCode::Char(' ')));
        assert_eq!(a.view.len(), n - 3);
        assert!(a.proc.collapsed.contains(&10));
        // la selezione resta sul ramo piegato
        assert_eq!(a.selected_process().unwrap().pid, 10);
        a.on_key(key(KeyCode::Char(' ')));
        assert_eq!(a.view.len(), n);
        // foglia: niente da piegare
        let leaf = a
            .view
            .iter()
            .position(|(i, _)| a.rows[*i].pid == 11)
            .unwrap();
        a.select_index(leaf);
        a.on_key(key(KeyCode::Char(' ')));
        assert!(a.proc.collapsed.is_empty());
        // in lista lo Spazio non fa nulla
        a.on_key(key(KeyCode::F(5)));
        a.on_key(key(KeyCode::Char(' ')));
        assert!(a.proc.collapsed.is_empty());
    }

    static SENT: Mutex<Vec<(u32, i32)>> = Mutex::new(Vec::new());

    fn fake_start(_: &std::path::Path, _: u32) -> Option<u64> {
        Some(0)
    }

    fn fake_kill(_: &std::path::Path, pid: u32, _: u64, sig: Sig) -> Result<(), String> {
        if pid == 13 {
            return Err("permission denied (EPERM)".into());
        }
        SENT.lock().unwrap().push((pid, sig.num));
        Ok(())
    }

    #[test]
    fn dialogo_selezione_e_invio() {
        let mut a = app_albero();
        a.proc.kill = fake_kill;
        a.proc.start_fn = fake_start;
        let pos = a
            .view
            .iter()
            .position(|(i, _)| a.rows[*i].pid == 10)
            .unwrap();
        a.select_index(pos);
        a.on_key(key(KeyCode::F(9)));
        let d = a.dialog.clone().unwrap();
        assert_eq!((d.pid, d.signal().name), (10, "SIGTERM"));
        assert_eq!(d.tree_pids, [10, 11, 12, 13]);
        a.on_key(key(KeyCode::Char('q'))); // inghiottito
        assert!(!a.should_quit);
        a.on_key(key(KeyCode::Down));
        assert_eq!(a.dialog.as_ref().unwrap().signal().num, 9);
        for _ in 0..20 {
            a.on_key(key(KeyCode::Down));
        }
        assert_eq!(a.dialog.as_ref().unwrap().signal().name, "SIGUSR2");
        a.on_key(key(KeyCode::Esc));
        assert!(a.dialog.is_none());
        assert!(SENT.lock().unwrap().is_empty() || SENT.lock().unwrap().iter().all(|x| x.0 != 10));

        // Invio su un solo processo
        SENT.lock().unwrap().clear();
        let pos = a
            .view
            .iter()
            .position(|(i, _)| a.rows[*i].pid == 11)
            .unwrap();
        a.select_index(pos);
        a.on_key(key(KeyCode::F(9)));
        a.on_key(key(KeyCode::Enter));
        assert!(a.dialog.is_none());
        assert_eq!(*SENT.lock().unwrap(), [(11, 15)]);
        let (t, ok) = a.proc.status_text().unwrap();
        assert!(ok);
        assert_eq!(t, "SIGTERM sent to 11 a");

        // sottoalbero con un errore sul 13
        SENT.lock().unwrap().clear();
        let pos = a
            .view
            .iter()
            .position(|(i, _)| a.rows[*i].pid == 10)
            .unwrap();
        a.select_index(pos);
        a.on_key(key(KeyCode::F(9)));
        a.on_key(key(KeyCode::Char(' ')));
        assert!(
            a.dialog
                .as_ref()
                .unwrap()
                .lines()
                .iter()
                .any(|l| l.contains("(4 processes)"))
        );
        a.on_key(key(KeyCode::Enter));
        // processi di altri utenti: seconda conferma (da root non serve)
        if a.dialog.is_some() {
            a.on_key(key(KeyCode::Enter));
        }
        assert_eq!(*SENT.lock().unwrap(), [(10, 15), (11, 15), (12, 15)]);
        let (t, ok) = a.proc.status_text().unwrap();
        assert!(!ok);
        assert!(t.contains("3 sent, 1 errors") && t.contains("EPERM"), "{t}");
    }

    #[test]
    fn pid_uno_e_se_stessi_protetti() {
        let mut r = rows();
        r.push(proc(std::process::id(), 10, "ngtop"));
        let mut a = App::new((120, 40), "h".into());
        a.tree_proc = true;
        a.set_processes(r);
        a.proc.kill = fake_kill;
        a.proc.start_fn = fake_start;
        SENT.lock().unwrap().clear();
        let pos = a
            .view
            .iter()
            .position(|(i, _)| a.rows[*i].pid == 1)
            .unwrap();
        a.select_index(pos);
        a.on_key(key(KeyCode::F(9)));
        let d = a.dialog.clone().unwrap();
        assert!(d.warnings().iter().any(|w| w.contains("PID 1")));
        assert!(d.targets().is_empty());
        a.on_key(key(KeyCode::Esc));
        let pos = a
            .view
            .iter()
            .position(|(i, _)| a.rows[*i].pid == 10)
            .unwrap();
        a.select_index(pos);
        a.on_key(key(KeyCode::F(9)));
        a.on_key(key(KeyCode::Char(' ')));
        let d = a.dialog.clone().unwrap();
        let t = d.targets();
        assert!(!t.contains(&1) && !t.contains(&std::process::id()) && t.contains(&10));
        assert!(d.warnings().iter().any(|w| w.contains("ngtop itself")));
        a.on_key(key(KeyCode::Enter));
        if a.dialog.is_some() {
            a.on_key(key(KeyCode::Enter));
        }
        assert!(
            SENT.lock()
                .unwrap()
                .iter()
                .all(|x| x.0 != 1 && x.0 != std::process::id())
        );
        // diretto su ngtop: avviso ma consentito
        let pos = a
            .view
            .iter()
            .position(|(i, _)| a.rows[*i].pid == std::process::id())
            .unwrap();
        a.select_index(pos);
        a.on_key(key(KeyCode::F(9)));
        let d = a.dialog.clone().unwrap();
        assert!(
            d.warnings()
                .iter()
                .any(|w| w.contains("you are signalling ngtop"))
        );
        assert_eq!(d.targets(), [std::process::id()]);
        // senza processi non si apre
        let mut b = App::new((80, 24), "h".into());
        b.on_key(key(KeyCode::F(9)));
        assert!(b.dialog.is_none());
    }

    static CALLS_A: Mutex<Vec<u32>> = Mutex::new(Vec::new());
    static CALLS_B: Mutex<Vec<u32>> = Mutex::new(Vec::new());
    static CALLS_C: Mutex<Vec<u32>> = Mutex::new(Vec::new());

    fn kill_a(_: &std::path::Path, pid: u32, _: u64, _: Sig) -> Result<(), String> {
        CALLS_A.lock().unwrap().push(pid);
        Ok(())
    }
    fn kill_b(_: &std::path::Path, pid: u32, _: u64, _: Sig) -> Result<(), String> {
        CALLS_B.lock().unwrap().push(pid);
        Ok(())
    }
    fn kill_c(_: &std::path::Path, pid: u32, _: u64, _: Sig) -> Result<(), String> {
        CALLS_C.lock().unwrap().push(pid);
        Ok(())
    }

    fn select_pid(a: &mut App, pid: u32) {
        let pos = a
            .view
            .iter()
            .position(|(i, _)| a.rows[*i].pid == pid)
            .unwrap();
        a.select_index(pos);
    }

    #[test]
    fn sottoalbero_di_init_vietato() {
        for root in [1u32, 2] {
            let mut r = rows();
            r.push(proc(2, 0, "kthreadd"));
            r.push(proc(30, 2, "kw"));
            let mut a = App::new((120, 40), "h".into());
            a.set_processes(r);
            a.proc.kill = kill_a;
            a.proc.start_fn = fake_start;
            select_pid(&mut a, root);
            a.on_key(key(KeyCode::F(9)));
            a.on_key(key(KeyCode::Char(' ')));
            assert!(!a.dialog.as_ref().unwrap().subtree);
            // anche forzando il campo, targets() resta vuoto
            a.dialog.as_mut().unwrap().subtree = true;
            assert!(a.dialog.as_ref().unwrap().targets().is_empty());
            a.on_key(key(KeyCode::Enter));
            a.on_key(key(KeyCode::Enter));
        }
        assert!(CALLS_A.lock().unwrap().is_empty());
    }

    #[test]
    fn pid_riusato_non_riceve_nulla() {
        fn reused(_: &std::path::Path, pid: u32) -> Option<u64> {
            // 11 è stato sostituito da un processo diverso (altro avvio); 12 è sparito
            match pid {
                11 => Some(777),
                12 => None,
                _ => Some(0),
            }
        }
        let mut a = app_albero();
        a.proc.kill = kill_b;
        a.proc.start_fn = reused;
        select_pid(&mut a, 11);
        a.on_key(key(KeyCode::F(9)));
        a.on_key(key(KeyCode::Enter));
        assert!(CALLS_B.lock().unwrap().is_empty());
        let (t, ok) = a.proc.status_text().unwrap();
        assert!(!ok && t.contains("has exited, no signal"), "{t}");
        // sottoalbero di 10: solo 10 e 13 sono ancora gli stessi processi
        select_pid(&mut a, 10);
        a.on_key(key(KeyCode::F(9)));
        a.on_key(key(KeyCode::Char(' ')));
        a.on_key(key(KeyCode::Enter));
        if a.dialog.is_some() {
            a.on_key(key(KeyCode::Enter));
        }
        assert_eq!(*CALLS_B.lock().unwrap(), [10, 13]);
        let (t, _) = a.proc.status_text().unwrap();
        assert!(t.contains("2 no longer exist"), "{t}");
    }

    static CALLS_D: Mutex<Vec<u32>> = Mutex::new(Vec::new());
    static CALLS_E: Mutex<Vec<u32>> = Mutex::new(Vec::new());

    fn kill_d(_: &std::path::Path, pid: u32, _: u64, _: Sig) -> Result<(), String> {
        CALLS_D.lock().unwrap().push(pid);
        Ok(())
    }
    fn kill_e(_: &std::path::Path, pid: u32, _: u64, _: Sig) -> Result<(), String> {
        CALLS_E.lock().unwrap().push(pid);
        Ok(())
    }

    fn set_start(a: &mut App, pid: u32, st: u64) {
        let mut r = a.rows.clone();
        r.iter_mut()
            .filter(|p| p.pid == pid)
            .for_each(|p| p.start_ticks = st);
        a.set_processes(r);
    }

    #[test]
    fn radice_riusata_i_figli_del_nuovo_non_ricevono() {
        fn live(_: &std::path::Path, pid: u32) -> Option<u64> {
            Some(match pid {
                10 => 7,
                40 | 41 => 8,
                _ => 0,
            })
        }
        let mut a = app_albero();
        a.proc.kill = kill_d;
        a.proc.start_fn = live;
        select_pid(&mut a, 10);
        a.on_key(key(KeyCode::F(9)));
        a.on_key(key(KeyCode::Char(' ')));
        // dopo l'apertura: 10 muore e il pid va a X, che ha due figli nuovi
        set_start(&mut a, 10, 7);
        let mut r = a.rows.clone();
        for pid in [40, 41] {
            let mut c = proc(pid, 10, "figlio");
            c.start_ticks = 8;
            r.push(c);
        }
        a.set_processes(r);
        a.on_key(key(KeyCode::Enter));
        if a.dialog.is_some() {
            a.on_key(key(KeyCode::Enter));
        }
        assert!(CALLS_D.lock().unwrap().is_empty());
        let (t, ok) = a.proc.status_text().unwrap();
        assert!(!ok && t.contains("has exited, no signal"), "{t}");
    }

    #[test]
    fn nodo_intermedio_riusato_salta_il_ramo() {
        fn live(_: &std::path::Path, pid: u32) -> Option<u64> {
            Some(match pid {
                12 => 9,
                60 => 1,
                _ => 0,
            })
        }
        let mut a = app_albero();
        a.proc.kill = kill_e;
        a.proc.start_fn = live;
        select_pid(&mut a, 10);
        a.on_key(key(KeyCode::F(9)));
        a.on_key(key(KeyCode::Char(' ')));
        // 12 viene riusato (con il figlio 13 ormai di altri); 60 è un figlio nuovo di 10
        set_start(&mut a, 12, 9);
        let mut r = a.rows.clone();
        let mut n = proc(60, 10, "nuovo");
        n.start_ticks = 1;
        r.push(n);
        a.set_processes(r);
        a.on_key(key(KeyCode::Enter));
        if a.dialog.is_some() {
            a.on_key(key(KeyCode::Enter));
        }
        assert_eq!(*CALLS_E.lock().unwrap(), [10, 11, 60]);
    }

    #[test]
    fn sottoalbero_grande_chiede_seconda_conferma() {
        let mut r = rows();
        for i in 0..60 {
            r.push(proc(100 + i, 10, "f"));
        }
        for p in &mut r {
            p.uid = rustix::process::geteuid().as_raw(); // nessun processo di altri utenti
        }
        let mut a = App::new((120, 40), "h".into());
        a.set_processes(r);
        a.proc.kill = kill_c;
        a.proc.start_fn = fake_start;
        select_pid(&mut a, 10);
        a.on_key(key(KeyCode::F(9)));
        a.on_key(key(KeyCode::Char(' ')));
        a.on_key(key(KeyCode::Enter));
        assert!(a.dialog.as_ref().unwrap().armed);
        assert!(CALLS_C.lock().unwrap().is_empty());
        a.on_key(key(KeyCode::Enter));
        assert!(a.dialog.is_none());
        assert_eq!(CALLS_C.lock().unwrap().len(), 64);
    }

    #[test]
    fn salto_a_pid_apre_rami_e_toglie_filtro() {
        let mut a = app_albero();
        select_pid(&mut a, 10);
        a.on_key(key(KeyCode::Char(' '))); // piega 10
        assert!(a.view.iter().all(|(i, _)| a.rows[*i].pid != 13));
        a.section = Section::Panoramica;
        a.jump_to_pid(13);
        assert_eq!(a.section, Section::Processi);
        assert_eq!(a.selected_process().unwrap().pid, 13);
        assert!(a.proc.collapsed.is_empty());
        // filtro che lo esclude
        a.filter = "init".into();
        a.rebuild_view();
        a.jump_to_pid(20);
        assert_eq!(a.filter, "");
        assert_eq!(a.selected_process().unwrap().pid, 20);
        // pid inesistente: nessuna selezione sbagliata, messaggio
        a.jump_to_pid(4242);
        assert_eq!(a.selected_process().unwrap().pid, 20);
        assert!(a.proc.status_text().unwrap().0.contains("has exited"));
    }

    #[test]
    fn albero_separato_per_sezione() {
        let mut a = App::new((120, 40), "h".into());
        a.set_processes(rows());
        assert!(!a.tree());
        a.set_section(Section::Processi);
        assert!(a.tree());
        assert!(a.view[1].1.contains('─') || a.view.iter().any(|(_, p)| !p.is_empty()));
        a.on_key(key(KeyCode::F(5)));
        assert!(!a.tree());
        a.set_section(Section::Panoramica);
        assert!(!a.tree());
        assert!(a.view.iter().all(|(_, p)| p.is_empty()));
    }

    #[test]
    fn segnale_reale_a_un_figlio() {
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("sleep");
        let pid = child.id();
        let start = live_start(std::path::Path::new("/proc"), pid).unwrap();
        send_signal(std::path::Path::new("/proc"), pid, start, SIGNALS[0]).unwrap();
        let st = child.wait().unwrap();
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(st.signal(), Some(15));
        // processo ormai inesistente (già atteso): errore leggibile, niente panic
        let e = send_signal(std::path::Path::new("/proc"), pid, start, SIGNALS[0]).unwrap_err();
        assert!(e.contains("ESRCH"), "{e}");
    }

    #[test]
    fn nice_su_un_figlio() {
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("sleep");
        let pid = child.id();
        assert!(set_nice(pid, 5).is_ok());
        let n = rustix::process::getpriority_process(to_pid(pid)).unwrap();
        assert_eq!(n, 5);
        // abbassare senza root non riesce (se si è root il test non ha senso)
        if rustix::process::getuid().as_raw() != 0 {
            assert!(set_nice(pid, 4).is_err());
        }
        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn testo_avvio() {
        let tz = Tz::Utc;
        let now = 1_700_000_000; // 2023-11-14 22:13:20 UTC
        assert_eq!(start_text(now, now - 4860, &tz), "today 20:52 (1h 21m ago)");
        assert!(start_text(now, now - 86400, &tz).starts_with("yesterday"));
        assert!(start_text(now, now - 5 * 86400, &tz).starts_with("2023-11-09 22:13"));
        assert_eq!(ago(45), "45s");
        assert_eq!(ago(303), "5m 03s");
        assert_eq!(ago(2 * 86400 + 3 * 3600), "2d 3h");
        assert_eq!(civil(0), (1970, 1, 1));
    }

    #[test]
    fn disegno_senza_panic_alle_misure_previste() {
        let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/snap_a");
        let mut sys = System::new(Roots::with_dirs(
            d.join("proc"),
            d.join("sys"),
            d.join("etc"),
        ));
        sys.refresh_with_dt(1.0);
        sys.refresh_with_dt(1.0);
        for (w, h) in [(182, 52), (120, 40), (100, 30), (80, 24), (30, 8), (10, 4)] {
            for tab in DetailTab::ALL {
                let mut app = App::new((w, h), "h".into());
                app.section = Section::Processi;
                app.update(&sys);
                app.proc.tab = tab;
                if let Some(i) = app.view.iter().position(|(i, _)| app.rows[*i].pid == 100) {
                    app.select_index(i);
                }
                app.update(&sys);
                let t = to_text(&render_app(&sys, &mut app));
                assert_eq!(t.lines().count(), h as usize);
                if (w, h) == (182, 52) && tab == DetailTab::Generale {
                    assert!(t.contains("process tree"), "{t}");
                    assert!(t.contains("Web Content · PID 100"));
                    assert!(t.contains("0-3"));
                    assert!(t.contains("memory & I/O"));
                    assert!(t.contains("PSS"));
                }
                if (w, h) == (182, 52) && tab == DetailTab::Rete {
                    assert!(t.contains("127.0.0.1:53"), "{t}");
                }
            }
        }
    }
}
