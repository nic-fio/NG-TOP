// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! GPU: sezione 2 (pannelli per dispositivo, cronologia, motori, energia, processi GPU)
//! e i pezzi riusati dalla Panoramica (pannello gpu e riga compatta).

use std::collections::HashMap;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier};
use ratatui::text::Span;

use crate::collect::System;
use crate::collect::gpu::{EngineKind, Gpu, GpuState, WorkKind, engine_kind};
use crate::collect::process::SortKey;
use crate::ui::app::App;
use crate::ui::fmt;
use crate::ui::theme::{self, Grad};
use crate::ui::widgets::{BrailleGraph, ColumnDef, Meter, Panel, Sparkline, put, putn, width};

use super::panoramica::{
    GpuAgg, dim, fg, frac, meter_row, pct_color, proc_table, sort_name, span, temp_text,
};

const GRAY: Color = Color::Rgb(0x7d, 0x85, 0x90);

fn panel_color(g: &Gpu) -> Color {
    if matches!(g.state, GpuState::Active | GpuState::Idle) {
        theme::rgb(theme::GPU[1])
    } else {
        GRAY
    }
}

/// Un elemento informativo di un pannello GPU: barra con valore oppure testo.
pub(crate) enum Cell {
    Meter {
        label: &'static str,
        frac: f64,
        grad: Grad,
        text: String,
    },
    Text(Vec<Span<'static>>),
}

/// Elementi disponibili per il dispositivo (quelli mancanti sono omessi).
/// `meters_first`: ENC/DEC subito dopo uso e memoria (sezione GPU); altrimenti in coda (Panoramica).
pub(crate) fn cells(g: &Gpu, sys: &System, meters_first: bool) -> Vec<Cell> {
    let mut out = Vec::new();
    if matches!(g.state, GpuState::Suspended | GpuState::Unsupported) {
        return out;
    }
    if let Some(u) = g.usage {
        out.push(Cell::Meter {
            label: "GPU ",
            frac: u / 100.0,
            grad: theme::GPU,
            text: format!("{u:>3.0}%"),
        });
    }
    // Memoria: VRAM dedicata, oppure memoria di sistema residente per le integrate.
    match (g.vram_used, g.vram_total) {
        (Some(u), Some(t)) if t > 0 => out.push(Cell::Meter {
            label: "VRAM",
            frac: frac(u, t),
            grad: theme::MEM,
            text: format!("{}/{}", fmt::bytes(u), fmt::bytes(t)),
        }),
        _ => {
            if let Some(m) = g.sys_mem {
                out.push(Cell::Meter {
                    label: "MEM ",
                    frac: frac(m, sys.mem.total),
                    grad: theme::MEM,
                    text: fmt::bytes(m),
                });
            }
        }
    }
    let mut codec = Vec::new();
    for (label, v) in [("ENC ", g.enc), ("DEC ", g.dec), ("VID ", g.video)] {
        if let Some(v) = v {
            codec.push(Cell::Meter {
                label,
                frac: v / 100.0,
                grad: theme::UP,
                text: format!("{v:>3.0}%"),
            });
        }
    }
    if meters_first {
        out.append(&mut codec);
    }
    if !g.temps.is_empty() {
        let mut v = vec![span("Temp ", dim())];
        for (l, t) in g.temps.iter().take(3) {
            let (txt, c) = temp_text(*t);
            v.push(span(txt, fg(c)));
            v.push(span(format!(" {} ", l), dim()));
        }
        out.push(Cell::Text(v));
    } else if let Some(t) = g.temp {
        let (txt, c) = temp_text(t);
        out.push(Cell::Text(vec![span("Temp ", dim()), span(txt, fg(c))]));
    }
    if let Some(p) = g.power_w {
        let mut v = vec![
            span("Power ", dim()),
            span(format!("{p:.0}"), fg(theme::FG)),
        ];
        if let Some(c) = g.power_cap_w {
            v.push(span(format!("/{c:.0}"), dim()));
        }
        v.push(span(" W", dim()));
        out.push(Cell::Text(v));
    }
    if let Some(c) = g.clock_mhz {
        let mut v = vec![span("Clock ", dim()), span(format!("{c}"), fg(theme::FG))];
        if let Some(m) = g.clock_max_mhz {
            v.push(span(format!("/{m}"), dim()));
        }
        v.push(span(" MHz", dim()));
        out.push(Cell::Text(v));
    }
    if let Some(m) = g.mem_clock_mhz {
        out.push(Cell::Text(vec![
            span("Mem clk ", dim()),
            span(format!("{m} MHz"), fg(theme::FG)),
        ]));
    }
    if g.fan_rpm.is_some() || g.fan_pct.is_some() {
        let mut v = vec![span("Fan ", dim())];
        if let Some(p) = g.fan_pct {
            v.push(span(format!("{p:.0}% "), fg(theme::FG)));
        }
        if let Some(r) = g.fan_rpm {
            v.push(span(format!("{r} RPM"), dim()));
        }
        out.push(Cell::Text(v));
    }
    if let Some(p) = &g.pcie {
        out.push(Cell::Text(vec![
            span("PCIe ", dim()),
            span(p.clone(), fg(theme::FG)),
        ]));
    }
    if !meters_first {
        out.append(&mut codec);
    }
    out
}

/// Disegna gli elementi a due per riga dentro `area`; restituisce le righe usate.
pub(crate) fn draw_cells(buf: &mut Buffer, area: Rect, cells: &[Cell]) -> u16 {
    let half = area.width / 2;
    if half < 14 {
        return 0;
    }
    let mut used = 0;
    for (i, c) in cells.iter().enumerate() {
        let (col, row) = ((i % 2) as u16, (i / 2) as u16);
        if row >= area.height {
            break;
        }
        used = used.max(row + 1);
        let r = Rect::new(area.x + col * half, area.y + row, half.saturating_sub(1), 1);
        match c {
            Cell::Meter {
                label,
                frac,
                grad,
                text,
            } => {
                let val = [span(format!(" {text}"), fg(theme::FG))];
                meter_row(buf, r, label, 5, *frac, grad, &val);
            }
            Cell::Text(spans) => {
                let mut x = r.x;
                for s in spans {
                    if x >= r.x + r.width {
                        break;
                    }
                    let room = (r.x + r.width - x) as usize;
                    putn(buf, x, r.y, &s.content, room, s.style);
                    x += width(&s.content) as u16;
                }
            }
        }
    }
    used
}

/// Righe necessarie per `n` elementi.
pub(crate) fn cell_rows(n: usize) -> u16 {
    n.div_ceil(2) as u16
}

fn state_line(g: &Gpu) -> Option<String> {
    match g.state {
        GpuState::Active | GpuState::Idle => None,
        GpuState::Suspended if g.d3cold => {
            Some("suspended (D3cold) — not queried, to avoid waking it".into())
        }
        GpuState::Suspended => Some("suspended — not queried, to avoid waking it".into()),
        GpuState::Unsupported => Some(format!("support coming soon (driver {})", g.driver)),
    }
}

fn mem_title(g: &Gpu) -> String {
    match (g.vram_total, g.integrated) {
        (Some(t), _) if t > 0 => format!("{} · ", fmt::bytes(t)),
        _ => String::new(),
    }
}

// ---------------------------------------------------------------- Panoramica

/// Altezza del pannello gpu della Panoramica secondo lo spazio concesso (0 = non mostrarlo).
pub(crate) fn overview_height(app: &App, sys: &System, avail: u16) -> u16 {
    let Some(g) = app.selected_gpu(sys) else {
        return 0;
    };
    let rows = if state_line(g).is_some() {
        1
    } else {
        cell_rows(cells(g, sys, false).len()).max(1)
    };
    let full = 2 + 3 + rows;
    let no_graph = 2 + rows;
    if avail >= full {
        full
    } else if avail >= no_graph {
        no_graph
    } else if avail >= 3 {
        3
    } else {
        0
    }
}

pub(crate) fn draw_overview(buf: &mut Buffer, area: Rect, app: &App, sys: &System) {
    let Some(g) = app.selected_gpu(sys) else {
        return;
    };
    let idx = sys
        .gpu
        .devices
        .iter()
        .position(|d| d.pdev == g.pdev)
        .unwrap_or(0);
    let multi = sys.gpu.devices.len() > 1;
    let title = if multi {
        format!("gpu {idx}/{} (d: switch)", sys.gpu.devices.len())
    } else {
        format!("gpu{idx}")
    };
    let right = g.name.clone();
    let inner = Panel::new(&title, panel_color(g))
        .right(&right)
        .render(area, buf);
    if inner.height == 0 || inner.width < 12 {
        return;
    }
    if let Some(t) = state_line(g) {
        putn(buf, inner.x + 1, inner.y, t, inner.width as usize, dim());
        return;
    }
    let list = cells(g, sys, false);
    let rows = cell_rows(list.len()).max(1);
    let graph_h = inner.height.saturating_sub(rows).min(3);
    if graph_h > 0
        && let Some(h) = app.gpu_hist.get(&g.pdev)
    {
        let vals = h.usage.last_n(inner.width as usize * 2);
        BrailleGraph {
            values: &vals,
            gradient: theme::GPU,
            down: false,
        }
        .render(Rect::new(inner.x, inner.y, inner.width, graph_h), buf);
    }
    let body = Rect::new(
        inner.x,
        inner.y + graph_h,
        inner.width,
        inner.height - graph_h,
    );
    draw_cells(buf, body, &list);
}

/// Riga compatta: GPU [barra] % temp potenza memoria.
pub(crate) fn draw_compact_line(buf: &mut Buffer, r: Rect, app: &App, sys: &System) {
    let Some(g) = app.selected_gpu(sys) else {
        return;
    };
    let bold = fg(theme::rgb(theme::GPU[1])).add_modifier(Modifier::BOLD);
    put(buf, r.x, r.y, "GPU", bold);
    if let Some(t) = state_line(g) {
        let short = if g.state == GpuState::Suspended {
            "suspended"
        } else {
            "coming soon"
        };
        let _ = t;
        putn(
            buf,
            r.x + 4,
            r.y,
            short,
            r.width.saturating_sub(4) as usize,
            dim(),
        );
        return;
    }
    let mw = (r.width / 4).clamp(8, 24);
    let u = g.usage.unwrap_or(0.0);
    Meter::new(u / 100.0, theme::GPU).render(Rect::new(r.x + 4, r.y, mw, 1), buf);
    let mut text = format!("{u:>4.0}%");
    if let Some(t) = g.temp {
        text.push_str(&format!(" {}", fmt::temp(t)));
    }
    if let Some(p) = g.power_w {
        text.push_str(&format!(" {p:.0}W"));
    }
    if let (Some(u), Some(t)) = (g.vram_used, g.vram_total) {
        text.push_str(&format!(" {}/{}", fmt::bytes(u), fmt::bytes(t)));
    } else if let Some(m) = g.sys_mem {
        text.push_str(&format!(" {}", fmt::bytes(m)));
    }
    putn(
        buf,
        r.x + 4 + mw + 1,
        r.y,
        &text,
        r.width.saturating_sub(mw + 5) as usize,
        fg(pct_color(u)),
    );
}

// ---------------------------------------------------------------- sezione GPU

fn engine_label(name: &str) -> String {
    match name {
        "render" | "gfx" | "rcs" => "3D / gfx".into(),
        "compute" | "ccs" => "Compute".into(),
        "copy" | "dma" | "bcs" => "Copy (DMA)".into(),
        "enc" => "Encode".into(),
        "dec" => "Decode".into(),
        "video" | "vcs" => "Video".into(),
        "video-enhance" | "vecs" => "Video enh.".into(),
        other => other.to_string(),
    }
}

/// Aggregato per processo (tutte le GPU) per le colonne extra della tabella.
fn aggregate(sys: &System) -> HashMap<u32, GpuAgg> {
    let mut m: HashMap<u32, (GpuAgg, f64)> = HashMap::new();
    for p in &sys.gpu.procs {
        let idx = sys
            .gpu
            .devices
            .iter()
            .position(|d| d.pdev == p.pdev)
            .unwrap_or(0);
        let e = m.entry(p.pid).or_default();
        if !e.0.devs.is_empty() {
            e.0.devs.push(',');
        }
        e.0.devs.push_str(&idx.to_string());
        e.0.enc += p.enc_pct;
        e.0.dec += p.dec_pct;
        if p.kind != WorkKind::None && (e.0.kind == WorkKind::None || p.gpu_pct > e.1) {
            e.0.kind = p.kind;
            e.1 = p.gpu_pct;
        }
    }
    m.into_iter().map(|(k, (a, _))| (k, a)).collect()
}

fn gpu_columns(w: u16) -> Vec<ColumnDef> {
    let mut v = vec![ColumnDef::new("PID", 7, true, Some(SortKey::Pid))];
    if w >= 60 {
        v.push(ColumnDef::new("User", 9, false, Some(SortKey::User)));
    }
    v.push(ColumnDef::new("DEV", 3, true, None));
    if w >= 70 {
        v.push(ColumnDef::new("Type", 8, false, None));
    }
    let gw = if w >= 80 { 14 } else { 5 };
    v.push(ColumnDef::new("GPU%", gw, true, Some(SortKey::Gpu)));
    if w >= 90 {
        v.push(ColumnDef::new("ENC%", 4, true, None));
        v.push(ColumnDef::new("DEC%", 4, true, None));
    }
    v.push(ColumnDef::new("VRAM", 7, true, Some(SortKey::Vram)));
    if w >= 100 {
        v.push(ColumnDef::new("CPU%", 5, true, Some(SortKey::Cpu)));
        v.push(ColumnDef::new("Mem", 7, true, Some(SortKey::Mem)));
    }
    if w >= 120 {
        v.push(ColumnDef::new("Name", 16, false, Some(SortKey::Name)));
        v.push(ColumnDef::new("Command", 0, false, None));
    } else {
        v.push(ColumnDef::new("Name", 0, false, Some(SortKey::Name)));
    }
    v
}

pub fn draw(buf: &mut Buffer, area: Rect, app: &mut App, sys: &System) {
    let devs = &sys.gpu.devices;
    if devs.is_empty() {
        let inner = Panel::new("GPU", theme::rgb(theme::GPU[1])).render(area, buf);
        putn(
            buf,
            inner.x + 1,
            inner.y,
            "No GPU detected",
            inner.width as usize,
            dim(),
        );
        return;
    }
    // Pannelli dei dispositivi: affiancati se c'è spazio, altrimenti impilati.
    let n = devs.len();
    let cols = ((area.width / 70).max(1) as usize).min(n);
    let rows = n.div_ceil(cols);
    // Altezza dei pannelli: tante righe quante servono al dispositivo più ricco.
    let content_rows = devs
        .iter()
        .map(|g| {
            if matches!(g.state, GpuState::Active | GpuState::Idle) {
                cell_rows(cells(g, sys, true).len())
            } else {
                1
            }
        })
        .max()
        .unwrap_or(1)
        .max(1);
    let panel_h: u16 = 2 + content_rows;
    let top_h = (rows as u16 * panel_h)
        .min(area.height / 2)
        .max(panel_h.min(area.height));
    let mid_h = if area.height >= top_h + 14 {
        ((area.height - top_h) / 3).clamp(8, 14)
    } else {
        0
    };
    let [top, mid, procs] = Layout::vertical([
        Constraint::Length(top_h),
        Constraint::Length(mid_h),
        Constraint::Min(3),
    ])
    .areas(area);

    let cell_w = top.width / cols as u16;
    let per_col = (top.height / panel_h).max(1) as usize;
    for (i, g) in devs.iter().enumerate() {
        let (c, r) = (i / per_col.max(1), i % per_col.max(1));
        if c >= cols {
            break;
        }
        let a = Rect::new(
            top.x + c as u16 * cell_w,
            top.y + r as u16 * panel_h,
            cell_w,
            panel_h,
        );
        draw_device(buf, a, i, g, app, sys);
    }

    if mid_h > 0
        && let Some(g) = app.selected_gpu(sys)
    {
        let idx = devs.iter().position(|d| d.pdev == g.pdev).unwrap_or(0);
        if mid.width >= 100 {
            let [left, right] =
                Layout::horizontal([Constraint::Min(20), Constraint::Length(48)]).areas(mid);
            draw_history(buf, left, app, g, idx);
            let eng_h = (g.engines.len() as u16 + 2).clamp(4, mid.height.saturating_sub(5).max(4));
            let [eng, pow] =
                Layout::vertical([Constraint::Length(eng_h), Constraint::Min(0)]).areas(right);
            draw_engines(buf, eng, g, idx);
            draw_power(buf, pow, app, g);
        } else {
            draw_history(buf, mid, app, g, idx);
        }
    }

    // Processi GPU
    let extra = aggregate(sys);
    let total: u64 = app.view.iter().map(|(i, _)| app.rows[*i].gpu_mem).sum();
    let right = format!(
        "sort: ‹ {} ›  {} {} · {}",
        sort_name(app.sort),
        app.view.len(),
        if app.view.len() == 1 {
            "process"
        } else {
            "processes"
        },
        fmt::bytes(total)
    );
    let inner = Panel::new("GPU processes", theme::rgb(theme::PROC[1]))
        .right(&right)
        .render(procs, buf);
    let cols = gpu_columns(inner.width);
    proc_table(buf, inner, app, cols, false, &extra);
}

fn draw_device(buf: &mut Buffer, area: Rect, idx: usize, g: &Gpu, app: &App, sys: &System) {
    let selected = app.selected_gpu(sys).is_some_and(|s| s.pdev == g.pdev);
    let multi = sys.gpu.devices.len() > 1;
    let kind = if g.integrated { "integrated · " } else { "" };
    let title = format!(
        "{}GPU {idx} · {}",
        if multi && selected { "● " } else { "" },
        g.name
    );
    let idle = if g.state == GpuState::Idle {
        "idle · "
    } else {
        ""
    };
    let right = format!("{idle}{kind}{}{} · {}", mem_title(g), g.driver, g.pdev);
    let inner = Panel::new(&title, panel_color(g))
        .right(&right)
        .render(area, buf);
    if inner.height == 0 {
        return;
    }
    if let Some(t) = state_line(g) {
        putn(
            buf,
            inner.x + 1,
            inner.y,
            t,
            inner.width as usize,
            fg(theme::grad(theme::CPU, 0.5)),
        );
        return;
    }
    let list = cells(g, sys, true);
    if list.is_empty() {
        putn(
            buf,
            inner.x + 1,
            inner.y,
            "no data available",
            inner.width as usize,
            dim(),
        );
        return;
    }
    draw_cells(buf, inner, &list);
}

fn draw_history(buf: &mut Buffer, area: Rect, app: &App, g: &Gpu, idx: usize) {
    let inner = Panel::new(&format!("GPU {idx} history"), panel_color(g))
        .right("use · VRAM · temp")
        .render(area, buf);
    if inner.height < 2 || inner.width < 12 {
        return;
    }
    let Some(h) = app.gpu_hist.get(&g.pdev) else {
        return;
    };
    let lab = 5u16;
    let gw = inner.width - lab;
    let (usage_h, vram_h, temp_h) = if inner.height >= 8 {
        (inner.height - 4, 3, 1)
    } else {
        (inner.height, 0, 0)
    };
    let gx = inner.x + lab;
    let uvals = h.usage.last_n(gw as usize * 2);
    BrailleGraph {
        values: &uvals,
        gradient: theme::GPU,
        down: false,
    }
    .render(Rect::new(gx, inner.y, gw, usage_h), buf);
    put(buf, inner.x, inner.y, "100%", dim());
    if usage_h > 2 {
        put(buf, inner.x, inner.y + usage_h / 2, " 50%", dim());
    }
    put(buf, inner.x, inner.y + usage_h - 1, "  0%", dim());
    if vram_h > 0 {
        let vvals = h.vram.last_n(gw as usize * 2);
        let y = inner.y + usage_h;
        put(buf, inner.x, y, "VRAM", dim());
        BrailleGraph {
            values: &vvals,
            gradient: theme::MEM,
            down: false,
        }
        .render(Rect::new(gx, y, gw, vram_h), buf);
        let tvals: Vec<f64> = h
            .temp
            .last_n(gw as usize)
            .into_iter()
            .map(|t| t / 100.0)
            .collect();
        put(buf, inner.x, y + vram_h, fmt::temp_unit(), dim());
        Sparkline {
            values: &tvals,
            gradient: theme::TEMP,
        }
        .render(Rect::new(gx, y + vram_h, gw, temp_h), buf);
    }
}

fn draw_engines(buf: &mut Buffer, area: Rect, g: &Gpu, idx: usize) {
    let inner = Panel::new(&format!("engines · GPU {idx}"), panel_color(g)).render(area, buf);
    if inner.height == 0 {
        return;
    }
    if g.engines.is_empty() {
        putn(
            buf,
            inner.x + 1,
            inner.y,
            "no data (clients not readable)",
            inner.width as usize,
            dim(),
        );
        return;
    }
    for (i, (name, pct)) in g.engines.iter().enumerate().take(inner.height as usize) {
        let grad = match engine_kind(name) {
            EngineKind::Enc | EngineKind::Dec | EngineKind::Video => theme::UP,
            _ => theme::GPU,
        };
        let r = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
        let label = format!("{:<13}", engine_label(name));
        meter_row(
            buf,
            r,
            &label,
            13,
            pct / 100.0,
            grad,
            &[span(format!(" {pct:>3.0}%"), fg(pct_color(*pct)))],
        );
    }
}

/// Scrive una riga di span a `*y` dentro `inner` e passa alla riga successiva.
fn put_spans(buf: &mut Buffer, inner: Rect, y: &mut u16, spans: Vec<Span<'static>>) {
    if *y >= inner.y + inner.height {
        return;
    }
    let mut x = inner.x + 1;
    for s in spans {
        putn(
            buf,
            x,
            *y,
            &s.content,
            (inner.x + inner.width).saturating_sub(x) as usize,
            s.style,
        );
        x += width(&s.content) as u16;
    }
    *y += 1;
}

fn draw_power(buf: &mut Buffer, area: Rect, app: &App, g: &Gpu) {
    let inner = Panel::new("power", theme::rgb(theme::CPU[1])).render(area, buf);
    if inner.height == 0 {
        return;
    }
    let mut y = inner.y;
    if let Some(p) = g.power_w {
        let vals: Vec<f64> = app
            .gpu_hist
            .get(&g.pdev)
            .map(|h| {
                let max = g
                    .power_cap_w
                    .unwrap_or(0.0)
                    .max(h.power.max_last(22))
                    .max(1.0);
                h.power.last_n(22).into_iter().map(|v| v / max).collect()
            })
            .unwrap_or_default();
        put(buf, inner.x + 1, y, "Power   ", dim());
        Sparkline {
            values: &vals,
            gradient: theme::CPU,
        }
        .render(
            Rect::new(inner.x + 9, y, 22.min(inner.width.saturating_sub(10)), 1),
            buf,
        );
        putn(
            buf,
            inner.x + 32,
            y,
            format!(" {p:.0} W"),
            inner.width.saturating_sub(32) as usize,
            fg(theme::FG),
        );
        y += 1;
    }
    if let Some(c) = g.power_cap_w {
        put_spans(
            buf,
            inner,
            &mut y,
            vec![
                span("Limit    ", dim()),
                span(format!("{c:.0} W"), fg(theme::FG)),
            ],
        );
    }
    if g.fan_rpm.is_some() || g.fan_pct.is_some() {
        let mut v = vec![span("Fan      ", dim())];
        if let Some(p) = g.fan_pct {
            v.push(span(format!("{p:.0}% "), fg(theme::FG)));
        }
        if let Some(r) = g.fan_rpm {
            v.push(span(format!("{r} RPM"), dim()));
        }
        put_spans(buf, inner, &mut y, v);
    }
    if y == inner.y {
        put_spans(buf, inner, &mut y, vec![span("not available", dim())]);
    }
}
