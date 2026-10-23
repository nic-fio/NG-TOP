// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Disegno di un fotogramma: schede in alto, corpo della sezione, barra dei tasti in basso.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;

use crate::collect::System;

use super::app::{App, LayoutMode, Section};
use super::fmt;
use super::keys::{
    KEYS_BIG, KEYS_FILTER, KEYS_GPU, KEYS_RETE, KEYS_SERVIZI, KEYS_SMALL, help_lines,
};
use super::sections::{gpu, panoramica, processi, retedischi, servizi};
use super::theme;
use super::widgets::{Dialog, KeyBar, TabBar, put, putn};

pub fn draw(frame: &mut Frame, app: &mut App, sys: &System) {
    let area = frame.area();
    let buf = frame.buffer_mut();
    app.hits.table = None;
    app.hits.tabs.clear();
    if area.width < 10 || area.height < 4 {
        return;
    }
    let [top, body, bottom] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(area);

    draw_tabs(buf, top, app, sys);
    match app.section {
        Section::Panoramica => {
            panoramica::draw(buf, body, app, sys, app.layout == LayoutMode::Grande)
        }
        Section::Gpu => gpu::draw(buf, body, app, sys),
        Section::ReteDischi => retedischi::draw(buf, body, app, sys),
        Section::Processi => processi::draw(buf, body, app, sys),
        Section::Servizi => servizi::draw(buf, body, app, sys),
    }
    let keys = if app.filtering || (app.section == Section::Servizi && app.servizi.searching) {
        KEYS_FILTER
    } else if app.section == Section::Servizi {
        KEYS_SERVIZI
    } else if app.section == Section::Gpu {
        KEYS_GPU
    } else if app.section == Section::ReteDischi {
        KEYS_RETE
    } else if app.section == Section::Processi {
        processi::KEYS_PROC
    } else if app.layout == LayoutMode::Grande {
        KEYS_BIG
    } else {
        KEYS_SMALL
    };
    KeyBar { items: keys }.render(bottom, buf);
    // Esito di segnali e nice: riga di stato al posto della barra dei tasti, per qualche secondo.
    if let Some((text, ok)) = app.proc.status_text() {
        let color = if ok {
            theme::rgb(theme::USED[0])
        } else {
            theme::RED
        };
        for x in bottom.x..bottom.x + bottom.width {
            put(buf, x, bottom.y, " ", Style::default());
        }
        putn(
            buf,
            bottom.x + 1,
            bottom.y,
            text,
            bottom.width.saturating_sub(2) as usize,
            Style::default().fg(color),
        );
    }
    if app.help {
        let lines = help_lines(area.width as usize);
        Dialog {
            title: "Help",
            lines: &lines,
            buttons: &["Close"],
            selected: 0,
        }
        .render(area, buf);
    } else if let Some(d) = &app.dialog {
        d.render(area, buf);
    }
}

fn interval_text(ms: u64) -> String {
    if ms.is_multiple_of(1000) {
        format!("{}s", ms / 1000)
    } else {
        format!("{ms}ms")
    }
}

fn draw_tabs(buf: &mut Buffer, area: Rect, app: &mut App, sys: &System) {
    let names: Vec<&str> = Section::ALL.iter().map(|s| s.title()).collect();
    let unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let right = format!(
        "{} │ up {} │ load {:.2} {:.2} {:.2} │ every {} │ {}",
        app.hostname,
        fmt::uptime(sys.uptime.up_secs),
        sys.load.one,
        sys.load.five,
        sys.load.fifteen,
        interval_text(app.interval_ms),
        app.tz.clock(unix)
    );
    let bar = TabBar {
        tabs: &names,
        active: app.section.index(),
        right: &right,
    };
    app.hits.tabs = bar.tab_ranges(area.x);
    bar.render(area, buf);
}
