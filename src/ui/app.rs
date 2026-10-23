// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Stato dell'applicazione e gestione di tasti e mouse (funzioni pure, testabili).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use std::collections::{HashMap, HashSet};

use crate::collect::System;
use crate::collect::net::NetInfo;
use crate::collect::process::{Process, SortKey, sort_processes, tree_order};

use super::history::History;
use super::sections::processi::{self, ProcState};
use super::sections::retedischi::{self, RdState};
use super::sections::servizi::Servizi;
use super::tz::Tz;
use super::widgets::ColumnDef;
use super::widgets::proctable::{column_at, follow};

pub const HISTORY_LEN: usize = 600;
/// Aggiornamenti remoti lenti consecutivi prima di allargare l'intervallo.
const SLOW_STREAK: u8 = 3;

pub const INTERVALS_MS: [u64; 5] = [250, 500, 1000, 2000, 5000];
/// Colonne su cui cicla F6 / '<' '>'.
pub const SORT_CYCLE: [SortKey; 7] = [
    SortKey::Pid,
    SortKey::Name,
    SortKey::User,
    SortKey::Mem,
    SortKey::Cpu,
    SortKey::Gpu,
    SortKey::Vram,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Panoramica,
    Gpu,
    Processi,
    ReteDischi,
    Servizi,
}

impl Section {
    pub const ALL: [Section; 5] = [
        Section::Panoramica,
        Section::Gpu,
        Section::Processi,
        Section::ReteDischi,
        Section::Servizi,
    ];

    pub fn index(self) -> usize {
        Section::ALL.iter().position(|&s| s == self).unwrap_or(0)
    }

    pub fn title(self) -> &'static str {
        match self {
            Section::Panoramica => "Overview",
            Section::Gpu => "GPU",
            Section::Processi => "Processes",
            Section::ReteDischi => "Network & Disks",
            Section::Servizi => "Services",
        }
    }

    /// Passo del piano in cui la sezione sarà realizzata.
    pub fn next(self) -> Section {
        Section::ALL[(self.index() + 1) % Section::ALL.len()]
    }

    pub fn prev(self) -> Section {
        Section::ALL[(self.index() + Section::ALL.len() - 1) % Section::ALL.len()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    Grande,
    Compatto,
}

impl LayoutMode {
    /// Compatto se il terminale è più stretto di 100 o più basso di 30.
    pub fn for_size(w: u16, h: u16) -> Self {
        if w < 100 || h < 30 {
            LayoutMode::Compatto
        } else {
            LayoutMode::Grande
        }
    }
}

/// Dialogo "Invia segnale" (definito nella sezione Processi, comune a Panoramica).
pub use super::sections::processi::SignalDialog;

/// Storici di un dispositivo GPU.
#[derive(Debug, Clone)]
pub struct GpuHist {
    /// Uso e VRAM come frazioni 0..1.
    pub usage: History,
    pub vram: History,
    /// Temperatura in °C e potenza in W (valori grezzi).
    pub temp: History,
    pub power: History,
}

impl GpuHist {
    fn new() -> Self {
        GpuHist {
            usage: History::new(HISTORY_LEN),
            vram: History::new(HISTORY_LEN),
            temp: History::raw(HISTORY_LEN),
            power: History::raw(HISTORY_LEN),
        }
    }
}

/// Zone cliccabili calcolate dall'ultimo disegno.
#[derive(Debug, Clone, Default)]
pub struct Hits {
    pub tabs: Vec<(u16, u16)>,
    /// Area della tabella (riga 0 = intestazione).
    pub table: Option<Rect>,
    pub cols: Vec<ColumnDef>,
}

pub struct App {
    pub section: Section,
    pub layout: LayoutMode,
    pub size: (u16, u16),
    pub interval_ms: u64,
    pub should_quit: bool,
    pub hostname: String,
    /// Macchina remota (via ssh): le azioni passano dal server.
    pub remote: bool,
    /// Destinazione remota come l'ha scritta l'utente (vuota in locale) e se i comandi girano da root.
    pub remote_target: String,
    pub remote_root: bool,
    /// Uid sul server.
    pub remote_uid: Option<u32>,
    /// Aggiornamenti remoti lenti consecutivi (l'intervallo si allarga solo se persistono).
    slow_streak: u8,
    pub dialog: Option<SignalDialog>,
    /// Processi nell'ordine mostrato.
    pub rows: Vec<Process>,
    pub selected: usize,
    pub selected_pid: Option<u32>,
    pub offset: usize,
    /// Righe visibili nella tabella (aggiornato dal disegno).
    pub page: usize,
    pub sort: SortKey,
    pub descending: bool,
    pub cpu_hist: History,
    pub mem_hist: History,
    /// Storico di ricezione/invio (byte/s) per interfaccia.
    pub net_hist: HashMap<String, (History, History)>,
    /// Storico dell'attività (lettura + scrittura, byte/s) per disco; chiave "" = totale.
    pub disk_hist: HashMap<String, History>,
    pub net_iface: Option<String>,
    /// Storici per GPU (chiave: indirizzo PCI) e GPU mostrata (`d` per cambiare).
    pub gpu_hist: HashMap<String, GpuHist>,
    pub gpu_sel: Option<String>,
    gpu_pdevs: Vec<String>,
    /// Processi che usano la GPU (per la tabella della sezione GPU).
    pub gpu_pids: HashSet<u32>,
    net_cache: NetInfo,
    /// Albero invece di lista.
    /// Albero invece di lista, separato per sezione (Processi: albero; le altre: lista).
    pub tree_overview: bool,
    pub tree_proc: bool,
    pub filter: String,
    /// Digitazione del filtro in corso.
    pub filtering: bool,
    pub help: bool,
    /// Larghezza della colonna sinistra della Panoramica, in % (dalla configurazione).
    pub left_pct: u16,
    /// Configurazione letta dal file (si scrive con `W`) e suo percorso.
    pub config: crate::config::Config,
    pub config_path: Option<std::path::PathBuf>,
    pub tz: Tz,
    /// Stato della sezione Servizi (il thread di lavoro parte alla prima visita).
    pub servizi: Servizi,
    /// Processi mostrati: indice in `rows` e prefisso dell'albero.
    pub view: Vec<(usize, String)>,
    pub hits: Hits,
    /// Stato della sezione Rete e dischi.
    pub rd: RdState,
    /// Stato della sezione Processi (schede, rami piegati, riga di stato).
    pub proc: ProcState,
}

impl App {
    pub fn new(size: (u16, u16), hostname: String) -> Self {
        App {
            section: Section::Panoramica,
            layout: LayoutMode::for_size(size.0, size.1),
            size,
            interval_ms: 1000,
            should_quit: false,
            hostname,
            remote: false,
            remote_target: String::new(),
            remote_root: false,
            remote_uid: None,
            slow_streak: 0,
            left_pct: 40,
            config: crate::config::Config::default(),
            config_path: None,
            dialog: None,
            rows: Vec::new(),
            selected: 0,
            selected_pid: None,
            offset: 0,
            page: 10,
            sort: SortKey::Cpu,
            descending: true,
            cpu_hist: History::new(HISTORY_LEN),
            mem_hist: History::new(HISTORY_LEN),
            net_hist: HashMap::new(),
            disk_hist: HashMap::new(),
            net_iface: None,
            gpu_hist: HashMap::new(),
            gpu_sel: None,
            gpu_pdevs: Vec::new(),
            gpu_pids: HashSet::new(),
            net_cache: NetInfo::default(),
            tree_overview: false,
            tree_proc: true,
            filter: String::new(),
            filtering: false,
            help: false,
            tz: Tz::Utc,
            servizi: Servizi::new(),
            view: Vec::new(),
            hits: Hits::default(),
            rd: RdState::default(),
            proc: ProcState::new(),
        }
    }

    pub fn resize(&mut self, w: u16, h: u16) {
        self.size = (w, h);
        self.layout = LayoutMode::for_size(w, h);
    }

    /// Dopo ogni `System::refresh`: storico e lista processi.
    pub fn update(&mut self, sys: &System) {
        self.cpu_hist.push(sys.cpu.total_usage / 100.0);
        let m = &sys.mem;
        self.mem_hist.push(if m.total > 0 {
            m.used as f64 / m.total as f64
        } else {
            0.0
        });
        for i in &sys.net.ifaces {
            let (rx, tx) = self
                .net_hist
                .entry(i.name.clone())
                .or_insert_with(|| (History::raw(HISTORY_LEN), History::raw(HISTORY_LEN)));
            rx.push(i.rx_rate);
            tx.push(i.tx_rate);
        }
        if let Some(st) = sys.remote_status() {
            self.remote_target = st.target;
            self.remote_root = st.root;
            self.remote_uid = st.uid;
        }
        self.gpu_pdevs = sys.gpu.devices.iter().map(|g| g.pdev.clone()).collect();
        self.gpu_pids = sys.gpu.procs.iter().map(|p| p.pid).collect();
        for g in &sys.gpu.devices {
            let h = self
                .gpu_hist
                .entry(g.pdev.clone())
                .or_insert_with(GpuHist::new);
            h.usage.push(g.usage.unwrap_or(0.0) / 100.0);
            h.vram.push(match (g.vram_used, g.vram_total) {
                (Some(u), Some(t)) if t > 0 => u as f64 / t as f64,
                _ => 0.0,
            });
            h.temp.push(g.temp.unwrap_or(0.0));
            h.power.push(g.power_w.unwrap_or(0.0));
        }
        self.gpu_hist
            .retain(|k, _| self.gpu_pdevs.iter().any(|p| p == k));
        self.net_cache = sys.net.clone();
        // Via gli storici di interfacce e dischi spariti (es. veth dei container).
        self.net_hist
            .retain(|k, _| sys.net.ifaces.iter().any(|i| &i.name == k));
        self.disk_hist
            .retain(|k, _| k.is_empty() || sys.disks.io.iter().any(|d| &d.name == k));
        let total = sys.disks.read_rate + sys.disks.write_rate;
        self.disk_hist
            .entry(String::new())
            .or_insert_with(|| History::raw(HISTORY_LEN))
            .push(total);
        for d in &sys.disks.io {
            self.disk_hist
                .entry(d.name.clone())
                .or_insert_with(|| History::raw(HISTORY_LEN))
                .push(d.read_rate + d.write_rate);
        }
        self.rd.update(sys, self.section == Section::ReteDischi);
        self.set_processes(sys.processes.clone());
        if self.section == Section::Processi {
            processi::update(self, sys);
        }
        self.poll_background();
    }

    /// Raccoglie le risposte del thread dei servizi; `true` se serve ridisegnare.
    pub fn poll_background(&mut self) -> bool {
        self.section == Section::Servizi && self.servizi.poll()
    }

    /// `true` se la sezione attiva aspetta dati dal thread di lavoro (ciclo eventi più rapido).
    pub fn background_active(&self) -> bool {
        self.section == Section::Servizi && self.servizi.busy()
    }

    /// Processo selezionato nella vista.
    pub fn selected_process(&self) -> Option<&Process> {
        self.view
            .get(self.selected)
            .and_then(|(i, _)| self.rows.get(*i))
    }

    /// `f` è il filtro già in minuscolo.
    fn matches_filter(p: &Process, f: &str) -> bool {
        if f.is_empty() {
            return true;
        }
        p.name.to_lowercase().contains(f)
            || p.cmdline.to_lowercase().contains(f)
            || p.user.to_lowercase().contains(f)
            || p.pid.to_string().contains(f)
    }

    /// Ricalcola l'elenco mostrato (albero e filtro) e riposiziona la selezione.
    pub(crate) fn rebuild_view(&mut self) {
        let f = self.filter.to_lowercase();
        let tree = self.tree();
        let order: Vec<(usize, String)> = if tree {
            tree_order(&self.rows)
        } else {
            (0..self.rows.len()).map(|i| (i, String::new())).collect()
        };
        let folded = if tree {
            processi::hidden_by_fold(&self.rows, &self.proc.collapsed)
        } else {
            HashSet::new()
        };
        self.view = order
            .into_iter()
            .filter(|(i, _)| {
                let p = &self.rows[*i];
                (!self.gpu_only() || self.gpu_pids.contains(&p.pid))
                    && Self::matches_filter(p, &f)
                    && !folded.contains(&p.pid)
            })
            .collect();
        self.relocate();
    }

    /// Albero attivo nella sezione corrente.
    pub fn tree(&self) -> bool {
        if self.section == Section::Processi {
            self.tree_proc
        } else {
            self.tree_overview
        }
    }

    /// Cambia sezione; nella sezione GPU la tabella mostra solo i processi che usano la GPU.
    pub fn set_section(&mut self, s: Section) {
        if self.section != s {
            let before = (self.gpu_only(), self.tree());
            self.section = s;
            if before != (self.gpu_only(), self.tree()) {
                self.rebuild_view();
            }
        }
    }

    fn gpu_only(&self) -> bool {
        self.section == Section::Gpu
    }

    /// GPU mostrata: quella scelta con `d` se esiste ancora, altrimenti la prima.
    pub fn selected_gpu<'a>(&self, sys: &'a System) -> Option<&'a crate::collect::gpu::Gpu> {
        let devs = &sys.gpu.devices;
        self.gpu_sel
            .as_deref()
            .and_then(|s| devs.iter().find(|g| g.pdev == s))
            .or_else(|| devs.first())
    }

    fn cycle_gpu(&mut self) {
        if self.gpu_pdevs.is_empty() {
            return;
        }
        let cur = self
            .gpu_sel
            .as_ref()
            .and_then(|s| self.gpu_pdevs.iter().position(|p| p == s))
            .unwrap_or(0);
        self.gpu_sel = Some(self.gpu_pdevs[(cur + 1) % self.gpu_pdevs.len()].clone());
    }

    pub(crate) fn cycle_net(&mut self, step: isize) {
        let cur = self
            .net_iface
            .as_deref()
            .or(self.net_cache.pick(None).map(|i| i.name.as_str()));
        self.net_iface = self.net_cache.cycle(cur, step);
    }

    /// Passa ai Processi con quel pid selezionato (per ora fissa solo la selezione).
    pub fn jump_to_pid(&mut self, pid: u32) {
        if !self.rows.iter().any(|p| p.pid == pid) {
            self.proc
                .set_status(format!("process {pid} has exited"), false);
            return;
        }
        self.set_section(Section::Processi);
        // Apre i rami piegati che lo nascondono.
        let ppid: HashMap<u32, u32> = self.rows.iter().map(|p| (p.pid, p.ppid)).collect();
        let mut cur = pid;
        for _ in 0..ppid.len() {
            match ppid.get(&cur) {
                Some(&up) if up != cur => {
                    self.proc.collapsed.remove(&up);
                    cur = up;
                }
                _ => break,
            }
        }
        self.selected_pid = Some(pid);
        self.rebuild_view();
        // Escluso dal filtro: lo si toglie.
        if !self.filter.is_empty() && self.selected_process().map(|p| p.pid) != Some(pid) {
            self.filter.clear();
            self.selected_pid = Some(pid);
            self.rebuild_view();
        }
    }

    fn filter_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => {
                self.filter.clear();
                self.filtering = false;
            }
            KeyCode::Enter => self.filtering = false,
            KeyCode::Backspace => {
                self.filter.pop();
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.filter.push(c);
            }
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            _ => return false,
        }
        if matches!(
            key.code,
            KeyCode::Esc | KeyCode::Backspace | KeyCode::Char(_)
        ) {
            self.rebuild_view();
        }
        true
    }

    /// Sostituisce l'elenco, riordina e mantiene la selezione sullo stesso pid.
    pub fn set_processes(&mut self, procs: Vec<Process>) {
        self.rows = procs;
        sort_processes(&mut self.rows, self.sort, self.descending);
        self.rebuild_view();
    }

    fn relocate(&mut self) {
        match self
            .selected_pid
            .and_then(|pid| self.view.iter().position(|(i, _)| self.rows[*i].pid == pid))
        {
            Some(i) => self.selected = i,
            // Nessuna scelta dell'utente (o processo sparito): si limita solo l'indice.
            None => {
                self.selected_pid = None;
                self.selected = self.selected.min(self.view.len().saturating_sub(1));
            }
        }
        self.follow();
    }

    pub fn follow(&mut self) {
        self.offset = follow(self.selected, self.offset, self.page, self.view.len());
    }

    pub fn select_index(&mut self, i: usize) {
        match self.view.len() {
            0 => {
                self.selected = 0;
                self.selected_pid = None;
            }
            n => {
                self.selected = i.min(n - 1);
                self.selected_pid = Some(self.rows[self.view[self.selected].0].pid);
            }
        }
        self.follow();
    }

    fn move_selection(&mut self, delta: isize) {
        let i = (self.selected as isize + delta).max(0) as usize;
        self.select_index(i);
    }

    /// Applica colonna e verso di ordinamento, riordina e mantiene la selezione.
    fn apply_sort(&mut self, key: SortKey, descending: bool) {
        self.sort = key;
        self.descending = descending;
        sort_processes(&mut self.rows, self.sort, self.descending);
        self.rebuild_view();
    }

    fn default_descending(key: SortKey) -> bool {
        matches!(
            key,
            SortKey::Cpu | SortKey::Mem | SortKey::Gpu | SortKey::Vram
        )
    }

    fn set_sort(&mut self, key: SortKey) {
        let desc = if key == self.sort {
            !self.descending
        } else {
            Self::default_descending(key)
        };
        self.apply_sort(key, desc);
    }

    fn cycle_sort(&mut self, step: isize) {
        let n = SORT_CYCLE.len() as isize;
        let cur = SORT_CYCLE.iter().position(|&k| k == self.sort).unwrap_or(0) as isize;
        let key = SORT_CYCLE[((cur + step).rem_euclid(n)) as usize];
        self.apply_sort(key, Self::default_descending(key));
    }

    /// Tasto `W`: scrive nel file di configurazione intervallo e sezione correnti.
    fn save_config(&mut self) {
        let Some(path) = self.config_path.clone() else {
            self.proc
                .set_status("no configuration path (HOME is not set)".into(), false);
            return;
        };
        let mut cfg = self.config.clone();
        cfg.interval_ms = self.interval_ms;
        cfg.section = self.section.index() + 1;
        match crate::config::save(&path, &cfg) {
            Ok(()) => {
                self.config = cfg;
                self.proc
                    .set_status(format!("configuration saved to {}", path.display()), true);
            }
            Err(e) => self.proc.set_status(
                format!("configuration not saved ({}): {e}", path.display()),
                false,
            ),
        }
    }

    fn change_interval(&mut self, up: bool) {
        let i = INTERVALS_MS
            .iter()
            .position(|&v| v >= self.interval_ms)
            .unwrap_or(INTERVALS_MS.len() - 1);
        let j = if up {
            (i + 1).min(INTERVALS_MS.len() - 1)
        } else {
            i.saturating_sub(1)
        };
        self.interval_ms = INTERVALS_MS[j];
    }

    /// In remoto un aggiornamento più lungo dell'intervallo allarga l'intervallo al passo
    /// successivo e lo dice nella riga di stato. `true` se l'intervallo è cambiato.
    pub fn adapt_interval(&mut self, took: std::time::Duration) -> bool {
        if !self.remote {
            return false;
        }
        if took.as_millis() <= u128::from(self.interval_ms) {
            self.slow_streak = 0;
            return false;
        }
        // Un solo campione lento (es. dopo una riconnessione) non basta: ne servono tre di fila.
        self.slow_streak += 1;
        if self.slow_streak < SLOW_STREAK {
            return false;
        }
        self.slow_streak = 0;
        let Some(next) = INTERVALS_MS.iter().copied().find(|&v| v > self.interval_ms) else {
            return false;
        };
        self.interval_ms = next;
        self.proc.set_status(
            format!(
                "remote update takes {} ms: interval widened to {}",
                took.as_millis(),
                super::view::interval_text(next)
            ),
            true,
        );
        true
    }

    /// Gestisce un tasto; `true` se lo stato è cambiato (serve ridisegnare).
    pub fn on_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C')) {
            self.should_quit = true;
            return true;
        }
        if self.help {
            if matches!(
                key.code,
                KeyCode::Esc | KeyCode::Enter | KeyCode::F(1) | KeyCode::Char('q')
            ) {
                self.help = false;
                return true;
            }
            return false;
        }
        if self.dialog.is_some() {
            return processi::dialog_key(self, key);
        }
        if self.section == Section::Servizi && self.servizi.on_key(key) {
            return true;
        }
        if self.filtering {
            return self.filter_key(key);
        }
        if self.section == Section::ReteDischi && retedischi::on_key(self, key) {
            return true;
        }
        if self.section == Section::Processi && processi::on_key(self, key) {
            return true;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::F(10) => self.should_quit = true,
            KeyCode::F(1) => self.help = true,
            KeyCode::Char('/') | KeyCode::F(3) | KeyCode::F(4) => self.filtering = true,
            KeyCode::F(5) => {
                if self.section == Section::Processi {
                    self.tree_proc = !self.tree_proc;
                } else {
                    self.tree_overview = !self.tree_overview;
                }
                self.rebuild_view();
            }
            KeyCode::Char('W') => self.save_config(),
            KeyCode::Char('n') => self.cycle_net(1),
            KeyCode::Char('N') => self.cycle_net(-1),
            KeyCode::Char(c @ '1'..='5') => {
                self.set_section(Section::ALL[(c as u8 - b'1') as usize]);
            }
            KeyCode::Tab => self.set_section(self.section.next()),
            KeyCode::BackTab => self.set_section(self.section.prev()),
            KeyCode::Char('d') => self.cycle_gpu(),
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::PageUp => self.move_selection(-(self.page.max(1) as isize)),
            KeyCode::PageDown => self.move_selection(self.page.max(1) as isize),
            KeyCode::Home => self.select_index(0),
            KeyCode::End => self.select_index(usize::MAX),
            KeyCode::F(6) | KeyCode::Char('>') => self.cycle_sort(1),
            KeyCode::Char('<') => self.cycle_sort(-1),
            KeyCode::Char('i') => {
                self.apply_sort(self.sort, !self.descending);
            }
            KeyCode::Char('+') => self.change_interval(true),
            KeyCode::Char('-') => self.change_interval(false),
            KeyCode::F(9) => match self.section {
                Section::Panoramica | Section::Gpu | Section::Processi => {
                    processi::open_dialog(self)
                }
                Section::ReteDischi => {
                    if let Some(pid) = self.rd.selected_pid() {
                        processi::open_dialog_pid(self, pid);
                    }
                }
                Section::Servizi => {}
            },
            _ => return false,
        }
        true
    }

    /// Gestisce il mouse; `true` se lo stato è cambiato (i movimenti non ridisegnano).
    pub fn on_mouse(&mut self, m: MouseEvent) -> bool {
        if self.dialog.is_some() || self.help {
            return false;
        }
        if self.section == Section::Servizi {
            match m.kind {
                MouseEventKind::ScrollUp => self.servizi.scroll(-3),
                MouseEventKind::ScrollDown => self.servizi.scroll(3),
                MouseEventKind::Down(MouseButton::Left) => self.click(m.column, m.row),
                _ => return false,
            }
            return true;
        }
        if self.section == Section::ReteDischi
            && matches!(
                m.kind,
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
            )
        {
            retedischi::wheel(
                self,
                if m.kind == MouseEventKind::ScrollUp {
                    -3
                } else {
                    3
                },
            );
            return true;
        }
        if self.section == Section::Processi && processi::on_mouse(self, m) {
            return true;
        }
        match m.kind {
            MouseEventKind::ScrollUp => self.move_selection(-3),
            MouseEventKind::ScrollDown => self.move_selection(3),
            MouseEventKind::Down(MouseButton::Left) => self.click(m.column, m.row),
            _ => return false,
        }
        true
    }

    fn click(&mut self, x: u16, y: u16) {
        if y == 0 {
            if let Some(i) = self
                .hits
                .tabs
                .iter()
                .position(|&(tx, tw)| x >= tx && x < tx + tw)
            {
                self.set_section(Section::ALL[i.min(Section::ALL.len() - 1)]);
            }
            return;
        }
        if self.section == Section::ReteDischi {
            retedischi::click(self, x, y);
            return;
        }
        let Some(t) = self.hits.table else { return };
        if x < t.x || x >= t.x + t.width || y < t.y || y >= t.y + t.height {
            return;
        }
        if y == t.y {
            let col = column_at(&self.hits.cols, t.width, x - t.x);
            if let Some(key) = col.and_then(|c| self.hits.cols.get(c)).and_then(|c| c.sort) {
                self.set_sort(key);
            }
        } else {
            let i = self.offset + (y - t.y - 1) as usize;
            // Clic sotto l'ultima riga: ignorato.
            if i < self.view.len() {
                self.select_index(i);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn app() -> App {
        let mut a = App::new((120, 40), "h".into());
        let procs = (1..=30)
            .map(|i| Process {
                pid: i,
                name: format!("p{i}"),
                user: "u".into(),
                cpu_percent: i as f64,
                rss: (100 - i) as u64,
                ..Process::default()
            })
            .collect();
        a.page = 10;
        a.set_processes(procs);
        a
    }

    #[test]
    fn azioni_in_remoto_passano_dal_server() {
        use crate::ui::sections::processi::FakeActions;
        // F9, k e F7/F8 funzionano anche in remoto, tramite le azioni della macchina
        for (sec, code) in [
            (Section::Processi, KeyCode::F(9)),
            (Section::Processi, KeyCode::Char('k')),
            (Section::Panoramica, KeyCode::F(9)),
        ] {
            let mut a = app();
            a.remote = true;
            a.remote_target = "alex@server".into();
            a.proc.act = std::sync::Arc::new(FakeActions::default());
            a.set_section(sec);
            a.on_key(key(code));
            let d = a.dialog.as_ref().expect("dialogo aperto");
            assert!(d.lines().iter().any(|l| l == "On: alex@server"), "{code:?}");
        }
        // 'k' mentre si digita il filtro resta testo
        let mut a = app();
        a.remote = true;
        a.on_key(key(KeyCode::Char('/')));
        a.on_key(key(KeyCode::Char('k')));
        assert_eq!(a.filter, "k");
        assert!(a.proc.status_text().is_none());
    }

    #[test]
    fn intervallo_si_allarga_in_remoto() {
        use std::time::Duration;
        let mut a = app();
        let ms = Duration::from_millis;
        // tre lenti di fila: allarga all'ultimo
        let slow = |a: &mut App, t: u64| {
            assert!(!a.adapt_interval(ms(t)));
            assert!(!a.adapt_interval(ms(t)));
            a.adapt_interval(ms(t))
        };
        // locale: mai
        assert!(!a.adapt_interval(ms(3000)));
        a.remote = true;
        a.interval_ms = 250;
        assert!(!a.adapt_interval(ms(250)));
        // un campione lento isolato non conta
        assert!(!a.adapt_interval(ms(900)));
        assert!(!a.adapt_interval(ms(900)));
        assert!(!a.adapt_interval(ms(100)));
        assert_eq!(a.interval_ms, 250);
        assert!(slow(&mut a, 300));
        assert_eq!(a.interval_ms, 500);
        assert!(a.proc.status_text().unwrap().0.contains("widened to 500ms"));
        assert!(slow(&mut a, 700));
        assert!(slow(&mut a, 1500));
        assert_eq!(a.interval_ms, 2000);
        assert!(slow(&mut a, 2500));
        assert_eq!(a.interval_ms, 5000);
        for _ in 0..5 {
            assert!(!a.adapt_interval(ms(9000)));
        }
        assert_eq!(a.interval_ms, 5000);
    }

    #[test]
    fn layout_automatico() {
        assert_eq!(LayoutMode::for_size(100, 30), LayoutMode::Grande);
        assert_eq!(LayoutMode::for_size(99, 40), LayoutMode::Compatto);
        assert_eq!(LayoutMode::for_size(200, 29), LayoutMode::Compatto);
        let mut a = App::new((182, 52), "h".into());
        assert_eq!(a.layout, LayoutMode::Grande);
        a.resize(80, 24);
        assert_eq!(a.layout, LayoutMode::Compatto);
        a.resize(120, 40);
        assert_eq!(a.layout, LayoutMode::Grande);
    }

    #[test]
    fn sezioni_e_uscita() {
        let mut a = app();
        a.on_key(key(KeyCode::Char('3')));
        assert_eq!(a.section, Section::Processi);
        a.on_key(key(KeyCode::Tab));
        assert_eq!(a.section, Section::ReteDischi);
        a.on_key(key(KeyCode::BackTab));
        a.on_key(key(KeyCode::BackTab));
        a.on_key(key(KeyCode::BackTab));
        assert_eq!(a.section, Section::Panoramica);
        a.on_key(key(KeyCode::BackTab));
        assert_eq!(a.section, Section::Servizi);
        a.on_key(key(KeyCode::Char('9')));
        assert!(!a.should_quit);
        for k in [KeyCode::Char('q'), KeyCode::F(10)] {
            let mut b = app();
            b.on_key(key(k));
            assert!(b.should_quit);
        }
        let mut c = app();
        c.on_key(KeyEvent {
            modifiers: KeyModifiers::CONTROL,
            ..key(KeyCode::Char('c'))
        });
        assert!(c.should_quit);
    }

    #[test]
    fn selezione_e_scorrimento() {
        let mut a = app();
        // ordinati per CPU decrescente: il primo è pid 30
        // nessuna azione dell'utente: nessun pid fissato
        assert_eq!(a.selected_pid, None);
        assert_eq!(a.selected, 0);
        a.on_key(key(KeyCode::Down));
        assert_eq!(a.selected, 1);
        assert_eq!(a.selected_pid, Some(29));
        a.on_key(key(KeyCode::Up));
        a.on_key(key(KeyCode::Up));
        assert_eq!(a.selected, 0);
        a.on_key(key(KeyCode::End));
        assert_eq!(a.selected, 29);
        assert_eq!(a.offset, 20);
        a.on_key(key(KeyCode::Home));
        assert_eq!((a.selected, a.offset), (0, 0));
        a.on_key(key(KeyCode::PageDown));
        assert_eq!(a.selected, 10);
        assert_eq!(a.offset, 1);
        a.on_key(key(KeyCode::PageUp));
        assert_eq!((a.selected, a.offset), (0, 0));
    }

    #[test]
    fn senza_input_la_selezione_resta_in_cima() {
        let mut a = app();
        // due aggiornamenti con ordine diverso e nessun tasto: selected resta 0
        let mut procs = a.rows.clone();
        procs[0].cpu_percent = 0.0;
        a.set_processes(procs);
        let mut procs = a.rows.clone();
        procs.reverse();
        a.set_processes(procs);
        assert_eq!((a.selected, a.selected_pid), (0, None));
        // l'utente sceglie: da allora si segue il pid
        a.on_key(key(KeyCode::Down));
        assert!(a.selected_pid.is_some());
    }

    #[test]
    fn storici_ripuliti() {
        use crate::collect::{Roots, System};
        let mut a = app();
        a.net_hist
            .insert("veth9".into(), (History::raw(4), History::raw(4)));
        a.disk_hist.insert("sdz".into(), History::raw(4));
        let nessuna = std::env::temp_dir().join("ngtop-nessuna");
        let sys = System::new(Roots::with_dirs(nessuna.clone(), nessuna.clone(), nessuna));
        a.update(&sys);
        assert!(a.net_hist.is_empty());
        assert_eq!(a.disk_hist.len(), 1);
        assert!(a.disk_hist.contains_key(""));
    }

    #[test]
    fn selezione_segue_il_pid_dopo_aggiornamento() {
        let mut a = app();
        a.select_index(3);
        let pid = a.selected_pid;
        let mut procs = a.rows.clone();
        procs[0].cpu_percent = 0.0; // il primo scende in fondo
        a.set_processes(procs);
        assert_eq!(a.selected_pid, pid);
        assert_eq!(a.rows[a.selected].pid, pid.unwrap());
        // se il processo sparisce, la selezione resta nei limiti
        a.set_processes(Vec::new());
        assert_eq!((a.selected, a.selected_pid), (0, None));
    }

    #[test]
    fn ordinamento() {
        let mut a = app();
        assert_eq!(a.sort, SortKey::Cpu);
        a.on_key(key(KeyCode::F(6))); // dopo Cpu: Gpu, Vram, poi di nuovo Pid
        assert_eq!(a.sort, SortKey::Gpu);
        assert!(a.descending);
        a.on_key(key(KeyCode::F(6)));
        assert_eq!(a.sort, SortKey::Vram);
        a.on_key(key(KeyCode::F(6)));
        assert_eq!(a.sort, SortKey::Pid);
        assert!(!a.descending);
        assert_eq!(a.rows[0].pid, 1);
        a.on_key(key(KeyCode::Char('i')));
        assert!(a.descending);
        assert_eq!(a.rows[0].pid, 30);
        a.on_key(key(KeyCode::Char('<')));
        assert_eq!(a.sort, SortKey::Vram);
        a.on_key(key(KeyCode::Char('>')));
        a.on_key(key(KeyCode::Char('>')));
        assert_eq!(a.sort, SortKey::Name);
    }

    #[test]
    fn intervallo() {
        let mut a = app();
        a.on_key(key(KeyCode::Char('+')));
        assert_eq!(a.interval_ms, 2000);
        for _ in 0..5 {
            a.on_key(key(KeyCode::Char('+')));
        }
        assert_eq!(a.interval_ms, 5000);
        for _ in 0..9 {
            a.on_key(key(KeyCode::Char('-')));
        }
        assert_eq!(a.interval_ms, 250);
    }

    #[test]
    fn dialogo() {
        let mut a = app();
        // Invio reale sostituito da un finto: nei test non si segnalano processi veri.
        a.proc.act = std::sync::Arc::new(crate::ui::sections::processi::FakeActions::default());
        a.select_index(2);
        a.on_key(key(KeyCode::F(9)));
        let d = a.dialog.clone().unwrap();
        assert_eq!(d.pid, a.rows[2].pid);
        a.on_key(key(KeyCode::Char('q'))); // inghiottito dal dialogo
        assert!(!a.should_quit);
        a.on_key(key(KeyCode::Down));
        assert_eq!(a.dialog.as_ref().unwrap().sig, 1);
        a.on_key(key(KeyCode::Esc));
        assert!(a.dialog.is_none());
        a.on_key(key(KeyCode::F(9)));
        a.on_key(key(KeyCode::Enter));
        assert!(a.dialog.is_none());
        assert!(a.proc.status_text().is_some());
        // senza processi non si apre
        let mut b = App::new((80, 24), "h".into());
        b.on_key(key(KeyCode::F(9)));
        assert!(b.dialog.is_none());
    }

    #[test]
    fn filtro_albero_aiuto_rete() {
        let mut a = app();
        a.select_index(4);
        let pid = a.selected_pid;
        a.on_key(key(KeyCode::Char('/')));
        assert!(a.filtering);
        // mentre si digita, 'q' è testo e non esce
        a.on_key(key(KeyCode::Char('p')));
        a.on_key(key(KeyCode::Char('1')));
        assert_eq!(a.filter, "p1");
        assert_eq!(a.view.len(), 11); // p1, p10..p19
        a.on_key(key(KeyCode::Char('q')));
        assert!(!a.should_quit);
        assert_eq!(a.view.len(), 0);
        assert_eq!(a.selected_pid, None);
        a.on_key(key(KeyCode::Backspace));
        a.on_key(key(KeyCode::Enter));
        assert!(!a.filtering);
        assert_eq!(a.filter, "p1");
        // il filtro vale anche per utente e pid
        a.on_key(key(KeyCode::F(4)));
        a.on_key(key(KeyCode::Esc));
        assert_eq!(a.filter, "");
        assert!(!a.filtering);
        assert_eq!(a.view.len(), 30);
        // la selezione riprende un processo valido
        assert_eq!(a.selected, 0);
        let _ = pid;

        // albero: radici in ordine di pid
        a.on_key(key(KeyCode::F(5)));
        assert!(a.tree());
        assert_eq!(a.rows[a.view[0].0].pid, 1);
        a.on_key(key(KeyCode::F(5)));
        assert!(!a.tree());
        assert_eq!(a.rows[a.view[0].0].pid, 30);

        // aiuto: inghiotte i tasti, si chiude con Esc
        a.on_key(key(KeyCode::F(1)));
        assert!(a.help);
        assert!(!a.on_key(key(KeyCode::Char('x'))));
        assert!(!a.should_quit);
        a.on_key(key(KeyCode::Esc));
        assert!(!a.help);

        // cambio interfaccia
        use crate::collect::net::{Iface, NetInfo};
        let mk = |n: &str| Iface {
            name: n.into(),
            ..Iface::default()
        };
        a.net_cache = NetInfo::new(vec![mk("lo"), mk("eth0"), mk("wlan0")], Some("eth0".into()));
        a.on_key(key(KeyCode::Char('n')));
        assert_eq!(a.net_iface.as_deref(), Some("wlan0"));
        a.on_key(key(KeyCode::Char('n')));
        assert_eq!(a.net_iface.as_deref(), Some("eth0"));
        a.on_key(key(KeyCode::Char('N')));
        assert_eq!(a.net_iface.as_deref(), Some("wlan0"));
    }

    #[test]
    fn tasto_w_salva_la_configurazione() {
        let dir = std::env::temp_dir().join(format!("ngtop-w-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let f = dir.join("ngtop/config.toml");
        let mut a = app();
        // senza percorso: messaggio di errore, nessun panic
        a.on_key(key(KeyCode::Char('W')));
        assert_eq!(a.proc.status_text().map(|s| s.1), Some(false));
        a.config_path = Some(f.clone());
        a.on_key(key(KeyCode::Char('+')));
        a.set_section(Section::ReteDischi);
        a.on_key(key(KeyCode::Char('W')));
        let (msg, ok) = a.proc.status_text().unwrap();
        assert!(ok && msg.contains("saved"), "{msg}");
        let c = crate::config::load(&f, true).unwrap().0;
        assert_eq!((c.interval_ms, c.section), (2000, 4));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn mouse_schede_righe_intestazione_rotella() {
        let mut a = app();
        a.hits.tabs = vec![(12, 14), (26, 7)];
        a.hits.table = Some(Rect::new(0, 5, 40, 11));
        a.hits.cols = vec![
            ColumnDef::new("PID", 5, true, Some(SortKey::Pid)),
            ColumnDef::new("Name", 0, false, Some(SortKey::Name)),
        ];
        a.on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 30, 0));
        assert_eq!(a.section, Section::Gpu);
        a.on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 15, 0));
        assert_eq!(a.section, Section::Panoramica);
        a.on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 3, 8));
        assert_eq!(a.selected, 2);
        a.on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 2, 5));
        assert_eq!(a.sort, SortKey::Pid);
        a.on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 2, 5));
        assert!(a.descending);
        a.on_mouse(mouse(MouseEventKind::ScrollDown, 3, 8));
        assert_eq!(a.selected, 5);
        a.on_mouse(mouse(MouseEventKind::ScrollUp, 3, 8));
        assert_eq!(a.selected, 2);
        // clic sotto l'ultima riga (30 processi, tabella alta 11 ma offset alto): ignorato
        a.set_processes(a.rows.iter().take(3).cloned().collect());
        a.select_index(1);
        assert!(a.on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 3, 14)));
        assert_eq!(a.selected, 1);
        assert!(!a.on_mouse(mouse(MouseEventKind::Moved, 3, 8)));
        assert!(!a.on_key(key(KeyCode::Char('z'))));
        // clic fuori dalla tabella: ignorato
        a.on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 3, 40));
        assert_eq!(a.selected, 1);
    }
}
