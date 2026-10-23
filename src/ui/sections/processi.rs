// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Sezione 3 "Processi": albero a sinistra, dettaglio a schede a destra, segnali e nice.
//! Contiene anche il dialogo "Invia segnale" (comune a Panoramica e Processi).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;

use super::panoramica::{dim, fg, pct_color, span};
use crate::collect::System;
use crate::collect::procdetail::{DetailCollector, DetailTab, ProcDetail, policy_name};
use crate::collect::process::{Process, SortKey};
use crate::source::actions::{NoActions, ProcActions};
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

/// Oltre questa soglia il sottoalbero richiede una seconda conferma.
const BIG_SUBTREE: usize = 50;

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
    /// Pid del sottoalbero da non colpire alla leggera, con il motivo (chiesto alle azioni).
    pub prot: Vec<(u32, &'static str)>,
    /// Macchina remota su cui si agisce (destinazione), se non è questa.
    pub machine: Option<String>,
    /// Connessione remota all'apertura (0 in locale).
    pub generation: u64,
}

/// Inizio del testo con cui il trasporto dichiara un esito incerto (connessione caduta).
const UNKNOWN_OUTCOME: &str = "connection lost: outcome unknown";

/// Dove si agisce, per contare i processi "di altri utenti" e nominare la macchina.
pub struct Origin<'a> {
    pub target: &'a str,
    /// Uid con cui si agisce sul server, se noto (ignoto: ogni processo conta come di altri).
    pub uid: Option<u32>,
    /// Numero della connessione all'apertura: se cambia, l'invio è rifiutato.
    pub generation: u64,
}

impl SignalDialog {
    pub fn new(
        p: &Process,
        rows: &[Process],
        act: &dyn ProcActions,
        origin: Option<&Origin>,
    ) -> Self {
        let tree_pids = subtree_pids(rows, p.pid);
        let in_tree: HashSet<u32> = tree_pids.iter().copied().collect();
        let me = rustix::process::geteuid().as_raw();
        // In locale si confronta l'uid; in remoto quello del server (ignoto: tutti di altri, prudente).
        let foreign_of = |q: &Process| match origin {
            None => q.uid != me,
            Some(o) => o.uid != Some(q.uid),
        };
        let mut tree_starts = vec![(p.pid, p.start_ticks)];
        let mut foreign = 0;
        for q in rows.iter().filter(|q| q.pid != p.pid) {
            if in_tree.contains(&q.pid) {
                tree_starts.push((q.pid, q.start_ticks));
                foreign += usize::from(foreign_of(q));
            }
        }
        foreign += usize::from(foreign_of(p));
        let prot = tree_pids
            .iter()
            .filter_map(|&t| act.protected(t).map(|why| (t, why)))
            .collect();
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
            prot,
            machine: origin.map(|o| o.target.to_string()),
            generation: origin.map_or(0, |o| o.generation),
        }
    }

    fn protected(&self, pid: u32) -> Option<&'static str> {
        self.prot.iter().find(|&&(p, _)| p == pid).map(|&(_, w)| w)
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
                .filter(|&p| self.protected(p).is_none())
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
            match self.protected(p) {
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
        if let Some(m) = &self.machine {
            l.push(format!("On: {m}"));
        }
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
        app.dialog = Some(dialog_for(app, p));
    }
}

/// Dialogo per `p`, con le azioni e (in remoto) la macchina dell'app.
fn dialog_for(app: &App, p: &Process) -> SignalDialog {
    let origin = app.remote.then(|| Origin {
        target: &app.remote_target,
        uid: app.remote_uid,
        generation: app.remote_generation,
    });
    SignalDialog::new(p, &app.rows, &*app.proc.act, origin.as_ref())
}

/// Apre il dialogo su un pid noto (es. la connessione scelta in Rete e dischi).
pub fn open_dialog_pid(app: &mut App, pid: u32) {
    if let Some(p) = app.rows.iter().find(|p| p.pid == pid) {
        app.dialog = Some(dialog_for(app, p));
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
    // Riconnessione (o server riavviato) dopo l'apertura: pid e protezioni non valgono più.
    if app.remote && d.generation != app.remote_generation {
        app.proc.set_status(
            "the connection changed since the dialog was opened: nothing sent".into(),
            false,
        );
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
    let act = app.proc.act.clone();
    let cand: Vec<(u32, u64)> = if d.subtree {
        verified_subtree(&app.rows, d.pid, &stored)
            .into_iter()
            .filter(|&(p, _)| act.protected(p).is_none())
            .collect()
    } else {
        stored
            .get(&d.pid)
            .map(|&e| vec![(d.pid, e)])
            .unwrap_or_default()
    };
    // Una sola lettura degli avvii: radice per prima, poi i candidati.
    let mut ask = vec![d.pid];
    ask.extend(cand.iter().map(|&(p, _)| p));
    let live = act.live_starts(&ask);
    // La radice si verifica per prima: se il pid è stato riusato non si invia nulla.
    if stored
        .get(&d.pid)
        .is_none_or(|&e| live.first().copied().flatten() != Some(e))
    {
        app.proc.set_status(
            format!("process {} has exited, no signal sent", d.pid),
            false,
        );
        return;
    }
    let cand: Vec<(u32, u64)> = cand
        .into_iter()
        .zip(live.into_iter().skip(1))
        .filter(|&((_, e), l)| l == Some(e))
        .map(|(c, _)| c)
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
    let (mut ok, mut fail) = (0usize, 0usize);
    let mut first_err: Option<(u32, String)> = None;
    let mut unknown: Option<String> = None;
    // Una sola chiamata per tutto il sottoalbero.
    for (&(pid, _), res) in cand.iter().zip(act.signal(&cand, sig.num)) {
        match res {
            Ok(()) => ok += 1,
            Err(e) => {
                fail += 1;
                if e.starts_with(UNKNOWN_OUTCOME) {
                    unknown.get_or_insert_with(|| e.clone());
                }
                first_err.get_or_insert((pid, e));
            }
        }
    }
    let label = |pid: u32| format!("{} {}", pid, name_of(&app.rows, pid).trim().to_owned());
    let (text, good) = match (targets.len(), first_err) {
        _ if unknown.is_some() => (
            format!("{}: {}", sig.name, unknown.unwrap_or_default()),
            false,
        ),
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
    let Some((pid, name, cur, start)) = app
        .selected_process()
        .map(|p| (p.pid, p.name.clone(), p.nice as i32, p.start_ticks))
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
    // L'avvio letto con la lista impedisce di colpire un pid riusato nel frattempo.
    match app.proc.act.set_nice(pid, start, new) {
        Ok(()) => app.proc.set_status(
            format!("nice of {pid} {name} (main thread): {cur} → {new}"),
            true,
        ),
        Err(e) => {
            let denied = e.contains("EACCES") || e.contains("EPERM");
            let why = if delta < 0 && denied && !app.remote_root {
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
    /// Segnali e nice sul sistema osservato (locale, remoto o finto nei test).
    pub act: Arc<dyn ProcActions>,
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
            // Mai azioni locali per default: le assegna chi crea l'app da `sys.roots.act`.
            act: Arc::new(NoActions),
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

/// Azioni finte per i test: registrano i segnali e il nice, niente tocca processi veri.
#[cfg(test)]
#[derive(Default)]
pub struct FakeActions {
    pub sent: std::sync::Mutex<Vec<(u32, i32)>>,
    pub nices: std::sync::Mutex<Vec<(u32, u64, i32)>>,
    /// Avvio dei processi vivi (predefinito: tutti vivi con avvio 0).
    pub live: Option<fn(u32) -> Option<u64>>,
    /// Pid per cui il segnale fallisce con EPERM.
    pub fail_pid: Option<u32>,
    pub nice_err: Option<String>,
    /// Errore per ogni segnale (prevale su `fail_pid`).
    pub sig_err: Option<String>,
    pub starts_calls: std::sync::atomic::AtomicUsize,
    pub signal_calls: std::sync::atomic::AtomicUsize,
}

#[cfg(test)]
impl ProcActions for FakeActions {
    fn live_starts(&self, pids: &[u32]) -> Vec<Option<u64>> {
        self.starts_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        pids.iter()
            .map(|&p| self.live.map_or(Some(0), |f| f(p)))
            .collect()
    }
    fn signal(&self, targets: &[(u32, u64)], sig: i32) -> Vec<Result<(), String>> {
        self.signal_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        targets
            .iter()
            .map(|&(pid, _)| {
                if let Some(e) = &self.sig_err {
                    return Err(e.clone());
                }
                if Some(pid) == self.fail_pid {
                    return Err("permission denied (EPERM)".to_string());
                }
                self.sent.lock().unwrap().push((pid, sig));
                Ok(())
            })
            .collect()
    }
    fn set_nice(&self, pid: u32, expect: u64, nice: i32) -> Result<(), String> {
        if let Some(e) = &self.nice_err {
            return Err(e.clone());
        }
        self.nices.lock().unwrap().push((pid, expect, nice));
        Ok(())
    }
    fn protected(&self, pid: u32) -> Option<&'static str> {
        crate::source::actions::protected(pid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collect::Roots;
    use crate::source::actions::LocalActions;
    use crate::ui::app::Section;
    use crate::ui::snapshot::{render_app, to_text};
    use crossterm::event::{KeyEventKind, KeyEventState, KeyModifiers};
    use std::path::PathBuf;
    use std::sync::atomic::Ordering;

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

    fn fake(f: FakeActions) -> (Arc<FakeActions>, Arc<dyn ProcActions>) {
        let a = Arc::new(f);
        (a.clone(), a)
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
    fn dialogo_selezione_e_invio() {
        let mut a = app_albero();
        let (f, act) = fake(FakeActions {
            fail_pid: Some(13),
            ..FakeActions::default()
        });
        a.proc.act = act;
        select_pid(&mut a, 10);
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
        assert!(f.sent.lock().unwrap().is_empty());

        // Invio su un solo processo
        select_pid(&mut a, 11);
        a.on_key(key(KeyCode::F(9)));
        a.on_key(key(KeyCode::Enter));
        assert!(a.dialog.is_none());
        assert_eq!(*f.sent.lock().unwrap(), [(11, 15)]);
        let (t, ok) = a.proc.status_text().unwrap();
        assert!(ok);
        assert_eq!(t, "SIGTERM sent to 11 a");

        // sottoalbero con un errore sul 13; una sola chiamata per le letture e una per l'invio
        f.sent.lock().unwrap().clear();
        f.starts_calls.store(0, Ordering::SeqCst);
        f.signal_calls.store(0, Ordering::SeqCst);
        select_pid(&mut a, 10);
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
        assert_eq!(*f.sent.lock().unwrap(), [(10, 15), (11, 15), (12, 15)]);
        assert_eq!(f.starts_calls.load(Ordering::SeqCst), 1);
        assert_eq!(f.signal_calls.load(Ordering::SeqCst), 1);
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
        let (f, act) = fake(FakeActions::default());
        a.proc.act = act;
        select_pid(&mut a, 1);
        a.on_key(key(KeyCode::F(9)));
        let d = a.dialog.clone().unwrap();
        assert!(d.warnings().iter().any(|w| w.contains("PID 1")));
        assert!(d.targets().is_empty());
        a.on_key(key(KeyCode::Esc));
        select_pid(&mut a, 10);
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
            f.sent
                .lock()
                .unwrap()
                .iter()
                .all(|x| x.0 != 1 && x.0 != std::process::id())
        );
        // diretto su ngtop: avviso ma consentito
        select_pid(&mut a, std::process::id());
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

    #[test]
    fn sottoalbero_di_init_vietato() {
        for root in [1u32, 2] {
            let mut r = rows();
            r.push(proc(2, 0, "kthreadd"));
            r.push(proc(30, 2, "kw"));
            let mut a = App::new((120, 40), "h".into());
            a.set_processes(r);
            let (f, act) = fake(FakeActions::default());
            a.proc.act = act;
            select_pid(&mut a, root);
            a.on_key(key(KeyCode::F(9)));
            a.on_key(key(KeyCode::Char(' ')));
            assert!(!a.dialog.as_ref().unwrap().subtree);
            // anche forzando il campo, targets() resta vuoto
            a.dialog.as_mut().unwrap().subtree = true;
            assert!(a.dialog.as_ref().unwrap().targets().is_empty());
            a.on_key(key(KeyCode::Enter));
            a.on_key(key(KeyCode::Enter));
            assert!(f.sent.lock().unwrap().is_empty());
        }
    }

    #[test]
    fn pid_riusato_non_riceve_nulla() {
        // 11 è stato sostituito da un processo diverso (altro avvio); 12 è sparito
        let (f, act) = fake(FakeActions {
            live: Some(|pid| match pid {
                11 => Some(777),
                12 => None,
                _ => Some(0),
            }),
            ..FakeActions::default()
        });
        let mut a = app_albero();
        a.proc.act = act;
        select_pid(&mut a, 11);
        a.on_key(key(KeyCode::F(9)));
        a.on_key(key(KeyCode::Enter));
        assert!(f.sent.lock().unwrap().is_empty());
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
        let pids: Vec<u32> = f.sent.lock().unwrap().iter().map(|x| x.0).collect();
        assert_eq!(pids, [10, 13]);
        let (t, _) = a.proc.status_text().unwrap();
        assert!(t.contains("2 no longer exist"), "{t}");
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
        let (f, act) = fake(FakeActions {
            live: Some(|pid| {
                Some(match pid {
                    10 => 7,
                    40 | 41 => 8,
                    _ => 0,
                })
            }),
            ..FakeActions::default()
        });
        let mut a = app_albero();
        a.proc.act = act;
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
        assert!(f.sent.lock().unwrap().is_empty());
        let (t, ok) = a.proc.status_text().unwrap();
        assert!(!ok && t.contains("has exited, no signal"), "{t}");
    }

    #[test]
    fn nodo_intermedio_riusato_salta_il_ramo() {
        let (f, act) = fake(FakeActions {
            live: Some(|pid| {
                Some(match pid {
                    12 => 9,
                    60 => 1,
                    _ => 0,
                })
            }),
            ..FakeActions::default()
        });
        let mut a = app_albero();
        a.proc.act = act;
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
        let pids: Vec<u32> = f.sent.lock().unwrap().iter().map(|x| x.0).collect();
        assert_eq!(pids, [10, 11, 60]);
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
        let (f, act) = fake(FakeActions::default());
        a.proc.act = act;
        select_pid(&mut a, 10);
        a.on_key(key(KeyCode::F(9)));
        a.on_key(key(KeyCode::Char(' ')));
        a.on_key(key(KeyCode::Enter));
        assert!(a.dialog.as_ref().unwrap().armed);
        assert!(f.sent.lock().unwrap().is_empty());
        a.on_key(key(KeyCode::Enter));
        assert!(a.dialog.is_none());
        assert_eq!(f.sent.lock().unwrap().len(), 64);
    }

    fn remoto_app(uid: Option<u32>, target: &str) -> (App, Arc<FakeActions>) {
        let mut r = rows();
        for p in &mut r {
            p.uid = 1000;
        }
        r[3].uid = 0; // 12 di un altro utente
        let mut a = App::new((120, 40), "h".into());
        a.set_processes(r);
        a.remote = true;
        a.remote_target = target.into();
        a.remote_uid = uid;
        a.remote_generation = 5;
        let (f, act) = fake(FakeActions::default());
        a.proc.act = act;
        (a, f)
    }

    #[test]
    fn remoto_nomina_la_macchina_e_conta_gli_utenti_del_server() {
        // uid noto e uguale: solo il 12 è di altri, ma con 1 non basta a chiedere... lo chiede (foreign > 0)
        let (mut a, f) = remoto_app(Some(1000), "alex@server");
        select_pid(&mut a, 10);
        a.on_key(key(KeyCode::F(9)));
        a.on_key(key(KeyCode::Char(' ')));
        let d = a.dialog.clone().unwrap();
        assert!(d.lines().iter().any(|l| l == "On: alex@server"));
        assert_eq!(d.foreign, 1);
        assert!(d.needs_confirm());
        a.on_key(key(KeyCode::Enter));
        assert!(f.sent.lock().unwrap().is_empty());
        assert!(a.dialog.as_ref().unwrap().armed);
        a.on_key(key(KeyCode::Enter));
        assert_eq!(f.sent.lock().unwrap().len(), 4);
        // uid diverso o ignoto, target senza utente: tutti di altri, doppia conferma
        for (uid, target) in [(Some(0), "alex@server"), (None, "server")] {
            let (mut a, f) = remoto_app(uid, target);
            select_pid(&mut a, 10);
            a.on_key(key(KeyCode::F(9)));
            a.on_key(key(KeyCode::Char(' ')));
            let d = a.dialog.clone().unwrap();
            assert!(d.foreign >= 3, "{uid:?}");
            assert!(d.needs_confirm());
            a.on_key(key(KeyCode::Enter));
            assert!(f.sent.lock().unwrap().is_empty());
        }
    }

    #[test]
    fn invio_rifiutato_dopo_cambio_di_connessione() {
        let (mut a, f) = remoto_app(Some(1000), "alex@server");
        select_pid(&mut a, 11);
        a.on_key(key(KeyCode::F(9)));
        a.remote_generation = 6; // riconnessione mentre il dialogo è aperto
        a.on_key(key(KeyCode::Enter));
        assert!(a.dialog.is_none());
        assert!(f.sent.lock().unwrap().is_empty());
        let (t, ok) = a.proc.status_text().unwrap();
        assert!(!ok && t.contains("connection changed"), "{t}");
    }

    #[test]
    fn app_senza_azioni_non_tocca_quelle_locali() {
        // default: NoActions; in remoto un processo "vivo" qui non viene mai segnalato
        let mut child = std::process::Command::new("sleep")
            .arg("5")
            .spawn()
            .unwrap();
        let mut r = rows();
        r.push(proc(child.id(), 0, "sleep"));
        let mut a = App::new((120, 40), "h".into());
        a.set_processes(r);
        a.remote = true;
        select_pid(&mut a, child.id());
        a.on_key(key(KeyCode::F(9)));
        a.on_key(key(KeyCode::Enter));
        a.on_key(key(KeyCode::Enter));
        assert!(child.try_wait().unwrap().is_none());
        change_nice(&mut a, 1);
        assert!(!a.proc.status_text().unwrap().1);
        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn esito_incerto_mostrato_come_tale() {
        let (mut a, _) = remoto_app(Some(1000), "alex@server");
        let (_, act) = fake(FakeActions {
            sig_err: Some("connection lost: outcome unknown (retry after checking)".into()),
            ..FakeActions::default()
        });
        a.proc.act = act;
        select_pid(&mut a, 11);
        a.on_key(key(KeyCode::F(9)));
        a.on_key(key(KeyCode::Enter));
        let (t, ok) = a.proc.status_text().unwrap();
        assert!(!ok && t.contains("connection lost: outcome unknown"), "{t}");
        assert!(!t.contains("errors"));
    }

    #[test]
    fn nice_verifica_l_avvio_e_spiega_il_permesso() {
        let mut r = rows();
        r[2].start_ticks = 55;
        r[2].nice = 3;
        let mut a = App::new((120, 40), "h".into());
        a.set_processes(r);
        let (f, act) = fake(FakeActions::default());
        a.proc.act = act;
        select_pid(&mut a, 11);
        change_nice(&mut a, 1);
        assert_eq!(*f.nices.lock().unwrap(), [(11, 55, 4)]);
        assert!(a.proc.status_text().unwrap().1);
        // senza permessi, abbassare dà un testo chiaro (anche in remoto)
        let (_, act) = fake(FakeActions {
            nice_err: Some(crate::source::actions::ERR_PERM.into()),
            ..FakeActions::default()
        });
        a.proc.act = act;
        a.remote = true;
        change_nice(&mut a, -1);
        let (t, ok) = a.proc.status_text().unwrap();
        assert!(
            !ok && t.contains("root is needed to lower the nice value"),
            "{t}"
        );
        // con root sul server il testo grezzo non viene nascosto
        a.remote_root = true;
        change_nice(&mut a, -1);
        assert!(a.proc.status_text().unwrap().0.contains("EPERM"));
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
        let act = LocalActions::new("/proc".into());
        let start = act.live_starts(&[pid])[0].unwrap();
        act.signal(&[(pid, start)], SIGNALS[0].num)[0]
            .clone()
            .unwrap();
        let st = child.wait().unwrap();
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(st.signal(), Some(15));
        // processo ormai inesistente (già atteso): errore leggibile, niente panic
        let e = act.signal(&[(pid, start)], SIGNALS[0].num)[0]
            .clone()
            .unwrap_err();
        assert!(e.contains("ESRCH"), "{e}");
    }

    #[test]
    fn nice_su_un_figlio() {
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("sleep");
        let pid = child.id();
        let act = LocalActions::new("/proc".into());
        let start = act.live_starts(&[pid])[0].unwrap();
        assert!(act.set_nice(pid, start, 5).is_ok());
        let n = rustix::process::getpriority_process(crate::source::actions::to_pid(pid)).unwrap();
        assert_eq!(n, 5);
        // abbassare senza root non riesce (se si è root il test non ha senso)
        if rustix::process::getuid().as_raw() != 0 {
            assert!(act.set_nice(pid, start, 4).is_err());
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
