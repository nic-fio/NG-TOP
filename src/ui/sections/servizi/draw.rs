// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Disegno della sezione Servizi (riferimento: mockup/09-servizi.html).

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};

use super::{
    Availability, Chip, Servizi, chip_counts, cpu_time_text, duration_text, start_label,
    state_look, user_rows, when_text,
};
use crate::collect::System;
use crate::collect::services::{Phase, Scope, UnitDetail, UnitRow, boot_phases};
use crate::ui::app::App;
use crate::ui::fmt;
use crate::ui::sections::panoramica::{dim, fg, pct_color};
use crate::ui::theme;
use crate::ui::tz::Tz;
use crate::ui::widgets::{Dialog, Meter, Panel, clip, put, putn, width};

fn bold(c: Color) -> Style {
    Style::default().fg(c).add_modifier(Modifier::BOLD)
}

fn yellow() -> Color {
    theme::rgb(theme::CPU[1])
}

fn green() -> Color {
    theme::rgb(theme::CPU[0])
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn phase_color(p: Phase) -> Color {
    match p {
        Phase::Firmware => theme::rgb(0x8957e5),
        Phase::Loader => theme::rgb(0x1f6feb),
        Phase::Kernel => green(),
        Phase::Initrd => yellow(),
        Phase::Servizi => theme::rgb(0xf0883e),
    }
}

/// Testo (dim) al centro dell'area.
fn centered_text(buf: &mut Buffer, r: Rect, text: &str, style: Style) {
    if r.height == 0 || r.width == 0 {
        return;
    }
    let w = width(text).min(r.width as usize) as u16;
    putn(
        buf,
        r.x + (r.width - w) / 2,
        r.y + r.height / 2,
        text,
        r.width as usize,
        style,
    );
}

pub fn draw(buf: &mut Buffer, area: Rect, app: &mut App, sys: &System) {
    let tz = app.tz.clone();
    let s = &mut app.servizi;
    match s.avail {
        Availability::NoSystemd => {
            let inner = Panel::new("Services", theme::rgb(theme::NET[1])).render(area, buf);
            centered_text(buf, inner, "Services unavailable: systemd not found", dim());
            return;
        }
        Availability::Pending => {
            let inner = Panel::new("Services", theme::rgb(theme::NET[1])).render(area, buf);
            centered_text(buf, inner, "loading…", dim());
            return;
        }
        Availability::Available => {}
    }
    let now = now_unix();
    if s.log_full {
        draw_journal_panel(buf, area, s, &tz, now);
    } else if area.width >= 120 && area.height >= 36 {
        let bottom_h = (area.height / 4).clamp(9, 14);
        let [top, bottom] =
            Layout::vertical([Constraint::Min(8), Constraint::Length(bottom_h)]).areas(area);
        let [list, detail] =
            Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)]).areas(top);
        draw_list(buf, list, s);
        draw_detail(buf, detail, s, &tz, now);
        let [boot, users] =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(bottom);
        draw_boot(buf, boot, s, &tz, now);
        draw_users(buf, users, s, sys, &tz, now);
    } else if area.width >= 100 {
        let [list, detail] =
            Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)])
                .areas(area);
        draw_list(buf, list, s);
        draw_detail(buf, detail, s, &tz, now);
    } else {
        let dh = (area.height / 2).clamp(6, 12).min(area.height);
        let [list, detail] =
            Layout::vertical([Constraint::Min(4), Constraint::Length(dh)]).areas(area);
        draw_list(buf, list, s);
        draw_detail(buf, detail, s, &tz, now);
    }
    if let Some(c) = &s.confirm {
        let mut lines = vec![
            format!("Unit: {}", c.unit),
            format!(
                "Scope: {}",
                if s.scope == Scope::System {
                    "system"
                } else {
                    "user"
                }
            ),
        ];
        match &s.machine {
            Some((target, root)) => {
                lines.push(format!("On: {target}"));
                if !root {
                    lines.push("Needs root on the server (start ngtop with --sudo).".to_string());
                }
            }
            None => lines.push("If needed, polkit will ask for the password.".to_string()),
        }
        Dialog {
            title: c.action.verb(),
            lines: &lines,
            buttons: &["Cancel", "Confirm"],
            selected: c.button,
        }
        .render(area, buf);
    }
}

// ----- elenco unità -----

fn draw_chips(buf: &mut Buffer, r: Rect, s: &Servizi, counts: &[usize; 6]) {
    let mut x = r.x + 1;
    let end = r.x + r.width;
    // Ambito a destra: "‹ system │ user ›".
    let scope_w = 20u16;
    let chips_end = if r.width > scope_w + 30 {
        end - scope_w
    } else {
        end
    };
    for (i, c) in Chip::ALL.iter().enumerate() {
        let t = format!(" {} {} ", c.label(), counts[i]);
        let w = width(&t) as u16;
        if x + w > chips_end {
            break;
        }
        let style = if *c == s.chip {
            Style::default()
                .fg(theme::DARK)
                .bg(theme::CYAN)
                .add_modifier(Modifier::BOLD)
        } else {
            fg(theme::FG)
        };
        put(buf, x, r.y, &t, style);
        x += w + 1;
    }
    if chips_end < end {
        let (a, b) = if s.scope == Scope::System {
            (bold(theme::CYAN), dim())
        } else {
            (dim(), bold(theme::CYAN))
        };
        let sx = end - scope_w;
        put(buf, sx, r.y, "‹ ", dim());
        put(buf, sx + 2, r.y, "system", a);
        put(buf, sx + 8, r.y, " │ ", dim());
        put(buf, sx + 11, r.y, "user", b);
        put(buf, sx + 15, r.y, " ›", dim());
    }
}

struct Cols {
    name: u16,
    desc: u16,
    state: u16,
    start: u16,
}

/// Larghezze delle colonne in `w` celle; le colonne facoltative spariscono se non c'è spazio.
fn cols_for(w: u16) -> Cols {
    // PID 7, Mem 6, CPU 5 fissi; stato 11; avvio 16.
    let (state, start) = (11u16, 16u16);
    let fixed_min = 7 + 6 + 5 + state + 5; // + 5 spazi
    if w < fixed_min + 18 {
        return Cols {
            name: w.saturating_sub(7 + 6 + 5 + state + 4).max(8),
            desc: 0,
            state,
            start: 0,
        };
    }
    let flex = w - (fixed_min + start + 1);
    if flex < 40 {
        return Cols {
            name: (w - fixed_min).max(8),
            desc: 0,
            state,
            start: 0,
        };
    }
    let name = (flex * 6 / 11).clamp(16, 40);
    Cols {
        name,
        desc: flex - name,
        state,
        start,
    }
}

fn draw_list(buf: &mut Buffer, area: Rect, s: &mut Servizi) {
    let title = "systemd units";
    let search_txt = if s.searching {
        format!("/ {}_", s.search)
    } else if s.search.is_empty() {
        String::new()
    } else {
        format!("/ {}", s.search)
    };
    let inner = Panel::new(title, theme::rgb(theme::NET[1]))
        .right(&search_txt)
        .render(area, buf);
    if inner.height < 3 || inner.width < 24 {
        s.page = 1;
        return;
    }
    // Riga di stato (esito delle azioni) sul bordo inferiore.
    if let Some(st) = &s.status {
        let color = if st.ok { green() } else { theme::RED };
        let t = clip(
            &format!(" {} ", st.text),
            area.width.saturating_sub(6) as usize,
        );
        put(buf, area.x + 2, area.y + area.height - 1, &t, bold(color));
    }
    let counts = match &s.units {
        Some(Ok(u)) => chip_counts(u),
        _ => [0; 6],
    };
    draw_chips(buf, Rect::new(inner.x, inner.y, inner.width, 1), s, &counts);
    let body = Rect::new(inner.x, inner.y + 1, inner.width, inner.height - 1);
    s.page = (body.height as usize).saturating_sub(1).max(1);

    let units = match &s.units {
        None => {
            centered_text(buf, body, "loading…", dim());
            return;
        }
        Some(Err(e)) => {
            centered_text(buf, body, &format!("Error: {e}"), fg(theme::RED));
            return;
        }
        Some(Ok(u)) => u,
    };
    let vis = s.visible();
    s.offset = crate::ui::widgets::proctable::follow(s.selected, s.offset, s.page, vis.len());
    let w = body.width.saturating_sub(2);
    let c = cols_for(w);
    // Posizioni x relative alla colonna di sinistra con 1 cella di margine.
    let mut xs = Vec::new();
    let mut x = body.x + 1;
    let mut add = |wd: u16| {
        let r = x;
        x += wd + 1;
        r
    };
    let x_name = add(c.name);
    let x_desc = (c.desc > 0).then(|| add(c.desc));
    let x_state = add(c.state);
    let x_start = (c.start > 0).then(|| add(c.start));
    let x_pid = add(7);
    let x_mem = add(6);
    let x_cpu = add(5);
    xs.push(x);
    let head = bold(theme::DIM);
    put(buf, x_name, body.y, "Unit", head);
    if let Some(xd) = x_desc {
        put(buf, xd, body.y, "Description", head);
    }
    put(buf, x_state, body.y, "State", head);
    if let Some(xa) = x_start {
        put(buf, xa, body.y, "Startup", head);
    }
    put(buf, x_pid + 4, body.y, "PID", head);
    put(buf, x_mem + 3, body.y, "Mem", head);
    put(buf, x_cpu + 1, body.y, "CPU%", head);

    if vis.is_empty() {
        let r = Rect::new(body.x, body.y + 1, body.width, body.height - 1);
        centered_text(buf, r, "no units", dim());
        return;
    }
    for (row, &ui) in vis.iter().skip(s.offset).take(s.page).enumerate() {
        let y = body.y + 1 + row as u16;
        let u: &UnitRow = &units[ui];
        let selected = s.offset + row == s.selected;
        if selected {
            crate::ui::widgets::style_area(
                buf,
                Rect::new(body.x, y, body.width, 1),
                Style::default().bg(theme::SEL_BG),
            );
        }
        let (sym, label, color) = state_look(&u.info);
        let failed = u.info.active_state == "failed";
        let name_style = if failed {
            fg(theme::RED)
        } else {
            fg(theme::FG)
        };
        putn(buf, x_name, y, &u.info.name, c.name as usize, name_style);
        if let Some(xd) = x_desc {
            putn(buf, xd, y, &u.info.description, c.desc as usize, dim());
        }
        putn(
            buf,
            x_state,
            y,
            format!("{sym} {label}"),
            c.state as usize,
            fg(color),
        );
        if let Some(xa) = x_start {
            putn(
                buf,
                xa,
                y,
                start_label(&u.info.unit_file_state),
                c.start as usize,
                fg(theme::FG),
            );
        }
        let right = |buf: &mut Buffer, x: u16, wd: u16, t: &str, st: Style| {
            let tw = width(t) as u16;
            putn(buf, x + wd.saturating_sub(tw), y, t, wd as usize, st);
        };
        match u.info.main_pid {
            Some(p) => right(buf, x_pid, 7, &p.to_string(), fg(theme::FG)),
            None => right(buf, x_pid, 7, "—", dim()),
        }
        match u.mem {
            Some(m) => right(buf, x_mem, 6, &fmt::bytes(m), fg(theme::FG)),
            None => right(buf, x_mem, 6, "—", dim()),
        }
        match u.cpu {
            Some(p) => right(buf, x_cpu, 5, &format!("{p:.1}"), fg(pct_color(p))),
            None => right(buf, x_cpu, 5, "—", dim()),
        }
    }
}

// ----- dettaglio e registro -----

fn label(buf: &mut Buffer, x: u16, y: u16, t: &str) {
    put(buf, x, y, t, dim());
}

fn draw_detail(buf: &mut Buffer, area: Rect, s: &Servizi, tz: &Tz, now: i64) {
    let sel = s.selected_row();
    let name = sel.map_or("", |u| u.info.name.as_str());
    let d: Option<&UnitDetail> = match &s.detail {
        Some((n, Ok(d))) if n == name => Some(d),
        _ => None,
    };
    let path = d.map_or("", |d| d.fragment_path.as_str());
    let inner = Panel::new(
        if name.is_empty() { "details" } else { name },
        theme::rgb(theme::NET[1]),
    )
    .right(path)
    .render(area, buf);
    if inner.width < 12 || inner.height == 0 {
        return;
    }
    let Some(row) = sel else {
        centered_text(buf, inner, "no unit selected", dim());
        return;
    };
    let x0 = inner.x + 1;
    let vx = x0 + 9;
    let vw = inner.width.saturating_sub(11) as usize;
    let err = match &s.detail {
        Some((n, Err(e))) if n == name => Some(e.clone()),
        _ => None,
    };
    // Righe dell'intestazione per priorità di visualizzazione (la più importante ha numero basso).
    let header_n = (inner.height as usize).saturating_sub(4).clamp(1, 5);
    let mut y = inner.y;
    let show = |prio: usize| prio < header_n;
    let (sym, _, color) = state_look(&row.info);

    if show(0) {
        label(buf, x0, y, "State");
        let mut t = format!("{sym} {}", row.info.active_state);
        if !row.info.sub_state.is_empty() {
            t.push_str(&format!(" ({})", row.info.sub_state));
        }
        put(buf, vx, y, &t, fg(color));
        if let Some(since) = d.and_then(|d| d.since_usec) {
            let secs = (since / 1_000_000) as i64;
            let tx = vx + width(&t) as u16 + 1;
            let ago = duration_text((now - secs).max(0) as u64);
            let txt = format!("since {} ({ago} ago)", when_text(secs, now, tz, false));
            if tx < inner.x + inner.width {
                putn(
                    buf,
                    tx,
                    y,
                    &txt,
                    (inner.x + inner.width).saturating_sub(tx) as usize,
                    fg(theme::FG),
                );
            }
        }
        y += 1;
    }
    if show(1) {
        label(buf, x0, y, "PID");
        let t = match (&d, row.info.main_pid) {
            (Some(d), Some(pid)) => {
                let mut t = format!("{pid}");
                if let Some(c) = &d.comm {
                    t.push_str(&format!(" {c}"));
                }
                if let Some(n) = d.tasks {
                    t.push_str(&format!(" · {n} tasks"));
                    if let Some(m) = d.tasks_max {
                        t.push_str(&format!(" · limit {m}"));
                    }
                }
                t
            }
            (None, Some(pid)) => pid.to_string(),
            (Some(d), None) => d
                .tasks
                .map_or("—".to_string(), |n| format!("— · {n} tasks")),
            _ => "—".to_string(),
        };
        putn(buf, vx, y, &t, vw, fg(theme::FG));
        y += 1;
    }
    if show(2) {
        label(buf, x0, y, "Memory");
        let mem = d.and_then(|d| d.mem_current).or(row.mem);
        let mut t = mem.map_or("—".to_string(), fmt::bytes);
        if let Some(p) = d.and_then(|d| d.mem_peak) {
            t.push_str(&format!(" (peak {})", fmt::bytes(p)));
        }
        put(buf, vx, y, &t, fg(theme::FG));
        let cx = vx + width(&t) as u16 + 3;
        let cpu = d
            .and_then(|d| d.cpu_nsec)
            .map_or("—".to_string(), cpu_time_text);
        if cx + 4 < inner.x + inner.width {
            put(buf, cx, y, "CPU", dim());
            putn(
                buf,
                cx + 4,
                y,
                &cpu,
                (inner.x + inner.width).saturating_sub(cx + 4) as usize,
                fg(theme::FG),
            );
        }
        y += 1;
    }
    if show(4) {
        label(buf, x0, y, "Depends");
        let mut deps: Vec<&str> = Vec::new();
        if let Some(d) = d {
            for n in d.requires.iter().chain(&d.wants) {
                if !n.ends_with(".slice") && !deps.contains(&n.as_str()) {
                    deps.push(n);
                }
            }
        }
        let t = if deps.is_empty() {
            "—".to_string()
        } else {
            deps.join(" ")
        };
        putn(buf, vx, y, &t, vw, fg(theme::FG));
        y += 1;
    }
    if show(3) {
        label(buf, x0, y, "Restarts");
        let t = match d {
            Some(d) => {
                let n = d.n_restarts.map_or("—".to_string(), |n| n.to_string());
                if d.restart.is_empty() {
                    n
                } else {
                    format!("{n}   Restart={}", d.restart)
                }
            }
            None => "—".to_string(),
        };
        putn(buf, vx, y, &t, vw, fg(theme::FG));
        y += 1;
    }
    if d.is_none() {
        // Dettaglio non ancora arrivato (o errore): lo dice sulla riga dopo l'intestazione.
        let msg = err.map_or("loading…".to_string(), |e| format!("Error: {e}"));
        if y < inner.y + inner.height {
            putn(
                buf,
                x0,
                y,
                &msg,
                inner.width.saturating_sub(2) as usize,
                dim(),
            );
            y += 1;
        }
    }
    let rest = Rect::new(
        inner.x,
        y,
        inner.width,
        (inner.y + inner.height).saturating_sub(y),
    );
    draw_journal_in(buf, rest, s, name, tz, now);
}

/// Divisore "── registro (journalctl -u NOME) ──" e le ultime righe che ci stanno.
fn draw_journal_in(buf: &mut Buffer, r: Rect, s: &Servizi, name: &str, tz: &Tz, _now: i64) {
    if r.height == 0 || r.width < 8 {
        return;
    }
    let flag = if s.scope == Scope::System {
        "-u"
    } else {
        "--user-unit"
    };
    let head = format!("── journal (journalctl {flag} {name}) ");
    let mut line = head;
    while width(&line) < r.width as usize - 2 {
        line.push('─');
    }
    putn(buf, r.x + 1, r.y, &line, r.width as usize - 2, dim());
    let rows = r.height as usize - 1;
    if rows == 0 {
        return;
    }
    let body = Rect::new(r.x, r.y + 1, r.width, rows as u16);
    match &s.journal {
        Some((n, Ok(lines))) if n == name => {
            if lines.is_empty() {
                putn(
                    buf,
                    r.x + 1,
                    body.y,
                    "(journal empty or unreadable: the systemd-journal group is needed)",
                    r.width as usize - 2,
                    dim(),
                );
                return;
            }
            let start = lines.len().saturating_sub(rows);
            for (i, l) in lines[start..].iter().enumerate() {
                let y = body.y + i as u16;
                let color = match l.priority {
                    0..=3 => theme::RED,
                    4 => yellow(),
                    _ => theme::FG,
                };
                let t = tz.clock((l.time_usec / 1_000_000) as i64);
                put(buf, r.x + 1, y, &t, dim());
                putn(
                    buf,
                    r.x + 10,
                    y,
                    &l.message,
                    (r.width as usize).saturating_sub(11),
                    fg(color),
                );
            }
        }
        Some((n, Err(e))) if n == name => {
            putn(
                buf,
                r.x + 1,
                body.y,
                format!("Error: {e}"),
                r.width as usize - 2,
                fg(theme::RED),
            );
        }
        _ => putn(
            buf,
            r.x + 1,
            body.y,
            "loading…",
            r.width as usize - 2,
            dim(),
        ),
    }
}

/// Registro a tutto schermo (`j`).
fn draw_journal_panel(buf: &mut Buffer, area: Rect, s: &Servizi, tz: &Tz, now: i64) {
    let name = s.selected_name.clone().unwrap_or_default();
    let inner = Panel::new(&format!("journal · {name}"), theme::rgb(theme::NET[1]))
        .right("Esc closes")
        .render(area, buf);
    draw_journal_in(buf, inner, s, &name, tz, now);
}

// ----- avvio -----

fn draw_boot(buf: &mut Buffer, area: Rect, s: &Servizi, tz: &Tz, now: i64) {
    let phases = match &s.boot {
        Some(Ok(t)) => boot_phases(t).map(|p| (p, t.kernel_realtime)),
        _ => None,
    };
    let right = phases
        .as_ref()
        .map(|(p, kt)| {
            let total: f64 = p.iter().map(|x| x.1).sum();
            if *kt > 0 {
                let start = (*kt / 1_000_000) as i64;
                format!(
                    "last boot: {} · {total:.1} s",
                    when_text(start, now, tz, false)
                )
            } else {
                format!("{total:.1} s")
            }
        })
        .unwrap_or_default();
    let inner = Panel::new("boot", theme::rgb(theme::NET[1]))
        .right(&right)
        .render(area, buf);
    if inner.width < 20 || inner.height == 0 {
        return;
    }
    let x0 = inner.x + 1;
    let w = inner.width.saturating_sub(2) as usize;
    let Some((phases, _)) = phases else {
        let msg = match &s.boot {
            None => "loading…".to_string(),
            Some(Err(e)) => format!("Error: {e}"),
            Some(Ok(_)) => "boot times unavailable (container?)".to_string(),
        };
        putn(buf, x0, inner.y, &msg, w, dim());
        return;
    };
    // Barra a fasi proporzionali (almeno una cella per fase non nulla).
    let total: f64 = phases.iter().map(|p| p.1).sum::<f64>().max(1e-9);
    let mut cells: Vec<usize> = phases
        .iter()
        .map(|p| ((p.1 / total * w as f64).round() as usize).max(usize::from(p.1 > 0.0)))
        .collect();
    while cells.iter().sum::<usize>() > w {
        if let Some(m) = cells.iter_mut().max() {
            *m -= 1;
        }
    }
    let mut x = x0;
    for (i, (ph, _)) in phases.iter().enumerate() {
        for _ in 0..cells[i] {
            put(buf, x, inner.y, "█", fg(phase_color(*ph)));
            x += 1;
        }
    }
    let mut y = inner.y + 1;
    if y >= inner.y + inner.height {
        return;
    }
    // Legenda.
    let mut lx = x0;
    for (ph, secs) in &phases {
        let t = format!(" {} {secs:.1}s ", ph.label());
        let tw = width(&t) as u16 + 1;
        if lx + tw > x0 + w as u16 {
            break;
        }
        put(buf, lx, y, "■", fg(phase_color(*ph)));
        put(buf, lx + 1, y, &t, fg(theme::FG));
        lx += tw;
    }
    y += 1;
    if y >= inner.y + inner.height {
        return;
    }
    putn(buf, x0, y, "slowest (systemd-analyze blame)", w, dim());
    y += 1;
    let avail = (inner.y + inner.height).saturating_sub(y) as usize;
    match &s.blame {
        None => {
            if avail > 0 {
                putn(buf, x0, y, "loading…", w, dim());
            }
        }
        Some(Err(e)) => {
            if avail > 0 {
                putn(buf, x0, y, format!("Error: {e}"), w, fg(theme::RED));
            }
        }
        Some(Ok(list)) => {
            let n = avail.min(6);
            let max = list
                .iter()
                .take(n)
                .map(|b| b.1)
                .fold(0.0, f64::max)
                .max(1e-9);
            let name_w = (w / 2).clamp(12, 40);
            let mw = w.saturating_sub(name_w + 9).max(1);
            for (i, (name, secs)) in list.iter().take(n).enumerate() {
                let yy = y + i as u16;
                putn(buf, x0, yy, name, name_w, fg(theme::FG));
                Meter::new(secs / max, theme::DISK)
                    .render(Rect::new(x0 + name_w as u16 + 1, yy, mw as u16, 1), buf);
                let t = format!("{secs:.2} s");
                let tx = x0 + w as u16 - width(&t) as u16;
                put(buf, tx, yy, &t, fg(theme::FG));
            }
            if list.is_empty() && avail > 0 {
                putn(buf, x0, y, "no data", w, dim());
            }
        }
    }
}

// ----- utenti e sessioni -----

fn draw_users(buf: &mut Buffer, area: Rect, s: &Servizi, sys: &System, tz: &Tz, now: i64) {
    let inner = Panel::new("users & sessions", theme::rgb(theme::NET[1])).render(area, buf);
    if inner.width < 30 || inner.height == 0 {
        return;
    }
    let sessions = match &s.sessions {
        Some(Ok(v)) => Some(v.as_slice()),
        _ => None,
    };
    let rows = user_rows(sessions, &sys.processes);
    let x0 = inner.x + 1;
    let w = inner.width.saturating_sub(2);
    // User, Session, From, Since, Procs, CPU, Mem
    let (c_user, mut c_sess, c_from, c_since, c_proc, c_cpu, c_mem) =
        (10u16, 20u16, 0u16, 16u16, 5u16, 6u16, 6u16);
    let fixed = c_user + c_sess + c_since + c_proc + c_cpu + c_mem + 6;
    // Pannello stretto: spariscono prima Procs/CPU/Mem, poi Since; Session si accorcia.
    let show_agg = w >= fixed;
    let mut show_since = true;
    if !show_agg {
        c_sess = w.saturating_sub(c_user + c_since + 2).min(20);
        if c_sess < 8 {
            show_since = false;
            c_sess = w.saturating_sub(c_user + 1).clamp(0, 20);
        }
    }
    let c_from = if w > fixed + 6 {
        (w - fixed).min(16)
    } else {
        c_from
    };
    let mut x = x0;
    let mut add = |wd: u16| {
        let r = x;
        x += wd + 1;
        r
    };
    let xu = add(c_user);
    let xs = add(c_sess);
    let xf = (c_from > 0).then(|| add(c_from));
    let xd = add(c_since);
    let xp = add(c_proc);
    let xc = add(c_cpu);
    let xm = add(c_mem);
    let head = bold(theme::DIM);
    put(buf, xu, inner.y, "User", head);
    put(buf, xs, inner.y, "Session", head);
    if let Some(xf) = xf {
        put(buf, xf, inner.y, "From", head);
    }
    if show_since {
        put(buf, xd, inner.y, "Since", head);
    }
    if show_agg {
        put(buf, xp, inner.y, "Procs", head);
        put(buf, xc + 2, inner.y, "CPU", head);
        put(buf, xm + 3, inner.y, "Mem", head);
    }
    let avail = inner.height.saturating_sub(1) as usize;
    if s.sessions.is_none() && avail > 0 && rows.is_empty() {
        putn(buf, x0, inner.y + 1, "loading…", w as usize, dim());
    }
    for (i, r) in rows.iter().take(avail).enumerate() {
        let y = inner.y + 1 + i as u16;
        putn(buf, xu, y, &r.user, c_user as usize, fg(theme::FG));
        putn(buf, xs, y, &r.session, c_sess as usize, dim());
        if let Some(xf) = xf {
            putn(buf, xf, y, &r.from, c_from as usize, dim());
        }
        let since = r.since_usec.map_or("—".to_string(), |u| {
            when_text((u / 1_000_000) as i64, now, tz, false)
        });
        if show_since {
            putn(buf, xd, y, since, c_since as usize, dim());
        }
        let right = |buf: &mut Buffer, x: u16, wd: u16, t: &str, st: Style| {
            let tw = width(t) as u16;
            putn(buf, x + wd.saturating_sub(tw), y, t, wd as usize, st);
        };
        if let Some(a) = r.agg.filter(|_| show_agg) {
            right(buf, xp, c_proc, &a.procs.to_string(), fg(theme::FG));
            right(
                buf,
                xc,
                c_cpu,
                &format!("{:.1}%", a.cpu),
                fg(pct_color(a.cpu)),
            );
            right(buf, xm, c_mem, &fmt::bytes(a.mem), fg(theme::FG));
        }
    }
    if rows.len() > avail && avail > 0 {
        let y = inner.y + inner.height - 1;
        let t = format!("+{} more", rows.len() - avail + 1);
        for x in inner.x..inner.x + inner.width {
            put(buf, x, y, " ", Style::default());
        }
        put(buf, x0, y, &t, dim());
    }
}
