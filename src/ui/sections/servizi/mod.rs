// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Servizi: sezione 5 (unità systemd, dettaglio con registro, avvio, utenti e sessioni).
//! Qui lo stato e i tasti; il disegno è in `draw`. Tutto l'accesso a systemd passa dal
//! thread di lavoro (`collect::services::worker`): l'interfaccia non attende mai.

mod draw;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::Color;

use crate::collect::process::Process;
use crate::collect::services::dbus::DbusBackend;
use crate::collect::services::remote::{REMOTE_ACTION_MSG, REMOTE_USER_MSG};
use crate::collect::services::worker::{Client, Request, Response};
use crate::collect::services::{
    Action, ActionError, Backend, BootTimestamps, DENIED_TEXT, JournalLine, Scope, SessionInfo,
    UnitDetail, UnitInfo, UnitRow, systemd_present,
};
use crate::source::{CommandRunner, Source};
use crate::ui::theme;
use crate::ui::tz::Tz;
use crate::ui::widgets::proctable::follow;

pub use draw::draw;

/// Ogni quanto si rilegge l'elenco; più spesso finché non c'è una prima misura della CPU.
const UNITS_EVERY: Duration = Duration::from_secs(2);
const UNITS_EVERY_FIRST: Duration = Duration::from_millis(600);
const JOURNAL_EVERY: Duration = Duration::from_secs(5);
const SESSIONS_EVERY: Duration = Duration::from_secs(10);
const STATUS_FOR: Duration = Duration::from_secs(6);
/// Selezione ferma da tanto prima di chiedere dettaglio e registro (scorrendo non si chiede nulla).
const SETTLE: Duration = Duration::from_millis(200);
/// Oltre questo tempo una richiesta senza risposta fa dire che systemd non risponde.
const STALLED_AFTER: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    /// Non ancora controllato (il thread parte alla prima visita della sezione).
    Pending,
    Available,
    NoSystemd,
}

/// Filtri a "chip" in alto all'elenco.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chip {
    Tutti,
    Attivi,
    Falliti,
    Abilitati,
    Timer,
    Socket,
}

impl Chip {
    pub const ALL: [Chip; 6] = [
        Chip::Tutti,
        Chip::Attivi,
        Chip::Falliti,
        Chip::Abilitati,
        Chip::Timer,
        Chip::Socket,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Chip::Tutti => "All",
            Chip::Attivi => "Active",
            Chip::Falliti => "Failed",
            Chip::Abilitati => "Enabled",
            Chip::Timer => "Timers",
            Chip::Socket => "Sockets",
        }
    }

    pub fn matches(self, u: &UnitInfo) -> bool {
        match self {
            Chip::Tutti => true,
            Chip::Attivi => u.active_state == "active",
            Chip::Falliti => u.active_state == "failed",
            Chip::Abilitati => u.unit_file_state.starts_with("enabled"),
            Chip::Timer => u.name.ends_with(".timer"),
            Chip::Socket => u.name.ends_with(".socket"),
        }
    }

    pub fn next(self) -> Chip {
        Chip::ALL[(Chip::ALL.iter().position(|&c| c == self).unwrap_or(0) + 1) % Chip::ALL.len()]
    }
}

/// Conteggi per chip (nell'ordine di `Chip::ALL`), su tutte le unità.
pub fn chip_counts(units: &[UnitRow]) -> [usize; 6] {
    let mut out = [0; 6];
    for (i, c) in Chip::ALL.iter().enumerate() {
        out[i] = units.iter().filter(|u| c.matches(&u.info)).count();
    }
    out
}

/// Simbolo, etichetta e colore dello stato di un'unità.
pub fn state_look(u: &UnitInfo) -> (&'static str, &'static str, Color) {
    let green = theme::rgb(theme::CPU[0]);
    let yellow = theme::rgb(theme::CPU[1]);
    match u.active_state.as_str() {
        "failed" => ("✖", "failed", theme::RED),
        "active" if u.sub_state == "waiting" && u.name.ends_with(".timer") => {
            ("◷", "waiting", yellow)
        }
        "active" | "reloading" => ("●", "active", green),
        "activating" => ("◔", "starting", yellow),
        "deactivating" => ("◔", "stopping", yellow),
        _ => ("○", "inactive", theme::DIM),
    }
}

/// Testo della colonna "Avvio" (UnitFileState).
pub fn start_label(state: &str) -> &str {
    match state {
        "enabled" => "enabled",
        "enabled-runtime" => "enabled (rt)",
        "disabled" => "disabled",
        "static" => "static",
        "masked" | "masked-runtime" => "masked",
        "indirect" => "indirect",
        "generated" => "generated",
        "transient" => "transient",
        "alias" => "alias",
        "linked" | "linked-runtime" => "linked",
        "" => "—",
        other => other,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirm {
    pub unit: String,
    pub action: Action,
    /// 0 = Annulla, 1 = Conferma.
    pub button: usize,
}

#[derive(Debug, Clone)]
pub struct Status {
    pub text: String,
    pub ok: bool,
    pub at: Instant,
}

/// Sorgente, esecutore di comandi e core logici della macchina remota.
type RemoteCtx = (Arc<dyn Source>, Arc<dyn CommandRunner>, usize);

pub struct Servizi {
    run_dir: PathBuf,
    cgroup_root: PathBuf,
    backend: Option<Arc<dyn Backend>>,
    client: Option<Client>,
    pub avail: Availability,
    pub scope: Scope,
    /// `None` finché non arriva la prima risposta ("loading…").
    pub units: Option<Result<Vec<UnitRow>, String>>,
    pub chip: Chip,
    pub search: String,
    pub searching: bool,
    pub selected: usize,
    pub selected_name: Option<String>,
    pub offset: usize,
    /// Righe visibili nella tabella (aggiornato dal disegno).
    pub page: usize,
    pub detail: Option<(String, Result<Box<UnitDetail>, String>)>,
    pub journal: Option<(String, Result<Vec<JournalLine>, String>)>,
    pub boot: Option<Result<BootTimestamps, String>>,
    pub blame: Option<Result<Vec<(String, f64)>, String>>,
    pub sessions: Option<Result<Vec<SessionInfo>, String>>,
    pub confirm: Option<Confirm>,
    pub status: Option<Status>,
    /// Registro a tutto schermo (`j`).
    pub log_full: bool,
    // Pianificazione delle richieste.
    last_units: Option<Instant>,
    units_pending: bool,
    units_seen: u32,
    requested: Option<String>,
    /// Azione inviata e senza esito (polkit può attendere la password).
    action_pending: bool,
    /// Unità selezionata e da quando è ferma.
    sel_seen: Option<(String, Instant)>,
    last_detail: Option<Instant>,
    detail_pending: Option<String>,
    last_journal: Option<Instant>,
    journal_pending: Option<String>,
    last_sessions: Option<Instant>,
    boot_asked: bool,
    blame_asked: bool,
    /// Macchina remota: azioni disattivate e niente controllo di systemd sul disco locale.
    remote: Option<RemoteCtx>,
}

impl Default for Servizi {
    fn default() -> Self {
        Self::new()
    }
}

impl Servizi {
    /// Stato iniziale: il thread di lavoro parte solo alla prima visita della sezione.
    pub fn new() -> Self {
        Self::build(None, PathBuf::from("/run"), PathBuf::from("/sys/fs/cgroup"))
    }

    /// Con un backend dato (test) e radici configurabili.
    pub fn with_backend(backend: Arc<dyn Backend>, run_dir: PathBuf, cgroup_root: PathBuf) -> Self {
        Self::build(Some(backend), run_dir, cgroup_root)
    }

    /// Servizi di una macchina remota: backend a comandi (via `Roots.run`), letture via `Roots.src`.
    pub fn remote(src: Arc<dyn Source>, run: Arc<dyn CommandRunner>, ncores: usize) -> Self {
        let mut s = Self::build(None, PathBuf::from("/run"), PathBuf::from("/sys/fs/cgroup"));
        s.remote = Some((src, run, ncores));
        s
    }

    fn build(backend: Option<Arc<dyn Backend>>, run_dir: PathBuf, cgroup_root: PathBuf) -> Self {
        Servizi {
            run_dir,
            cgroup_root,
            backend,
            client: None,
            avail: Availability::Pending,
            scope: Scope::System,
            units: None,
            chip: Chip::Tutti,
            search: String::new(),
            searching: false,
            selected: 0,
            selected_name: None,
            offset: 0,
            page: 10,
            detail: None,
            journal: None,
            boot: None,
            blame: None,
            sessions: None,
            confirm: None,
            status: None,
            log_full: false,
            last_units: None,
            units_pending: false,
            units_seen: 0,
            requested: None,
            action_pending: false,
            sel_seen: None,
            last_detail: None,
            detail_pending: None,
            last_journal: None,
            journal_pending: None,
            last_sessions: None,
            boot_asked: false,
            blame_asked: false,
            remote: None,
        }
    }

    /// `true` se ci sono richieste in corso o dati da attendere (serve un ciclo eventi più rapido).
    pub fn busy(&self) -> bool {
        self.avail == Availability::Available
            && !self.stalled()
            && (self.units.is_none()
                || self.units_pending
                || self.detail_pending.is_some()
                || self.journal_pending.is_some()
                || self.confirm.is_some())
    }

    /// Una richiesta è in corso da troppo: systemd (o il bus) non risponde.
    pub fn stalled(&self) -> bool {
        let old = |pending: bool, at: Option<Instant>| {
            pending && at.is_some_and(|t| t.elapsed() > STALLED_AFTER)
        };
        old(self.units_pending, self.last_units)
            || old(self.detail_pending.is_some(), self.last_detail)
            || old(self.journal_pending.is_some(), self.last_journal)
    }

    fn send(&self, r: Request) {
        if let Some(c) = &self.client {
            c.send(r);
        }
    }

    fn start(&mut self) {
        if let Some((src, run, ncores)) = self.remote.clone() {
            if !crate::collect::services::systemd_present_in(&*src, &self.run_dir) {
                self.avail = Availability::NoSystemd;
                return;
            }
            let backend = self
                .backend
                .clone()
                .unwrap_or_else(|| crate::collect::services::backend_for(&src, &run));
            self.client = Some(Client::spawn_with(
                backend,
                self.cgroup_root.clone(),
                src,
                ncores,
            ));
            self.avail = Availability::Available;
            return;
        }
        if !systemd_present(&self.run_dir) {
            self.avail = Availability::NoSystemd;
            return;
        }
        let backend = self
            .backend
            .clone()
            .unwrap_or_else(|| Arc::new(DbusBackend::new()));
        self.client = Some(Client::spawn(backend, self.cgroup_root.clone()));
        self.avail = Availability::Available;
    }

    /// Indici (in `units`) delle unità mostrate: filtro a chip + ricerca.
    pub fn visible(&self) -> Vec<usize> {
        let Some(Ok(units)) = &self.units else {
            return Vec::new();
        };
        let q = self.search.to_lowercase();
        units
            .iter()
            .enumerate()
            .filter(|(_, u)| {
                self.chip.matches(&u.info)
                    && (q.is_empty()
                        || u.info.name.to_lowercase().contains(&q)
                        || u.info.description.to_lowercase().contains(&q))
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn selected_row(&self) -> Option<&UnitRow> {
        let Some(Ok(units)) = &self.units else {
            return None;
        };
        let name = self.selected_name.as_deref()?;
        units.iter().find(|u| u.info.name == name)
    }

    /// Riposiziona la selezione sul nome scelto (o la limita all'elenco) e fa scorrere la vista.
    fn relocate(&mut self) {
        let vis = self.visible();
        let names: Vec<&str> = match &self.units {
            Some(Ok(u)) => vis.iter().map(|&i| u[i].info.name.as_str()).collect(),
            _ => Vec::new(),
        };
        let pos = self
            .selected_name
            .as_deref()
            .and_then(|n| names.iter().position(|&x| x == n));
        match pos {
            Some(i) => self.selected = i,
            None => {
                self.selected = self.selected.min(names.len().saturating_sub(1));
                self.selected_name = names.get(self.selected).map(|s| s.to_string());
            }
        }
        self.offset = follow(self.selected, self.offset, self.page, names.len());
    }

    pub fn select_index(&mut self, i: usize) {
        let vis = self.visible();
        if vis.is_empty() {
            self.selected = 0;
            self.selected_name = None;
            return;
        }
        self.selected = i.min(vis.len() - 1);
        if let Some(Ok(u)) = &self.units {
            self.selected_name = Some(u[vis[self.selected]].info.name.clone());
        }
        self.offset = follow(self.selected, self.offset, self.page, vis.len());
    }

    pub fn move_selection(&mut self, delta: isize) {
        self.select_index((self.selected as isize + delta).max(0) as usize);
    }

    fn set_status(&mut self, text: String, ok: bool) {
        self.status = Some(Status {
            text,
            ok,
            at: Instant::now(),
        });
    }

    fn switch_scope(&mut self) {
        self.scope = match self.scope {
            Scope::System => Scope::User,
            Scope::User => Scope::System,
        };
        self.units = None;
        self.units_pending = false;
        self.last_units = None;
        self.units_seen = 0;
        self.selected = 0;
        self.selected_name = None;
        self.offset = 0;
        self.requested = None;
        self.sel_seen = None;
        self.detail = None;
        self.journal = None;
        self.detail_pending = None;
        self.journal_pending = None;
    }

    fn ask_confirm(&mut self, action: Action) {
        if self.action_pending {
            return;
        }
        if let Some(u) = self.selected_row() {
            let action = match (action, u.info.unit_file_state.starts_with("enabled")) {
                // `e` alterna abilita/disabilita secondo lo stato attuale.
                (Action::Enable, true) => Action::Disable,
                (a, _) => a,
            };
            self.confirm = Some(Confirm {
                unit: u.info.name.clone(),
                action,
                button: 0,
            });
        }
    }

    /// Gestisce un tasto nella sezione; `true` se consumato.
    pub fn on_key(&mut self, key: KeyEvent) -> bool {
        if let Some(c) = &mut self.confirm {
            match key.code {
                KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::BackTab => {
                    c.button = 1 - c.button.min(1);
                }
                KeyCode::Enter => {
                    let c = c.clone();
                    self.confirm = None;
                    if c.button == 1 {
                        self.set_status(format!("{} {}…", c.action.verb(), c.unit), true);
                        self.action_pending = true;
                        self.send(Request::Action(self.scope, c.unit, c.action));
                    }
                }
                KeyCode::Esc | KeyCode::Char('n') => self.confirm = None,
                _ => {}
            }
            return true;
        }
        if self.searching {
            match key.code {
                KeyCode::Esc => {
                    self.search.clear();
                    self.searching = false;
                }
                KeyCode::Enter => self.searching = false,
                KeyCode::Backspace => {
                    self.search.pop();
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.search.push(c);
                }
                KeyCode::Up => self.move_selection(-1),
                KeyCode::Down => self.move_selection(1),
                _ => return false,
            }
            if matches!(
                key.code,
                KeyCode::Esc | KeyCode::Backspace | KeyCode::Char(_)
            ) {
                self.relocate();
            }
            return true;
        }
        let usable = self.avail == Availability::Available;
        match key.code {
            KeyCode::Char('/') => self.searching = usable,
            KeyCode::Esc if self.log_full => self.log_full = false,
            _ if !usable => return false,
            KeyCode::Char('f') => {
                self.chip = self.chip.next();
                self.relocate();
            }
            KeyCode::Char('u') if self.remote.is_some() => {
                self.set_status(REMOTE_USER_MSG.into(), false);
            }
            KeyCode::Char('u') => self.switch_scope(),
            KeyCode::Char('s' | 't' | 'r' | 'e') if self.remote.is_some() => {
                self.set_status(REMOTE_ACTION_MSG.into(), false);
            }
            KeyCode::Char('s') => self.ask_confirm(Action::Start),
            KeyCode::Char('t') => self.ask_confirm(Action::Stop),
            KeyCode::Char('r') => self.ask_confirm(Action::Restart),
            KeyCode::Char('e') => self.ask_confirm(Action::Enable),
            KeyCode::Char('j') => self.log_full = !self.log_full,
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::PageUp => self.move_selection(-(self.page.max(1) as isize)),
            KeyCode::PageDown => self.move_selection(self.page.max(1) as isize),
            KeyCode::Home => self.select_index(0),
            KeyCode::End => self.select_index(usize::MAX),
            _ => return false,
        }
        true
    }

    /// Rotella del mouse.
    pub fn scroll(&mut self, delta: isize) {
        if self.avail == Availability::Available && self.confirm.is_none() {
            self.move_selection(delta);
        }
    }

    /// Avvia il thread se serve, raccoglie le risposte e pianifica le richieste.
    /// `true` se qualcosa è cambiato (serve ridisegnare). Non blocca mai.
    pub fn poll(&mut self) -> bool {
        if self.avail == Availability::Pending {
            self.start();
        }
        if self.avail != Availability::Available {
            return false;
        }
        let mut dirty = false;
        while let Some(r) = self.client.as_ref().and_then(|c| c.try_recv()) {
            dirty |= self.on_response(r);
        }
        if self
            .status
            .as_ref()
            .is_some_and(|s| s.at.elapsed() > STATUS_FOR)
        {
            self.status = None;
            dirty = true;
        }
        if self.action_pending {
            let waiting = "waiting for authorization…";
            if self.status.as_ref().is_none_or(|s| s.text != waiting) {
                self.set_status(waiting.to_string(), true);
                dirty = true;
            }
            // Lo stato non scade finché l'azione è in corso.
            if let Some(s) = &mut self.status {
                s.at = Instant::now();
            }
        }
        if self.stalled() && self.status.as_ref().is_none_or(|s| s.ok) {
            self.set_status("systemd is not responding".to_string(), false);
            dirty = true;
        }
        self.schedule();
        dirty
    }

    fn on_response(&mut self, r: Response) -> bool {
        match r {
            Response::Units(scope, res) => {
                if scope != self.scope {
                    return false;
                }
                self.units_pending = false;
                self.units_seen = self.units_seen.saturating_add(1);
                self.units = Some(res);
                self.relocate();
                true
            }
            Response::Detail(scope, name, res) => {
                if self.detail_pending.as_deref() == Some(name.as_str()) {
                    self.detail_pending = None;
                }
                if scope != self.scope || self.selected_name.as_deref() != Some(name.as_str()) {
                    return false;
                }
                self.detail = Some((name, res));
                true
            }
            Response::Journal(scope, name, res) => {
                if self.journal_pending.as_deref() == Some(name.as_str()) {
                    self.journal_pending = None;
                }
                if scope != self.scope || self.selected_name.as_deref() != Some(name.as_str()) {
                    return false;
                }
                self.journal = Some((name, res));
                true
            }
            Response::Action(_, unit, action, res) => {
                self.action_pending = false;
                match res {
                    Ok(()) => self.set_status(format!("{}: {unit}", action.done()), true),
                    Err(ActionError::Denied) => self.set_status(DENIED_TEXT.to_string(), false),
                    Err(ActionError::Failed(e)) => {
                        let first = e.lines().next().unwrap_or("").to_string();
                        self.set_status(format!("Error: {first}"), false);
                    }
                }
                // Rilegge subito l'elenco e il dettaglio.
                self.last_units = None;
                self.last_detail = None;
                self.last_journal = None;
                true
            }
            Response::Boot(res) => {
                self.boot = Some(res);
                true
            }
            Response::Sessions(res) => {
                self.sessions = Some(res);
                true
            }
            Response::Blame(res) => {
                self.blame = Some(res);
                true
            }
        }
    }

    fn due(last: Option<Instant>, every: Duration) -> bool {
        last.is_none_or(|t| t.elapsed() >= every)
    }

    fn schedule(&mut self) {
        let every = if self.units_seen < 2 {
            UNITS_EVERY_FIRST
        } else {
            UNITS_EVERY
        };
        if !self.units_pending && Self::due(self.last_units, every) {
            self.units_pending = true;
            self.last_units = Some(Instant::now());
            self.send(Request::Units(self.scope));
        }
        if let Some(name) = self.selected_name.clone() {
            let now = Instant::now();
            // Solo quando la selezione è ferma da SETTLE si chiede qualcosa per la nuova unità.
            match &self.sel_seen {
                Some((n, _)) if *n == name => {}
                _ => self.sel_seen = Some((name.clone(), now)),
            }
            let settled = self
                .sel_seen
                .as_ref()
                .is_some_and(|(_, t)| t.elapsed() >= SETTLE);
            if settled && self.requested.as_deref() != Some(name.as_str()) {
                self.requested = Some(name.clone());
                self.last_detail = None;
                self.last_journal = None;
            }
            // Al massimo un dettaglio e un registro in corso; se la risposta è di un'unità non
            // più selezionata, `on_response` libera lo slot e qui parte subito quella attuale.
            if settled && self.detail_pending.is_none() && Self::due(self.last_detail, UNITS_EVERY)
            {
                self.detail_pending = Some(name.clone());
                self.last_detail = Some(now);
                self.send(Request::Detail(self.scope, name.clone()));
            }
            if settled
                && self.journal_pending.is_none()
                && Self::due(self.last_journal, JOURNAL_EVERY)
            {
                self.journal_pending = Some(name.clone());
                self.last_journal = Some(now);
                self.send(Request::Journal(self.scope, name));
            }
        }
        if !self.boot_asked {
            self.boot_asked = true;
            self.send(Request::Boot);
        }
        if !self.blame_asked {
            self.blame_asked = true;
            self.send(Request::Blame);
        }
        if Self::due(self.last_sessions, SESSIONS_EVERY) {
            self.last_sessions = Some(Instant::now());
            self.send(Request::Sessions);
        }
    }
}

// ----- utenti e sessioni -----

/// Una riga del pannello "utenti e sessioni".
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UserRow {
    pub user: String,
    pub session: String,
    pub from: String,
    pub since_usec: Option<u64>,
    /// Aggregati per utente; `None` sulle sessioni successive alla prima dello stesso utente.
    pub agg: Option<UserAgg>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct UserAgg {
    pub procs: usize,
    pub cpu: f64,
    pub mem: u64,
}

/// Processi, CPU% e memoria sommati per uid dall'elenco processi già raccolto.
pub fn aggregate_users(procs: &[Process]) -> std::collections::HashMap<u32, (String, UserAgg)> {
    let mut m: std::collections::HashMap<u32, (String, UserAgg)> = Default::default();
    for p in procs {
        let e = m.entry(p.uid).or_insert_with(|| {
            (
                if p.user.is_empty() {
                    p.uid.to_string()
                } else {
                    p.user.clone()
                },
                UserAgg::default(),
            )
        });
        e.1.procs += 1;
        e.1.cpu += p.cpu_percent;
        e.1.mem += p.rss;
    }
    m
}

/// Righe del pannello: una per sessione login1 (aggregato sulla prima sessione di ogni utente),
/// poi gli utenti senza sessione per memoria decrescente. Senza login1 solo gli aggregati.
pub fn user_rows(sessions: Option<&[SessionInfo]>, procs: &[Process]) -> Vec<UserRow> {
    let agg = aggregate_users(procs);
    let mut rows = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for s in sessions.unwrap_or(&[]) {
        let first = seen.insert(s.uid);
        let place = [s.seat.as_str(), s.tty.as_str(), s.kind.as_str()]
            .iter()
            .filter(|x| !x.is_empty() && **x != "n/a")
            .copied()
            .collect::<Vec<_>>()
            .join(" ");
        let from = if !s.remote_host.is_empty() {
            s.remote_host.clone()
        } else if !s.desktop.is_empty() {
            s.desktop.clone()
        } else {
            "—".to_string()
        };
        rows.push(UserRow {
            user: s.user.clone(),
            session: if place.is_empty() {
                s.id.clone()
            } else {
                place
            },
            from,
            since_usec: (s.since_usec > 0).then_some(s.since_usec),
            agg: first.then(|| agg.get(&s.uid).map(|a| a.1).unwrap_or_default()),
        });
    }
    let mut rest: Vec<(&u32, &(String, UserAgg))> =
        agg.iter().filter(|(uid, _)| !seen.contains(uid)).collect();
    rest.sort_by(|a, b| b.1.1.mem.cmp(&a.1.1.mem).then(a.0.cmp(b.0)));
    for (_, (name, a)) in rest {
        rows.push(UserRow {
            user: name.clone(),
            session: "— system".to_string(),
            from: "—".to_string(),
            since_usec: None,
            agg: Some(*a),
        });
    }
    rows
}

// ----- formattazione di tempi (ora locale col fuso dell'App) -----

/// Data civile (anno, mese, giorno) da giorni dal 1970-01-01 (algoritmo di H. Hinnant).
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// "today 07:38:12", "yesterday 23:10:00" oppure "2026-10-21 07:38:12" (con `secs`) in ora locale.
pub fn when_text(unix: i64, now_unix: i64, tz: &Tz, secs: bool) -> String {
    let local = unix + tz.offset_at(unix);
    let day = local.div_euclid(86_400);
    let today = (now_unix + tz.offset_at(now_unix)).div_euclid(86_400);
    let t = local.rem_euclid(86_400);
    let clock = if secs {
        format!("{:02}:{:02}:{:02}", t / 3600, t % 3600 / 60, t % 60)
    } else {
        format!("{:02}:{:02}", t / 3600, t % 3600 / 60)
    };
    match today - day {
        0 => format!("today {clock}"),
        1 => format!("yesterday {clock}"),
        _ => {
            let (y, m, d) = civil_from_days(day);
            format!("{y}-{m:02}-{d:02} {clock}")
        }
    }
}

/// Durata breve: "42s", "5m 03s", "1h 25m", "3g 2h".
pub fn duration_text(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m {:02}s", secs / 60, secs % 60)
    } else if secs < 86_400 {
        format!("{}h {:02}m", secs / 3600, secs % 3600 / 60)
    } else {
        format!("{}d {}h", secs / 86_400, secs % 86_400 / 3600)
    }
}

/// Tempo di CPU totale: "312ms", "4.2s", "1m 42s", "2h 05m".
pub fn cpu_time_text(nsec: u64) -> String {
    let s = nsec as f64 / 1e9;
    if s < 1.0 {
        format!("{:.0}ms", s * 1000.0)
    } else if s < 60.0 {
        format!("{s:.1}s")
    } else if s < 3600.0 {
        format!("{}m {:02}s", (s / 60.0) as u64, (s % 60.0) as u64)
    } else {
        format!(
            "{}h {:02}m",
            (s / 3600.0) as u64,
            (s % 3600.0 / 60.0) as u64
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(name: &str, active: &str, sub: &str, file: &str) -> UnitInfo {
        UnitInfo {
            name: name.into(),
            description: format!("desc di {name}"),
            active_state: active.into(),
            sub_state: sub.into(),
            unit_file_state: file.into(),
            ..Default::default()
        }
    }

    fn rows() -> Vec<UnitRow> {
        [
            info("a.service", "active", "running", "enabled"),
            info("b.service", "failed", "failed", "enabled"),
            info("c.service", "inactive", "dead", "static"),
            info("d.timer", "active", "waiting", "enabled"),
            info("e.socket", "active", "listening", "disabled"),
        ]
        .into_iter()
        .map(|info| UnitRow {
            info,
            ..Default::default()
        })
        .collect()
    }

    #[test]
    fn stati_simboli_e_colori() {
        let r = rows();
        let green = theme::rgb(theme::CPU[0]);
        let yellow = theme::rgb(theme::CPU[1]);
        assert_eq!(state_look(&r[0].info), ("●", "active", green));
        assert_eq!(state_look(&r[1].info), ("✖", "failed", theme::RED));
        assert_eq!(state_look(&r[2].info), ("○", "inactive", theme::DIM));
        assert_eq!(state_look(&r[3].info), ("◷", "waiting", yellow));
        // Un servizio "waiting" non è un timer.
        let u = info("x.service", "active", "waiting", "");
        assert_eq!(state_look(&u).0, "●");
        assert_eq!(start_label("enabled"), "enabled");
        assert_eq!(start_label("disabled"), "disabled");
        assert_eq!(start_label("static"), "static");
        assert_eq!(start_label(""), "—");
    }

    #[test]
    fn filtri_e_conteggi() {
        let r = rows();
        // All, Active, Failed, Enabled, Timers, Sockets
        assert_eq!(chip_counts(&r), [5, 3, 1, 3, 1, 1]);
        assert_eq!(Chip::Socket.next(), Chip::Tutti);
        assert_eq!(Chip::Tutti.next(), Chip::Attivi);
    }

    #[test]
    fn ricerca_e_selezione_seguono_il_nome() {
        let mut s = Servizi::new();
        s.units = Some(Ok(rows()));
        s.chip = Chip::Falliti;
        s.relocate();
        assert_eq!(s.visible().len(), 1);
        assert_eq!(s.selected_name.as_deref(), Some("b.service"));
        s.chip = Chip::Tutti;
        s.search = "TIMER".into();
        s.relocate();
        assert_eq!(s.selected_name.as_deref(), Some("d.timer"));
        s.search.clear();
        s.relocate();
        // La selezione resta su d.timer anche se l'elenco torna intero.
        assert_eq!(s.selected, 3);
        s.select_index(99);
        assert_eq!(s.selected_name.as_deref(), Some("e.socket"));
    }

    #[test]
    fn utenti_e_sessioni() {
        let procs = vec![
            Process {
                uid: 1000,
                user: "alex".into(),
                cpu_percent: 2.0,
                rss: 100,
                ..Process::default()
            },
            Process {
                uid: 1000,
                user: "alex".into(),
                cpu_percent: 1.0,
                rss: 50,
                ..Process::default()
            },
            Process {
                uid: 0,
                user: "root".into(),
                cpu_percent: 0.5,
                rss: 10,
                ..Process::default()
            },
        ];
        let s = |id: &str, kind: &str| SessionInfo {
            id: id.into(),
            user: "alex".into(),
            uid: 1000,
            seat: "seat0".into(),
            tty: "tty2".into(),
            kind: kind.into(),
            ..Default::default()
        };
        let sessions = vec![s("1", "wayland"), s("2", "tty")];
        let rows = user_rows(Some(&sessions), &procs);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].session, "seat0 tty2 wayland");
        assert_eq!(
            rows[0].agg,
            Some(UserAgg {
                procs: 2,
                cpu: 3.0,
                mem: 150
            })
        );
        assert_eq!(rows[1].agg, None);
        assert_eq!(rows[2].user, "root");
        assert_eq!(rows[2].session, "— system");
        // Senza login1: solo gli aggregati, per memoria decrescente.
        let rows = user_rows(None, &procs);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].user, "alex");
    }

    #[test]
    fn formati_di_tempo() {
        let tz = Tz::Utc;
        // 2023-11-14 22:13:20 UTC
        let t = 1_700_000_000;
        assert_eq!(when_text(t, t + 60, &tz, true), "today 22:13:20");
        assert_eq!(when_text(t, t + 86_400, &tz, false), "yesterday 22:13");
        assert_eq!(when_text(t, t + 5 * 86_400, &tz, false), "2023-11-14 22:13");
        assert_eq!(duration_text(42), "42s");
        assert_eq!(duration_text(5 * 60 + 3), "5m 03s");
        assert_eq!(duration_text(3600 + 25 * 60), "1h 25m");
        assert_eq!(cpu_time_text(102_000_000_000), "1m 42s");
        assert_eq!(cpu_time_text(312_000_000), "312ms");
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_675), (2023, 11, 14));
    }
}
