// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Rete e dischi (sezione 4): interfaccia selezionata, elenco interfacce, connessioni con
//! processo, pannelli per disco e filesystem.

use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::text::Line;

use super::panoramica::{dim, fg, span};
use crate::collect::System;
use crate::collect::conn::{Conn, Proto};
use crate::collect::disk::{DiskIo, Filesystem};
use crate::collect::net::Iface;
use crate::ui::app::{App, HISTORY_LEN};
use crate::ui::fmt;
use crate::ui::history::History;
use crate::ui::theme;
use crate::ui::widgets::proctable::follow;
use crate::ui::widgets::{
    BrailleGraph, ColumnDef, Meter, Panel, ProcTable, clip, put, putn, width,
};

fn green() -> Color {
    theme::grad(theme::CPU, 0.0)
}

fn yellow() -> Color {
    theme::grad(theme::CPU, 0.5)
}

// ------------------------------------------------------------------ stato

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProtoFilter {
    #[default]
    Tutte,
    Tcp,
    Udp,
}

impl ProtoFilter {
    fn next(self) -> Self {
        match self {
            ProtoFilter::Tutte => ProtoFilter::Tcp,
            ProtoFilter::Tcp => ProtoFilter::Udp,
            ProtoFilter::Udp => ProtoFilter::Tutte,
        }
    }

    fn label(self) -> &'static str {
        match self {
            ProtoFilter::Tutte => "TCP+UDP",
            ProtoFilter::Tcp => "TCP",
            ProtoFilter::Udp => "UDP",
        }
    }

    fn accepts(self, p: Proto) -> bool {
        match self {
            ProtoFilter::Tutte => true,
            ProtoFilter::Tcp => p.is_tcp(),
            ProtoFilter::Udp => !p.is_tcp(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnSort {
    #[default]
    Stato,
    Proto,
    Locale,
    Remoto,
    Pid,
    Processo,
    Coda,
}

impl ConnSort {
    fn next(self) -> Self {
        match self {
            ConnSort::Stato => ConnSort::Proto,
            ConnSort::Proto => ConnSort::Locale,
            ConnSort::Locale => ConnSort::Remoto,
            ConnSort::Remoto => ConnSort::Pid,
            ConnSort::Pid => ConnSort::Processo,
            ConnSort::Processo => ConnSort::Coda,
            ConnSort::Coda => ConnSort::Stato,
        }
    }

    fn label(self) -> &'static str {
        match self {
            ConnSort::Stato => "state",
            ConnSort::Proto => "proto",
            ConnSort::Locale => "local",
            ConnSort::Remoto => "remote",
            ConnSort::Pid => "pid",
            ConnSort::Processo => "process",
            ConnSort::Coda => "queue",
        }
    }
}

/// Connessione mostrata, con il nome del processo già risolto.
#[derive(Debug, Clone)]
pub struct ConnRow {
    pub conn: Conn,
    pub name: String,
}

impl ConnRow {
    fn key(&self) -> String {
        format!(
            "{}|{}|{}",
            self.conn.proto.label(),
            self.conn.local,
            self.conn.remote
        )
    }
}

/// Zone cliccabili dell'ultimo disegno.
#[derive(Debug, Clone, Default)]
struct RdHits {
    /// Riga dell'elenco interfacce → nome.
    ifaces: Vec<(Rect, String)>,
    /// Area interna della tabella connessioni (riga 0 = intestazione).
    conns: Option<Rect>,
}

/// Stato della sezione Rete e dischi.
#[derive(Debug, Default)]
pub struct RdState {
    pub proto_filter: ProtoFilter,
    pub show_listen: Option<bool>,
    pub sort: ConnSort,
    all: Vec<Conn>,
    names: HashMap<u32, String>,
    pub rows: Vec<ConnRow>,
    pub sel: usize,
    sel_key: Option<String>,
    pub offset: usize,
    /// Righe visibili nella tabella delle connessioni (dal disegno).
    pub page: usize,
    /// Storico lettura/scrittura (byte/s) per disco.
    pub disk_rw: HashMap<String, (History, History)>,
    pub disk_off: usize,
    /// Pannelli disco visibili (dal disegno).
    disk_cap: usize,
    /// Picchi di velocità di sessione per interfaccia (ricezione, invio).
    pub peaks: HashMap<String, (f64, f64)>,
    /// Ultimi contatori visti e punto zero per i totali di sessione (`z`).
    last_bytes: HashMap<String, (u64, u64)>,
    base: HashMap<String, (u64, u64)>,
    hits: RdHits,
}

impl RdState {
    fn listen_shown(&self) -> bool {
        self.show_listen.unwrap_or(true)
    }

    /// Dopo ogni refresh: storici e picchi sempre; elenco connessioni solo a sezione visibile.
    pub fn update(&mut self, sys: &System, visible: bool) {
        for i in &sys.net.ifaces {
            let p = self.peaks.entry(i.name.clone()).or_default();
            p.0 = p.0.max(i.rx_rate);
            p.1 = p.1.max(i.tx_rate);
            self.last_bytes
                .insert(i.name.clone(), (i.rx_bytes, i.tx_bytes));
        }
        self.peaks
            .retain(|k, _| sys.net.ifaces.iter().any(|i| &i.name == k));
        self.last_bytes
            .retain(|k, _| sys.net.ifaces.iter().any(|i| &i.name == k));
        for d in &sys.disks.io {
            let (r, w) = self
                .disk_rw
                .entry(d.name.clone())
                .or_insert_with(|| (History::raw(HISTORY_LEN), History::raw(HISTORY_LEN)));
            r.push(d.read_rate);
            w.push(d.write_rate);
        }
        self.disk_rw
            .retain(|k, _| sys.disks.io.iter().any(|d| &d.name == k));
        if visible {
            self.all = sys.conns.conns.clone();
            self.names = sys
                .processes
                .iter()
                .filter(|p| self.all.iter().any(|c| c.pid == Some(p.pid)))
                .map(|p| (p.pid, p.name.clone()))
                .collect();
            self.rebuild();
        }
    }

    /// Azzera picchi e totali di sessione.
    fn reset_session(&mut self) {
        self.peaks.clear();
        self.base = self.last_bytes.clone();
    }

    /// Totali (ricevuti, inviati) dal punto zero.
    fn totals(&self, i: &Iface) -> (u64, u64) {
        let (br, bt) = self.base.get(&i.name).copied().unwrap_or((0, 0));
        (i.rx_bytes.saturating_sub(br), i.tx_bytes.saturating_sub(bt))
    }

    /// Filtra e ordina le connessioni e mantiene la selezione sulla stessa connessione.
    fn rebuild(&mut self) {
        let mut rows: Vec<ConnRow> = self
            .all
            .iter()
            .filter(|c| {
                self.proto_filter.accepts(c.proto) && (self.listen_shown() || !c.is_listen())
            })
            .map(|c| ConnRow {
                conn: c.clone(),
                name: c
                    .pid
                    .and_then(|p| self.names.get(&p).cloned())
                    .unwrap_or_default(),
            })
            .collect();
        let state_rank = |c: &Conn| match c.state {
            "ESTAB" => 0,
            "LISTEN" => 1,
            _ => 2,
        };
        match self.sort {
            ConnSort::Stato => rows.sort_by(|a, b| {
                (state_rank(&a.conn), &a.conn.local).cmp(&(state_rank(&b.conn), &b.conn.local))
            }),
            ConnSort::Proto => rows.sort_by(|a, b| {
                (a.conn.proto.label(), &a.conn.local).cmp(&(b.conn.proto.label(), &b.conn.local))
            }),
            ConnSort::Locale => rows.sort_by(|a, b| a.conn.local.cmp(&b.conn.local)),
            ConnSort::Remoto => rows.sort_by(|a, b| a.conn.remote.cmp(&b.conn.remote)),
            ConnSort::Pid => rows.sort_by_key(|r| r.conn.pid.unwrap_or(u32::MAX)),
            ConnSort::Processo => rows.sort_by(|a, b| a.name.cmp(&b.name)),
            ConnSort::Coda => rows.sort_by_key(|r| {
                std::cmp::Reverse(r.conn.rx_queue.saturating_add(r.conn.tx_queue))
            }),
        }
        self.rows = rows;
        match self
            .sel_key
            .as_ref()
            .and_then(|k| self.rows.iter().position(|r| &r.key() == k))
        {
            Some(i) => self.sel = i,
            None => {
                self.sel_key = None;
                self.sel = self.sel.min(self.rows.len().saturating_sub(1));
            }
        }
        self.follow();
    }

    fn follow(&mut self) {
        self.offset = follow(self.sel, self.offset, self.page, self.rows.len());
    }

    fn select(&mut self, i: usize) {
        if self.rows.is_empty() {
            self.sel = 0;
            self.sel_key = None;
            return;
        }
        self.sel = i.min(self.rows.len() - 1);
        self.sel_key = Some(self.rows[self.sel].key());
        self.follow();
    }

    fn move_sel(&mut self, delta: isize) {
        self.select((self.sel as isize + delta).max(0) as usize);
    }

    /// Pid della connessione selezionata, se noto.
    pub(crate) fn selected_pid(&self) -> Option<u32> {
        self.rows.get(self.sel).and_then(|r| r.conn.pid)
    }
}

// ------------------------------------------------------------------ input

/// Tasti della sezione; `true` se gestito.
pub fn on_key(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('i') => app.cycle_net(1),
        KeyCode::Char('I') => app.cycle_net(-1),
        KeyCode::Char('c') => {
            app.rd.proto_filter = app.rd.proto_filter.next();
            app.rd.rebuild();
        }
        KeyCode::Char('l') => {
            app.rd.show_listen = Some(!app.rd.listen_shown());
            app.rd.rebuild();
        }
        KeyCode::F(6) => {
            app.rd.sort = app.rd.sort.next();
            app.rd.rebuild();
        }
        KeyCode::Char('z') => app.rd.reset_session(),
        KeyCode::Enter => {
            if let Some(pid) = app.rd.selected_pid() {
                app.jump_to_pid(pid);
            }
        }
        KeyCode::Up => app.rd.move_sel(-1),
        KeyCode::Down => app.rd.move_sel(1),
        KeyCode::Home => app.rd.select(0),
        KeyCode::End => app.rd.select(usize::MAX),
        KeyCode::PageUp => app.rd.disk_off = app.rd.disk_off.saturating_sub(app.rd.disk_cap.max(1)),
        KeyCode::PageDown => app.rd.disk_off += app.rd.disk_cap.max(1),
        _ => return false,
    }
    true
}

/// Rotella: sposta la selezione delle connessioni.
pub fn wheel(app: &mut App, delta: isize) {
    app.rd.move_sel(delta);
}

/// Clic su una interfaccia (la seleziona) o su una connessione.
pub fn click(app: &mut App, x: u16, y: u16) {
    let inside = |r: &Rect| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height;
    if let Some((_, name)) = app.rd.hits.ifaces.iter().find(|(r, _)| inside(r)) {
        app.net_iface = Some(name.clone());
        return;
    }
    if let Some(t) = app.rd.hits.conns
        && inside(&t)
        && y > t.y
    {
        let i = app.rd.offset + (y - t.y - 1) as usize;
        if i < app.rd.rows.len() {
            app.rd.select(i);
        }
    }
}

// ------------------------------------------------------------------ layout

/// Altezze dei blocchi, in ordine di disegno, dato lo spazio. `items` = (minimo, desiderata).
/// I minimi si assegnano per priorità finché ci stanno; poi si cresce verso le desiderate.
pub(crate) fn allocate(total: u16, items: &[(u16, u16)]) -> Vec<u16> {
    // Priorità: rete, connessioni, interfacce, dischi, filesystem.
    const PRIO: [usize; 5] = [0, 2, 1, 3, 4];
    const GROW: [usize; 5] = [0, 1, 3, 4, 2];
    let mut h = vec![0u16; items.len()];
    let mut left = total;
    for &i in PRIO.iter().filter(|&&i| i < items.len()) {
        let min = items[i].0;
        if left >= min {
            h[i] = min;
            left -= min;
        } else {
            if h.iter().all(|&x| x == 0) {
                h[i] = left;
                left = 0;
            }
            break;
        }
    }
    for &i in GROW.iter().filter(|&&i| i < items.len()) {
        if h[i] > 0 {
            let add = items[i].1.saturating_sub(h[i]).min(left);
            h[i] += add;
            left -= add;
        }
    }
    if left > 0
        && let Some(i) = PRIO.iter().copied().find(|&i| i < items.len() && h[i] > 0)
    {
        h[i] += left;
    }
    h
}

pub fn draw(buf: &mut Buffer, area: Rect, app: &mut App, sys: &System) {
    app.rd.hits = RdHits::default();
    if area.width < 10 || area.height < 3 {
        return;
    }
    let disks_cols = disk_columns(area.width);
    let disk_rows_want = sys.disks.io.len().div_ceil(disks_cols).clamp(1, 2) as u16;
    let iface_want = (sys.net.ifaces.len() as u16 + 4).clamp(5, 10);
    let fs_want = (sys.disks.filesystems.len() as u16 + 3).clamp(5, 9);
    let items = [
        (5, 9),
        (5, iface_want),
        (6, u16::MAX / 2),
        (DISK_H, DISK_H * disk_rows_want),
        (5, fs_want),
    ];
    let hs = allocate(area.height, &items);
    let mut y = area.y;
    let mut rect = |h: u16| {
        let r = Rect::new(area.x, y, area.width, h);
        y += h;
        r
    };
    let [net_a, if_a, conn_a, disk_a, fs_a] = [
        rect(hs[0]),
        rect(hs[1]),
        rect(hs[2]),
        rect(hs[3]),
        rect(hs[4]),
    ];
    if net_a.height > 0 {
        draw_net(buf, net_a, app, sys);
    }
    if if_a.height > 0 {
        draw_ifaces(buf, if_a, app, sys);
    }
    if conn_a.height > 0 {
        draw_conns(buf, conn_a, app, sys);
    }
    if disk_a.height > 0 {
        draw_disks(buf, disk_a, app, sys);
    }
    if fs_a.height > 0 {
        draw_fs(buf, fs_a, sys);
    }
}

// ------------------------------------------------------------------ rete

fn link_text(i: &Iface) -> String {
    let mut parts = Vec::new();
    if let Some(s) = i.speed_mbps {
        let sp = if s >= 1000 && s.is_multiple_of(1000) {
            format!("{} Gbit/s", s / 1000)
        } else if s >= 1000 {
            format!("{:.1} Gbit/s", f64::from(s) / 1000.0)
        } else {
            format!("{s} Mbit/s")
        };
        parts.push(match &i.duplex {
            Some(d) => format!("{sp} {d}"),
            None => sp,
        });
    }
    if let Some(a) = &i.ipv4 {
        parts.push(a.clone());
    }
    if let Some(a) = &i.ipv6 {
        parts.push(a.clone());
    }
    if parts.is_empty() {
        "—".to_string()
    } else {
        parts.join(" · ")
    }
}

fn draw_net(buf: &mut Buffer, area: Rect, app: &App, sys: &System) {
    let color = theme::rgb(theme::NET[1]);
    let Some(ifc) = sys.net.pick(app.net_iface.as_deref()) else {
        let inner = Panel::new("interface", color).render(area, buf);
        putn(
            buf,
            inner.x + 1,
            inner.y,
            "no network interface",
            inner.width.saturating_sub(1) as usize,
            dim(),
        );
        return;
    };
    let title = format!("interface {}", ifc.name);
    let right = link_text(ifc);
    let empty = (History::raw(1), History::raw(1));
    let (rx, tx) = app.net_hist.get(&ifc.name).unwrap_or(&empty);
    let iw = area.width.saturating_sub(2) as usize;
    let scale = fmt::nice_max(rx.max_last(iw * 2).max(tx.max_last(iw * 2)), 100.0 * 1024.0);
    let bottom = format!("scale {} · i: switch", fmt::rate(scale));
    let inner = Panel::new(&title, color)
        .right(&right)
        .bottom(&bottom)
        .render(area, buf);
    if inner.height == 0 || inner.width < 10 {
        return;
    }
    let tw: u16 = if inner.width >= 50 { 26 } else { 0 };
    let gw = inner.width.saturating_sub(if tw > 0 { tw + 1 } else { 0 });
    let norm = |h: &History| -> Vec<f64> {
        h.last_n(gw as usize * 2)
            .into_iter()
            .map(|v| v / scale)
            .collect()
    };
    let rx_h = inner.height - inner.height / 2;
    let tx_h = inner.height / 2;
    BrailleGraph {
        values: &norm(rx),
        gradient: theme::NET,
        down: false,
    }
    .render(Rect::new(inner.x, inner.y, gw, rx_h), buf);
    BrailleGraph {
        values: &norm(tx),
        gradient: theme::UP,
        down: true,
    }
    .render(Rect::new(inner.x, inner.y + rx_h, gw, tx_h), buf);
    if tw == 0 {
        return;
    }
    let (rx_peak, tx_peak) = app.rd.peaks.get(&ifc.name).copied().unwrap_or_default();
    let (rx_tot, tx_tot) = app.rd.totals(ifc);
    let x = inner.x + gw + 1;
    let down_c = theme::grad(theme::NET, 0.7);
    let up_c = theme::grad(theme::UP, 0.6);
    let lines = if inner.height >= 6 {
        vec![
            (format!("▼ {}", fmt::rate(ifc.rx_rate)), fg(down_c)),
            (format!("  peak {}", fmt::rate(rx_peak)), dim()),
            (format!("  total {}", fmt::bytes(rx_tot)), dim()),
            (format!("▲ {}", fmt::rate(ifc.tx_rate)), fg(up_c)),
            (format!("  peak {}", fmt::rate(tx_peak)), dim()),
            (format!("  total {}", fmt::bytes(tx_tot)), dim()),
        ]
    } else {
        vec![
            (format!("▼ {}", fmt::rate(ifc.rx_rate)), fg(down_c)),
            (format!("▲ {}", fmt::rate(ifc.tx_rate)), fg(up_c)),
            (format!("  ▼ tot {}", fmt::bytes(rx_tot)), dim()),
            (format!("  ▲ tot {}", fmt::bytes(tx_tot)), dim()),
        ]
    };
    for (i, (t, st)) in lines.iter().enumerate().take(inner.height as usize) {
        putn(buf, x, inner.y + i as u16, t, tw as usize, *st);
    }
}

fn rate_cell(v: f64) -> Line<'static> {
    let st = if v >= 1.0 { fg(theme::FG) } else { dim() };
    Line::from(span(fmt::rate(v), st))
}

fn draw_ifaces(buf: &mut Buffer, area: Rect, app: &mut App, sys: &System) {
    let multi = sys.net.ifaces.len() > 1;
    let inner = Panel::new("interfaces", theme::rgb(theme::NET[1]))
        .right(if multi { "i / click: select" } else { "" })
        .render(area, buf);
    if inner.height < 2 || inner.width < 20 {
        return;
    }
    let dns_line = inner.height >= 4;
    let table_h = inner.height - u16::from(dns_line);
    let cols = [
        ColumnDef::new("Name", 16, false, None),
        ColumnDef::new("IPv4 address", 16, false, None),
        ColumnDef::new("▼", 10, true, None),
        ColumnDef::new("▲", 10, true, None),
        ColumnDef::new("State", 7, false, None),
    ];
    let cols: Vec<ColumnDef> = if inner.width >= 54 {
        cols.to_vec()
    } else {
        vec![cols[0].clone(), cols[2].clone(), cols[3].clone()]
    };
    let full = cols.len() == 5;
    let sel_name = sys
        .net
        .pick(app.net_iface.as_deref())
        .map(|i| i.name.clone());
    let ifs = &sys.net.ifaces;
    let sel_idx = sel_name
        .as_ref()
        .and_then(|n| ifs.iter().position(|i| &i.name == n))
        .unwrap_or(0);
    let visible = table_h.saturating_sub(1) as usize;
    let off = follow(sel_idx, 0, visible, ifs.len());
    let rows: Vec<Vec<Line<'static>>> = ifs
        .iter()
        .skip(off)
        .take(visible)
        .map(|i| {
            let state = match i.up {
                Some(true) => Line::from(span("● up", fg(green()))),
                Some(false) => Line::from(span("○ down", dim())),
                None => Line::from(span("—", dim())),
            };
            let name = Line::from(span(i.name.clone(), fg(theme::FG)));
            if full {
                vec![
                    name,
                    Line::from(span(
                        i.ipv4.clone().unwrap_or_else(|| "—".into()),
                        fg(theme::FG),
                    )),
                    rate_cell(i.rx_rate),
                    rate_cell(i.tx_rate),
                    state,
                ]
            } else {
                vec![name, rate_cell(i.rx_rate), rate_cell(i.tx_rate)]
            }
        })
        .collect();
    let sel = sel_idx.checked_sub(off).filter(|s| *s < rows.len());
    let table_area = Rect::new(inner.x, inner.y, inner.width, table_h);
    ProcTable {
        cols: &cols,
        rows: &rows,
        selected: sel,
        sort: None,
    }
    .render(table_area, buf);
    for (k, i) in ifs.iter().skip(off).take(visible).enumerate() {
        app.rd.hits.ifaces.push((
            Rect::new(inner.x, inner.y + 1 + k as u16, inner.width, 1),
            i.name.clone(),
        ));
    }
    if dns_line {
        let dns = if sys.net.dns.is_empty() {
            "—".to_string()
        } else {
            sys.net.dns.join(", ")
        };
        let gw = sys.net.gateway.clone().unwrap_or_else(|| "—".into());
        putn(
            buf,
            inner.x,
            inner.y + inner.height - 1,
            format!("DNS {dns} · gateway {gw}"),
            inner.width as usize,
            dim(),
        );
    }
}

// ------------------------------------------------------------------ connessioni

fn state_cell(s: &'static str) -> Line<'static> {
    let st = match s {
        "ESTAB" => fg(green()),
        "LISTEN" => fg(yellow()),
        _ => dim(),
    };
    Line::from(span(s, st))
}

fn conn_columns(w: u16) -> Vec<ColumnDef> {
    let coda = w >= 96;
    let fixed = 5 + 9 + 7 + if coda { 11 } else { 0 } + 12;
    let gaps = if coda { 6 } else { 5 };
    let addr = (w.saturating_sub(fixed + gaps) / 2).clamp(15, 41);
    let mut v = vec![
        ColumnDef::new("Proto", 5, false, None),
        ColumnDef::new("Local", addr, false, None),
        ColumnDef::new("Remote", addr, false, None),
        ColumnDef::new("State", 9, false, None),
        ColumnDef::new("PID", 7, true, None),
        ColumnDef::new("Process", 0, false, None),
    ];
    if coda {
        v.push(ColumnDef::new("Queue rx/tx", 11, true, None));
    }
    v
}

fn draw_conns(buf: &mut Buffer, area: Rect, app: &mut App, sys: &System) {
    let _ = sys;
    let rd = &app.rd;
    let right = format!(
        "{} · LISTEN {} · {} · sort {}",
        rd.proto_filter.label(),
        if rd.listen_shown() { "yes" } else { "no" },
        rd.rows.len(),
        rd.sort.label()
    );
    let inner = Panel::new("connections", theme::rgb(theme::NET[1]))
        .right(&right)
        .render(area, buf);
    if inner.height < 2 || inner.width < 30 {
        return;
    }
    app.rd.page = inner.height as usize - 1;
    app.rd.follow();
    let rd = &app.rd;
    let cols = conn_columns(inner.width);
    let rows: Vec<Vec<Line<'static>>> = rd
        .rows
        .iter()
        .skip(rd.offset)
        .take(rd.page)
        .map(|r| {
            let c = &r.conn;
            let mut cells = vec![
                Line::from(span(c.proto.label(), dim())),
                Line::from(span(c.local.clone(), fg(theme::FG))),
                Line::from(span(c.remote.clone(), fg(theme::FG))),
                state_cell(c.state),
                match c.pid {
                    Some(p) => Line::from(span(p.to_string(), dim())),
                    None => Line::from(span("—", dim())),
                },
                if r.name.is_empty() {
                    Line::from(span("—", dim()))
                } else {
                    Line::from(span(r.name.clone(), fg(theme::FG)))
                },
            ];
            if cols.len() > 6 {
                let q = if c.rx_queue == 0 && c.tx_queue == 0 {
                    span("0/0", dim())
                } else {
                    span(
                        format!("{}/{}", fmt::bytes(c.rx_queue), fmt::bytes(c.tx_queue)),
                        fg(theme::FG),
                    )
                };
                cells.push(Line::from(q));
            }
            cells
        })
        .collect();
    let sel = rd.sel.checked_sub(rd.offset).filter(|s| *s < rows.len());
    ProcTable {
        cols: &cols,
        rows: &rows,
        selected: sel,
        sort: None,
    }
    .render(inner, buf);
    if rows.is_empty() {
        putn(
            buf,
            inner.x + 1,
            inner.y + 1,
            if app.rd.all.is_empty() {
                "no connections"
            } else {
                "no connections match these filters"
            },
            inner.width.saturating_sub(1) as usize,
            dim(),
        );
    }
    app.rd.hits.conns = Some(inner);
}

// ------------------------------------------------------------------ dischi

/// Altezza di un pannello disco (bordi compresi).
const DISK_H: u16 = 7;

fn disk_columns(w: u16) -> usize {
    if w >= 110 { 2 } else { 1 }
}

fn disk_title_right(d: &DiskIo) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !d.model.is_empty() {
        parts.push(d.model.clone());
    }
    if !d.kind.is_empty() {
        parts.push(d.kind.to_string());
    }
    if d.size > 0 {
        parts.push(fmt::bytes(d.size));
    }
    if let Some(t) = d.temp {
        parts.push(fmt::temp(t));
    }
    parts.join(" · ")
}

fn draw_disk(
    buf: &mut Buffer,
    area: Rect,
    d: &DiskIo,
    hist: Option<&(History, History)>,
    bottom: &str,
) {
    let right = disk_title_right(d);
    let inner = Panel::new(&d.name, theme::rgb(theme::DISK[1]))
        .right(&right)
        .bottom(bottom)
        .render(area, buf);
    if inner.height == 0 || inner.width < 12 {
        return;
    }
    let tw: u16 = if inner.width >= 44 { 28 } else { inner.width };
    let gw = if tw < inner.width {
        inner.width - tw - 1
    } else {
        0
    };
    if gw > 0 {
        let empty = (History::raw(1), History::raw(1));
        let (r, w) = hist.unwrap_or(&empty);
        let n = gw as usize * 2;
        let scale = fmt::nice_max(r.max_last(n).max(w.max_last(n)), 1024.0 * 1024.0);
        let norm =
            |h: &History| -> Vec<f64> { h.last_n(n).into_iter().map(|v| v / scale).collect() };
        let rh = inner.height - inner.height / 2;
        BrailleGraph {
            values: &norm(r),
            gradient: theme::DISK,
            down: false,
        }
        .render(Rect::new(inner.x, inner.y, gw, rh), buf);
        BrailleGraph {
            values: &norm(w),
            gradient: theme::WR,
            down: true,
        }
        .render(Rect::new(inner.x, inner.y + rh, gw, inner.height / 2), buf);
    }
    let x = inner.x + if gw > 0 { gw + 1 } else { 0 };
    let tw = tw as usize;
    let mb = |v: f64| v / 1048576.0;
    let lat = match d.latency_ms {
        Some(l) => format!("{l:.2} ms"),
        None => "—".to_string(),
    };
    let lines = [
        (
            format!("R {:.1} MB/s", mb(d.read_rate)),
            fg(theme::grad(theme::DISK, 0.7)),
        ),
        (
            format!("W {:.1} MB/s", mb(d.write_rate)),
            fg(theme::grad(theme::WR, 0.7)),
        ),
        (
            format!("IOPS {:.0}/{:.0}", d.read_iops, d.write_iops),
            fg(theme::FG),
        ),
    ];
    for (i, (t, st)) in lines.iter().enumerate().take(inner.height as usize) {
        putn(buf, x, inner.y + i as u16, t, tw, *st);
    }
    if inner.height > 3 {
        let y = inner.y + 3;
        put(buf, x, y, "Use ", dim());
        let pct = format!(" {:>3.0}%", d.util * 100.0);
        let mw = tw.saturating_sub(4 + width(&pct)).min(16);
        if mw > 0 {
            Meter::new(d.util, theme::USED).render(Rect::new(x + 4, y, mw as u16, 1), buf);
            putn(buf, x + 4 + mw as u16, y, &pct, tw, fg(theme::FG));
        }
    }
    if inner.height > 4 {
        putn(
            buf,
            x,
            inner.y + 4,
            format!("Lat {lat}  Queue {}", d.in_flight),
            tw,
            fg(theme::FG),
        );
    }
}

fn draw_disks(buf: &mut Buffer, area: Rect, app: &mut App, sys: &System) {
    let disks = &sys.disks.io;
    if disks.is_empty() {
        let inner = Panel::new("disks", theme::rgb(theme::DISK[1])).render(area, buf);
        putn(
            buf,
            inner.x + 1,
            inner.y,
            "no physical disk",
            inner.width.saturating_sub(1) as usize,
            dim(),
        );
        return;
    }
    let cols = disk_columns(area.width).min(disks.len());
    let rows_fit = (area.height / DISK_H).max(1) as usize;
    let cap = rows_fit * cols;
    app.rd.disk_cap = cap;
    app.rd.disk_off = app.rd.disk_off.min(disks.len().saturating_sub(cap));
    let off = app.rd.disk_off;
    let shown: Vec<&DiskIo> = disks.iter().skip(off).take(cap).collect();
    let rows_used = shown.len().div_ceil(cols).max(1);
    let ph = (area.height / rows_used as u16).max(1);
    let pw = area.width / cols as u16;
    for (k, d) in shown.iter().enumerate() {
        let (r, c) = (k / cols, k % cols);
        let x = area.x + c as u16 * pw;
        let w = if c + 1 == cols {
            area.width - c as u16 * pw
        } else {
            pw
        };
        let h = if r + 1 == rows_used {
            area.height - r as u16 * ph
        } else {
            ph
        };
        let bottom = if disks.len() > cap && k + 1 == shown.len() {
            format!(
                "{}-{} of {} · PgUp/PgDn",
                off + 1,
                off + shown.len(),
                disks.len()
            )
        } else {
            String::new()
        };
        draw_disk(
            buf,
            Rect::new(x, area.y + r as u16 * ph, w, h),
            d,
            app.rd.disk_rw.get(&d.name),
            &bottom,
        );
    }
}

// ------------------------------------------------------------------ filesystem

fn fs_columns(w: u16, mp_w: u16) -> Vec<ColumnDef> {
    let mut v = vec![ColumnDef::new("Mounted on", mp_w, false, None)];
    if w >= 84 {
        v.push(ColumnDef::new("Device", 16, false, None));
        v.push(ColumnDef::new("Type", 7, false, None));
    }
    v.push(ColumnDef::new("Use", 14, false, None));
    v.push(ColumnDef::new("Used", 6, true, None));
    if w >= 70 {
        v.push(ColumnDef::new("Free", 6, true, None));
    }
    v.push(ColumnDef::new("Total", 6, true, None));
    if w >= 96 {
        v.push(ColumnDef::new("Inode", 5, true, None));
    }
    if w >= 124 {
        v.push(ColumnDef::new("Options", 0, false, None));
    }
    v
}

fn fs_row(f: &Filesystem, cols: &[ColumnDef]) -> Vec<Line<'static>> {
    let frac = f.used_frac();
    let red = frac > 0.9;
    cols.iter()
        .map(|c| match c.title {
            "Mounted on" => Line::from(span(f.mountpoint.clone(), fg(theme::FG))),
            "Device" => Line::from(span(clip(&f.source, 16), fg(theme::FG))),
            "Type" => Line::from(span(f.fstype.clone(), dim())),
            _ if f.unresponsive => match c.title {
                "Use" => Line::from(span("not responding", fg(theme::RED))),
                _ => Line::from(span("—", dim())),
            },
            "Use" => {
                let mut spans = Meter::new(frac, theme::USED).spans(9);
                let st = if red { fg(theme::RED) } else { fg(theme::FG) };
                spans.push(span(format!(" {:>3.0}%", frac * 100.0), st));
                Line::from(spans)
            }
            "Used" => Line::from(span(
                fmt::bytes(f.used),
                if red { fg(theme::RED) } else { fg(theme::FG) },
            )),
            "Free" => Line::from(span(fmt::bytes(f.avail), fg(theme::FG))),
            "Total" => Line::from(span(fmt::bytes(f.total), dim())),
            "Inode" => Line::from(match f.inode_frac {
                Some(i) => span(format!("{:.0}%", i * 100.0), fg(theme::FG)),
                None => span("—", dim()),
            }),
            _ => Line::from(span(f.options.clone(), dim())),
        })
        .collect()
}

fn draw_fs(buf: &mut Buffer, area: Rect, sys: &System) {
    let fsl = &sys.disks.filesystems;
    let cap = area.height.saturating_sub(3) as usize;
    let bottom = if fsl.len() > cap {
        format!("+{} more", fsl.len() - cap)
    } else {
        String::new()
    };
    let inner = Panel::new("filesystems", theme::rgb(theme::DISK[1]))
        .bottom(&bottom)
        .render(area, buf);
    if inner.height < 2 || inner.width < 24 {
        return;
    }
    let mp_w = fsl
        .iter()
        .map(|f| width(&f.mountpoint))
        .max()
        .unwrap_or(10)
        .clamp(10, 24) as u16;
    let mp_w = mp_w.min(inner.width / 3).max(6);
    let cols = fs_columns(inner.width, mp_w);
    let rows: Vec<Vec<Line<'static>>> = fsl
        .iter()
        .take(inner.height as usize - 1)
        .map(|f| fs_row(f, &cols))
        .collect();
    ProcTable {
        cols: &cols,
        rows: &rows,
        selected: None,
        sort: None,
    }
    .render(inner, buf);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocazione_altezze() {
        let items = [(5, 9), (5, 9), (6, 1000), (7, 14), (5, 8)];
        // ampio: tutti i desiderati, il resto alle connessioni
        let h = allocate(50, &items);
        assert_eq!(h.iter().sum::<u16>(), 50);
        assert_eq!((h[0], h[1], h[3], h[4]), (9, 9, 14, 8));
        assert_eq!(h[2], 10);
        // stretto (22): rete, connessioni, interfacce, poi i dischi non entrano
        let h = allocate(22, &items);
        assert_eq!(h.iter().sum::<u16>(), 22);
        assert_eq!((h[3], h[4]), (0, 0));
        assert!(h[0] >= 5 && h[1] >= 5 && h[2] >= 6);
        // minuscolo: tutto alla rete
        assert_eq!(allocate(3, &items), vec![3, 0, 0, 0, 0]);
        assert_eq!(allocate(0, &items), vec![0; 5]);
    }
}
