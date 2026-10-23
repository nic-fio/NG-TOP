// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Panoramica: CPU, memoria, GPU (segnaposto), rete, dischi e processi (grande) oppure righe compatte.

use std::collections::HashMap;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::gpu;
use crate::collect::System;
use crate::collect::disk::{Filesystem, disk_of_source};
use crate::collect::gpu::WorkKind;
use crate::collect::process::SortKey;
use crate::ui::app::App;
use crate::ui::fmt;
use crate::ui::history::History;
use crate::ui::theme::{self, Grad};
use crate::ui::widgets::proctable::follow;
use crate::ui::widgets::{
    BrailleGraph, ColumnDef, Meter, Panel, ProcTable, Sparkline, clip, put, putn, width,
};

pub fn sort_name(k: SortKey) -> &'static str {
    match k {
        SortKey::Pid => "pid",
        SortKey::Name => "name",
        SortKey::Cpu => "cpu",
        SortKey::Mem => "mem",
        SortKey::Gpu => "gpu",
        SortKey::Vram => "vram",
        SortKey::User => "user",
    }
}

pub(crate) fn pct_color(p: f64) -> Color {
    theme::grad(theme::CPU, p / 100.0)
}

pub(crate) fn frac(num: u64, den: u64) -> f64 {
    if den == 0 {
        0.0
    } else {
        num as f64 / den as f64
    }
}

pub(crate) fn fg(c: Color) -> Style {
    Style::default().fg(c)
}

pub(crate) fn dim() -> Style {
    fg(theme::DIM)
}

/// Riga "etichetta | barra | valore" in `r` (una riga). Il valore è a destra, con stile dato.
pub(crate) fn meter_row(
    buf: &mut Buffer,
    r: Rect,
    label: &str,
    label_w: u16,
    f: f64,
    g: Grad,
    value: &[Span<'static>],
) {
    if r.width < label_w + 6 || r.height == 0 {
        return;
    }
    put(buf, r.x, r.y, label, fg(theme::FG));
    let vw: u16 = value.iter().map(|s| width(&s.content) as u16).sum();
    let mw = r.width.saturating_sub(label_w + vw + 1).max(1);
    Meter::new(f, g).render(Rect::new(r.x + label_w, r.y, mw, 1), buf);
    let mut x = r.x + r.width - vw.min(r.width);
    for s in value {
        put(buf, x, r.y, &s.content, s.style);
        x += width(&s.content) as u16;
    }
}

pub(crate) fn span(s: impl Into<String>, st: Style) -> Span<'static> {
    Span::styled(s.into(), st)
}

pub fn draw(buf: &mut Buffer, area: Rect, app: &mut App, sys: &System, big: bool) {
    if big {
        draw_big(buf, area, app, sys);
    } else {
        draw_small(buf, area, app, sys);
    }
}

// ---------------------------------------------------------------- grande

fn draw_big(buf: &mut Buffer, area: Rect, app: &mut App, sys: &System) {
    let cpu_h = cpu_panel_height(area, sys.cpu.cores.len(), core_id_width(sys));
    let [cpu_a, lower] =
        Layout::vertical([Constraint::Length(cpu_h), Constraint::Min(1)]).areas(area);
    draw_cpu(buf, cpu_a, app, sys);

    let left_w = ((u32::from(area.width) * u32::from(app.left_pct) / 100) as u16)
        .max(60)
        .min(area.width.saturating_sub(40));
    let [left, right] =
        Layout::horizontal([Constraint::Length(left_w), Constraint::Min(1)]).areas(lower);

    // Altezze della colonna sinistra: la rete prende il resto (minimo 5).
    let mem_lines: u16 = if has_compressed(sys) { 6 } else { 5 };
    let h = left.height;
    let mut mem_h = mem_lines + 2;
    let mut disk_h = 2 + (sys.disks.filesystems.len().clamp(1, 6) as u16);
    let net_min = 5;
    // Il pannello gpu prende lo spazio che resta dopo mem, dischi e il minimo della rete.
    let gpu_h = gpu::overview_height(app, sys, h.saturating_sub(mem_h + disk_h + net_min));
    if mem_h + disk_h + net_min > h {
        disk_h = disk_h.saturating_sub(mem_h + disk_h + net_min - h).max(3);
    }
    if mem_h + disk_h + net_min > h {
        mem_h = mem_h.saturating_sub(mem_h + disk_h + net_min - h).max(4);
    }
    let [mem_a, gpu_a, net_a, disk_a] = Layout::vertical([
        Constraint::Length(mem_h),
        Constraint::Length(gpu_h),
        Constraint::Min(0),
        Constraint::Length(disk_h),
    ])
    .areas(left);
    draw_mem(buf, mem_a, app, sys);
    if gpu_h > 0 {
        gpu::draw_overview(buf, gpu_a, app, sys);
    }
    draw_net(buf, net_a, app, sys);
    draw_disks(buf, disk_a, app, sys);
    draw_procs(buf, right, app, sys);
}

fn has_compressed(sys: &System) -> bool {
    sys.zram.is_some() || sys.mem.zswap.unwrap_or(0) > 0
}

/// Frequenza media dei core in GHz, se almeno uno la espone.
pub(crate) fn avg_ghz(sys: &System) -> Option<f64> {
    let f: Vec<u64> = sys.cpu.cores.iter().filter_map(|c| c.freq_mhz).collect();
    (!f.is_empty()).then(|| f.iter().sum::<u64>() as f64 / f.len() as f64 / 1000.0)
}

/// Testo e colore della temperatura CPU.
pub(crate) fn temp_text(t: f64) -> (String, Color) {
    (fmt::temp(t), theme::grad(theme::TEMP, (t - 30.0) / 60.0))
}

fn core_id_width(sys: &System) -> u16 {
    sys.cpu
        .cores
        .iter()
        .map(|c| c.id.to_string().len())
        .max()
        .unwrap_or(1) as u16
}

/// Altezza del pannello cpu in base al contenuto: righe della griglia dei core + intestazione +
/// riga vuota + bordi, almeno 8 per il grafico, al massimo un terzo dello schermo (metà con
/// molti core).
fn cpu_panel_height(area: Rect, n: usize, id_w: u16) -> u16 {
    let mut cap = (area.height / 3).max(8);
    if n > 16 {
        cap = cap.max((n.div_ceil(6).min(usize::from(u16::MAX)) as u16 + 4).min(area.height / 2));
    }
    let cap = cap.min(area.height.saturating_sub(10).max(6));
    let iw = area.width.saturating_sub(2);
    let shown = |h: u16| {
        let (_, ncol, per_col, _) = core_layout(n, iw, h.saturating_sub(2), id_w);
        (per_col * usize::from(ncol)).min(n)
    };
    let (_, _, per_col, _) = core_layout(n, iw, cap.saturating_sub(2), id_w);
    let needed = (per_col.min(usize::from(u16::MAX)) as u16 + 4).clamp(8.min(cap), cap);
    // Se con l'altezza ridotta la griglia cambierebbe e mostrerebbe meno core, si resta al massimo.
    if shown(needed) < shown(cap) {
        cap
    } else {
        needed
    }
}

/// Disposizione della griglia dei core: (larghezza blocco, colonne, righe per colonna, compatta?).
fn core_layout(n: usize, inner_w: u16, inner_h: u16, id_w: u16) -> (u16, u16, usize, bool) {
    let rows_avail = (inner_h as usize).saturating_sub(2).max(1);
    let label_w = id_w + 2;
    let want_cols = n.div_ceil(rows_avail).clamp(1, usize::from(u16::MAX)) as u16;
    let block_for = |cell: u16, gap: u16| {
        // Calcoli in u32: nessun overflow su terminali enormi o con moltissimi core.
        let w = u32::from(inner_w);
        let wanted = u32::from(want_cols) * u32::from(cell + gap);
        let max_frac = if want_cols > 4 { w * 7 / 10 } else { w * 3 / 5 };
        let v = wanted.max(w / 3).min(max_frac).clamp(
            u32::from(cell),
            u32::from(inner_w.saturating_sub(10).max(cell)),
        );
        v as u16
    };
    // Celle complete (etichetta + barra + %) se ci stanno tutti i core, altrimenti solo etichetta e %.
    let full_cell = label_w + 7;
    let block_w = block_for(20, 2);
    let full_cols = ((block_w + 2) / (full_cell + 2)).max(1);
    if full_cols as usize * rows_avail >= n {
        let ncol = want_cols.min(full_cols);
        let per_col = n.div_ceil(ncol as usize).min(rows_avail).max(1);
        return (block_w, ncol, per_col, false);
    }
    let cell = label_w + 5;
    let block_w = block_for(cell, 1);
    let max_cols = ((block_w + 1) / (cell + 1)).max(1);
    let ncol = want_cols.min(max_cols);
    (
        block_w,
        ncol,
        n.div_ceil(ncol as usize).min(rows_avail).max(1),
        true,
    )
}

fn draw_cpu(buf: &mut Buffer, area: Rect, app: &App, sys: &System) {
    let cpu = &sys.cpu;
    let right = format!("{} · {} threads", cpu.model, cpu.logical);
    let n = cpu.cores.len();
    let id_w = core_id_width(sys);
    let (iw, ih) = (area.width.saturating_sub(2), area.height.saturating_sub(2));
    let (block_w, ncol, per_col, compact) = core_layout(n, iw, ih, id_w);
    let hidden = n.saturating_sub(per_col * ncol as usize);
    let bottom = if hidden > 0 {
        format!("+{hidden} cores not shown")
    } else {
        String::new()
    };
    let inner = Panel::new("cpu", theme::rgb(theme::CPU[0]))
        .right(&right)
        .bottom(&bottom)
        .render(area, buf);
    if inner.width < 20 || inner.height < 3 {
        return;
    }
    let label_w = id_w + 2; // "C" + id + spazio
    let block_x = inner.x + inner.width - block_w.min(inner.width);

    // Grafico del totale, valori nuovi a destra.
    let graph_w = inner.width.saturating_sub(block_w + 1);
    let hist = app.cpu_hist.last_n(graph_w as usize * 2);
    BrailleGraph {
        values: &hist,
        gradient: theme::CPU,
        down: false,
    }
    .render(Rect::new(inner.x, inner.y, graph_w, inner.height), buf);

    // Intestazione: CPU [meter] 29% 62°C 4.71GHz
    let mut tail = vec![span(
        format!("{:>3.0}%", cpu.total_usage),
        fg(pct_color(cpu.total_usage)).add_modifier(Modifier::BOLD),
    )];
    if let Some(t) = sys.cpu_temp {
        let (txt, c) = temp_text(t);
        tail.push(span(format!(" {txt}"), fg(c)));
    }
    if let Some(avg) = avg_ghz(sys) {
        tail.push(span(format!(" {avg:.2}GHz"), dim()));
    }
    meter_row(
        buf,
        Rect::new(block_x, inner.y, block_w, 1),
        "CPU ",
        4,
        cpu.total_usage / 100.0,
        theme::CPU,
        &tail,
    );

    // Griglia dei core in colonne (ordine per colonna).
    let gap = if compact { 1 } else { 2 };
    let col_w = (block_w.saturating_sub((ncol - 1) * gap)) / ncol;
    for (i, c) in cpu.cores.iter().enumerate().take(per_col * ncol as usize) {
        let (col, row) = ((i / per_col) as u16, (i % per_col) as u16);
        let r = Rect::new(block_x + col * (col_w + gap), inner.y + 2 + row, col_w, 1);
        let label = format!("C{:<w$} ", c.id, w = id_w as usize);
        let pct = format!(" {:>3.0}%", c.usage);
        if compact {
            put(buf, r.x, r.y, &label, dim());
            put(buf, r.x + label_w, r.y, &pct, fg(pct_color(c.usage)));
        } else {
            meter_row(
                buf,
                r,
                &label,
                label_w,
                c.usage / 100.0,
                theme::CPU,
                &[span(pct, fg(pct_color(c.usage)))],
            );
        }
    }
}

fn draw_mem(buf: &mut Buffer, area: Rect, app: &App, sys: &System) {
    let m = &sys.mem;
    let right = fmt::bytes(m.total);
    let inner = Panel::new("mem", theme::rgb(theme::MEM[1]))
        .right(&right)
        .render(area, buf);
    if inner.height == 0 || inner.width < 20 {
        return;
    }
    let gw = if inner.width >= 56 { 22 } else { 0 };
    let lw = inner.width.saturating_sub(if gw > 0 { gw + 1 } else { 0 });
    // Righe oltre l'altezza interna: altezza 0, quindi non disegnate.
    let row = |i: u16| Rect::new(inner.x, inner.y + i, lw, u16::from(i < inner.height));
    let val = |v: u64| vec![span(format!(" {:>6}", fmt::bytes(v)), fg(theme::FG))];
    meter_row(
        buf,
        row(0),
        "Used        ",
        12,
        frac(m.used, m.total),
        theme::USED,
        &val(m.used),
    );
    meter_row(
        buf,
        row(1),
        "Available   ",
        12,
        frac(m.available, m.total),
        theme::MEM,
        &val(m.available),
    );
    meter_row(
        buf,
        row(2),
        "Cache       ",
        12,
        frac(m.cache, m.total),
        theme::MEM,
        &val(m.cache),
    );
    meter_row(
        buf,
        row(3),
        "Free        ",
        12,
        frac(m.free, m.total),
        theme::MEM,
        &val(m.free),
    );
    meter_row(
        buf,
        row(4),
        "Swap        ",
        12,
        frac(m.swap_used, m.swap_total),
        theme::USED,
        &[
            span(format!(" {}", fmt::bytes(m.swap_used)), fg(theme::FG)),
            span(format!("/{}", fmt::bytes(m.swap_total)), dim()),
        ],
    );
    if has_compressed(sys) && inner.height > 5 {
        let mut parts = Vec::new();
        if let Some((orig, compr)) = sys.zram {
            parts.push(format!(
                "ZRAM {} → {} ({:.1}×)",
                fmt::bytes(orig),
                fmt::bytes(compr),
                orig as f64 / compr.max(1) as f64
            ));
        }
        if let Some(z) = m.zswap.filter(|z| *z > 0) {
            parts.push(format!(
                "zswap {} ← {}",
                fmt::bytes(z),
                fmt::bytes(m.zswapped.unwrap_or(0))
            ));
        }
        if m.dirty > 0 {
            parts.push(format!("Dirty {}", fmt::bytes(m.dirty)));
        }
        putn(
            buf,
            inner.x,
            inner.y + 5,
            parts.join("  "),
            lw as usize,
            dim(),
        );
    }
    if gw > 0 {
        let hist = app.mem_hist.last_n(gw as usize * 2);
        BrailleGraph {
            values: &hist,
            gradient: theme::MEM,
            down: false,
        }
        .render(Rect::new(inner.x + lw + 1, inner.y, gw, inner.height), buf);
    }
}

fn draw_net(buf: &mut Buffer, area: Rect, app: &App, sys: &System) {
    let Some(ifc) = sys.net.pick(app.net_iface.as_deref()) else {
        Panel::new("net", theme::rgb(theme::NET[1])).render(area, buf);
        return;
    };
    let multi = sys.net.ifaces.iter().filter(|i| i.name != "lo").count() > 1;
    let mut right = ifc.name.clone();
    if multi {
        right.push_str(" ‹ ›");
    }
    if ifc.up == Some(false) {
        right.push_str(" (down)");
    }
    if let Some(ip) = &ifc.ipv4 {
        right.push_str(&format!("  {ip}"));
    }
    let empty = (History::raw(1), History::raw(1));
    let (rx, tx) = app.net_hist.get(&ifc.name).unwrap_or(&empty);
    let iw_guess = area.width.saturating_sub(2) as usize;
    let scale = fmt::nice_max(
        rx.max_last(iw_guess * 2).max(tx.max_last(iw_guess * 2)),
        100.0 * 1024.0,
    );
    let bottom = format!(
        "scale {}{}",
        fmt::rate(scale),
        if multi { " · n: switch" } else { "" }
    );
    let inner = Panel::new("net", theme::rgb(theme::NET[1]))
        .right(&right)
        .bottom(&bottom)
        .render(area, buf);
    if inner.height == 0 || inner.width < 10 {
        return;
    }
    let tw = if inner.width >= 44 { 22 } else { 0 };
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
    if tw > 0 {
        let x = inner.x + gw + 1;
        let down_c = theme::grad(theme::NET, 0.7);
        let up_c = theme::grad(theme::UP, 0.6);
        let lines: Vec<(String, Style)> = if inner.height >= 6 {
            vec![
                (format!("▼ {}", fmt::rate(ifc.rx_rate)), fg(down_c)),
                (format!("  peak {}", fmt::rate(ifc.rx_peak)), dim()),
                (format!("  total {}", fmt::bytes(ifc.rx_bytes)), dim()),
                (format!("▲ {}", fmt::rate(ifc.tx_rate)), fg(up_c)),
                (format!("  peak {}", fmt::rate(ifc.tx_peak)), dim()),
                (format!("  total {}", fmt::bytes(ifc.tx_bytes)), dim()),
            ]
        } else {
            vec![
                (format!("▼ {}", fmt::rate(ifc.rx_rate)), fg(down_c)),
                (format!("▲ {}", fmt::rate(ifc.tx_rate)), fg(up_c)),
                (format!("  ▼ tot {}", fmt::bytes(ifc.rx_bytes)), dim()),
                (format!("  ▲ tot {}", fmt::bytes(ifc.tx_bytes)), dim()),
            ]
        };
        for (i, (t, st)) in lines.iter().enumerate().take(inner.height as usize) {
            putn(buf, x, inner.y + i as u16, t, tw as usize, *st);
        }
    }
}

fn disk_spark(app: &App, fs: &Filesystem) -> Vec<f64> {
    let key = disk_of_source(&fs.source).filter(|k| app.disk_hist.contains_key(k));
    let h = app.disk_hist.get(key.as_deref().unwrap_or(""));
    h.map(|h| h.last_n(64)).unwrap_or_default()
}

fn draw_disks(buf: &mut Buffer, area: Rect, app: &App, sys: &System) {
    let d = &sys.disks;
    let right = format!(
        "R {:.1} W {:.1} MB/s",
        d.read_rate / 1048576.0,
        d.write_rate / 1048576.0
    );
    let fsl = &d.filesystems;
    let cap = (area.height.saturating_sub(2)) as usize;
    let extra = fsl.len().saturating_sub(cap);
    let bottom = if extra > 0 {
        format!("+{extra} more")
    } else {
        String::new()
    };
    let inner = Panel::new("disks", theme::rgb(theme::DISK[1]))
        .right(&right)
        .bottom(&bottom)
        .render(area, buf);
    if inner.width < 24 {
        return;
    }
    let mpw = fsl
        .iter()
        .map(|f| width(&f.mountpoint))
        .max()
        .unwrap_or(1)
        .clamp(6, 14) as u16;
    let vw: u16 = 15;
    let sw: u16 = if inner.width >= mpw + 1 + 10 + 1 + vw + 1 + 12 {
        12
    } else {
        0
    };
    let mw = inner
        .width
        .saturating_sub(mpw + 1 + 1 + vw + if sw > 0 { sw + 1 } else { 0 })
        .clamp(4, 28);
    // Scala comune a tutti i dischi, così le sparkline sono confrontabili.
    let scale = fmt::nice_max(
        app.disk_hist
            .values()
            .map(|h| h.max_last(64))
            .fold(0.0, f64::max),
        1024.0 * 1024.0,
    );
    for (i, f) in fsl.iter().enumerate().take(cap) {
        let y = inner.y + i as u16;
        let used_f = f.used_frac();
        put(
            buf,
            inner.x,
            y,
            clip(&f.mountpoint, mpw as usize),
            fg(theme::FG),
        );
        Meter::new(used_f, theme::USED).render(Rect::new(inner.x + mpw + 1, y, mw, 1), buf);
        let vx = inner.x + mpw + 1 + mw + 1;
        let used_style = if used_f > 0.9 {
            fg(theme::RED)
        } else {
            fg(theme::FG)
        };
        let u = fmt::bytes(f.used);
        put(buf, vx, y, &u, used_style);
        putn(
            buf,
            vx + width(&u) as u16,
            y,
            format!("/{}", fmt::bytes(f.total)),
            vw as usize,
            dim(),
        );
        if sw > 0 {
            let vals: Vec<f64> = disk_spark(app, f).into_iter().map(|v| v / scale).collect();
            Sparkline {
                values: &vals,
                gradient: theme::DISK,
            }
            .render(Rect::new(inner.x + inner.width - sw, y, sw, 1), buf);
        }
    }
}

// ---------------------------------------------------------------- processi

/// Colonne della tabella in base alla larghezza disponibile.
/// Passo 4: qui si aggiungeranno GPU% e VRAM prima di CPU%.
fn proc_columns(w: u16, with_gpu: bool) -> Vec<ColumnDef> {
    let mut v = vec![ColumnDef::new("PID", 7, true, Some(SortKey::Pid))];
    if w >= 70 {
        v.push(ColumnDef::new("Name", 16, false, Some(SortKey::Name)));
        v.push(ColumnDef::new("Command", 0, false, None));
    } else {
        v.push(ColumnDef::new("Name", 0, false, Some(SortKey::Name)));
    }
    if w >= 56 {
        v.push(ColumnDef::new("User", 10, false, Some(SortKey::User)));
    }
    if w >= 70 {
        v.push(ColumnDef::new("Thr", 4, true, None));
    }
    v.push(ColumnDef::new("Mem", 7, true, Some(SortKey::Mem)));
    // Con almeno una GPU e abbastanza spazio: GPU% e VRAM.
    if with_gpu && w >= 90 {
        v.push(ColumnDef::new("GPU%", 5, true, Some(SortKey::Gpu)));
        v.push(ColumnDef::new("VRAM", 7, true, Some(SortKey::Vram)));
    }
    // Stretto: solo il numero, senza la barretta.
    let cpu_w = if w >= 56 { 14 } else { 5 };
    v.push(ColumnDef::new("CPU%", cpu_w, true, Some(SortKey::Cpu)));
    v
}

fn small_columns() -> Vec<ColumnDef> {
    vec![
        ColumnDef::new("PID", 7, true, Some(SortKey::Pid)),
        ColumnDef::new("User", 8, false, Some(SortKey::User)),
        ColumnDef::new("CPU%", 5, true, Some(SortKey::Cpu)),
        ColumnDef::new("Mem", 7, true, Some(SortKey::Mem)),
        ColumnDef::new("Command", 0, false, Some(SortKey::Name)),
    ]
}

fn draw_procs(buf: &mut Buffer, area: Rect, app: &mut App, sys: &System) {
    let threads: u64 = sys.processes.iter().map(|p| p.threads as u64).sum();
    let right = format!(
        "filter: {}{}  ‹ {} ›  {}",
        app.filter,
        if app.filtering { "_" } else { "" },
        sort_name(app.sort),
        if app.tree() { "tree" } else { "list" }
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
    let inner = Panel::new("processes", theme::rgb(theme::PROC[1]))
        .right(&right)
        .bottom(&bottom)
        .render(area, buf);
    let cols = proc_columns(inner.width, !sys.gpu.devices.is_empty());
    proc_table(buf, inner, app, cols, false, &HashMap::new());
}

/// Disegna la tabella in `area` (riga 0 = intestazione) e registra le zone cliccabili.
/// Dati GPU per processo mostrati solo nella tabella della sezione GPU.
#[derive(Debug, Clone, Default)]
pub(crate) struct GpuAgg {
    pub devs: String,
    pub kind: WorkKind,
    pub enc: f64,
    pub dec: f64,
}

/// Cella percentuale: numero colorato, con barretta se la colonna è larga.
fn pct_cell(v: f64, width: u16, g: Grad) -> Line<'static> {
    let c = theme::grad(g, v / 100.0);
    if width < 14 {
        return Line::styled(format!("{v:.1}"), fg(c));
    }
    let mut spans = vec![span(format!("{v:>5.1} "), fg(c))];
    spans.extend(Meter::new(v / 100.0, g).spans(8));
    Line::from(spans)
}

pub(crate) fn proc_table(
    buf: &mut Buffer,
    area: Rect,
    app: &mut App,
    cols: Vec<ColumnDef>,
    small: bool,
    extra: &HashMap<u32, GpuAgg>,
) {
    if area.height < 2 || area.width == 0 {
        return;
    }
    app.page = (area.height - 1) as usize;
    app.offset = follow(app.selected, app.offset, app.page, app.view.len());
    app.hits.table = Some(area);
    app.hits.cols = cols.clone();
    let end = (app.offset + app.page).min(app.view.len());
    let rows: Vec<Vec<Line<'static>>> = app.view[app.offset..end]
        .iter()
        .map(|(idx, prefix)| {
            let p = &app.rows[*idx];
            cols.iter()
                .map(|c| match c.title {
                    "PID" => Line::from(p.pid.to_string()),
                    "Name" => Line::from(vec![
                        span(prefix.clone(), dim()),
                        span(p.name.clone(), fg(theme::WHITE)),
                    ]),
                    "User" => Line::styled(p.user.clone(), dim()),
                    "Thr" => Line::from(p.threads.to_string()),
                    "Mem" => Line::from(fmt::bytes(p.rss)),
                    "CPU%" if small || c.width < 14 => Line::styled(
                        format!("{:.1}", p.cpu_percent),
                        fg(pct_color(p.cpu_percent)),
                    ),
                    "CPU%" => pct_cell(p.cpu_percent, c.width, theme::CPU),
                    "GPU%" => pct_cell(p.gpu_percent, c.width, theme::GPU),
                    "VRAM" if p.gpu_mem == 0 => Line::styled("—", dim()),
                    "VRAM" => Line::from(fmt::bytes(p.gpu_mem)),
                    "DEV" => Line::styled(
                        extra
                            .get(&p.pid)
                            .map_or("—".to_string(), |e| e.devs.clone()),
                        dim(),
                    ),
                    "Type" => {
                        Line::from(extra.get(&p.pid).map_or(WorkKind::None, |e| e.kind).label())
                    }
                    "ENC%" => {
                        Line::from(format!("{:.0}", extra.get(&p.pid).map_or(0.0, |e| e.enc)))
                    }
                    "DEC%" => {
                        Line::from(format!("{:.0}", extra.get(&p.pid).map_or(0.0, |e| e.dec)))
                    }
                    // Comando: nella tabella compatta è l'unica colonna di testo, con prefisso albero.
                    _ if small => Line::from(vec![
                        span(prefix.clone(), dim()),
                        span(p.cmdline.clone(), fg(theme::FG)),
                    ]),
                    _ => Line::styled(p.cmdline.clone(), dim()),
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
    .render(area, buf);
}

// ---------------------------------------------------------------- compatto

fn draw_small(buf: &mut Buffer, area: Rect, app: &mut App, sys: &System) {
    // Con una GPU c'è una riga in più (dopo MEM).
    let g = u16::from(!sys.gpu.devices.is_empty());
    let [info, table] =
        Layout::vertical([Constraint::Length(5 + g), Constraint::Min(2)]).areas(area);
    let bold = |c: Color| fg(c).add_modifier(Modifier::BOLD);
    let w = info.width;
    let mw = (w / 4).clamp(8, 24);

    // CPU
    let cpu = &sys.cpu;
    put(buf, info.x, info.y, "CPU", bold(theme::rgb(theme::CPU[0])));
    Meter::new(cpu.total_usage / 100.0, theme::CPU)
        .render(Rect::new(info.x + 4, info.y, mw, 1), buf);
    let mut x = info.x + 4 + mw + 1;
    let pct = format!("{:>3.0}%", cpu.total_usage);
    put(buf, x, info.y, &pct, fg(pct_color(cpu.total_usage)));
    x += 5;
    if let Some(t) = sys.cpu_temp {
        let (s, c) = temp_text(t);
        put(buf, x, info.y, &s, fg(c));
        x += width(&s) as u16 + 1;
    }
    if let Some(avg) = avg_ghz(sys) {
        let s = format!("{avg:.2}GHz");
        put(buf, x, info.y, &s, dim());
        x += width(&s) as u16 + 1;
    }
    let end = info.x + w;
    if x + 2 < end {
        let vals: Vec<f64> = cpu.cores.iter().map(|c| c.usage / 100.0).collect();
        let n = vals.len().min((end - x) as usize);
        Sparkline {
            values: &vals[..n],
            gradient: theme::CPU,
        }
        .render(Rect::new(x, info.y, n as u16, 1), buf);
    }

    if info.height > 1 {
        // MEM
        let m = &sys.mem;
        put(
            buf,
            info.x,
            info.y + 1,
            "MEM",
            bold(theme::rgb(theme::MEM[1])),
        );
        Meter::new(frac(m.used, m.total), theme::USED)
            .render(Rect::new(info.x + 4, info.y + 1, mw, 1), buf);
        putn(
            buf,
            info.x + 4 + mw + 1,
            info.y + 1,
            format!(
                "{}/{}  swp {}/{}",
                fmt::bytes(m.used),
                fmt::bytes(m.total),
                fmt::bytes(m.swap_used),
                fmt::bytes(m.swap_total)
            ),
            w.saturating_sub(mw + 5) as usize,
            fg(theme::FG),
        );
    }
    if g == 1 && info.height > 2 {
        gpu::draw_compact_line(buf, Rect::new(info.x, info.y + 2, info.width, 1), app, sys);
    }
    if info.height > 2 + g {
        // NET
        put(
            buf,
            info.x,
            info.y + 2 + g,
            "NET",
            bold(theme::rgb(theme::NET[1])),
        );
        if let Some(i) = sys.net.pick(app.net_iface.as_deref()) {
            let empty = (History::raw(1), History::raw(1));
            let (rx, tx) = app.net_hist.get(&i.name).unwrap_or(&empty);
            let scale = fmt::nice_max(rx.max_last(16).max(tx.max_last(16)), 100.0 * 1024.0);
            let sp = |h: &History| {
                h.last_n(8)
                    .into_iter()
                    .map(|v| v / scale)
                    .collect::<Vec<_>>()
            };
            let mut x = info.x + 4;
            let rxs = format!("▼{:<9}", fmt::rate(i.rx_rate));
            put(
                buf,
                x,
                info.y + 2 + g,
                &rxs,
                fg(theme::grad(theme::NET, 0.7)),
            );
            x += width(&rxs) as u16;
            Sparkline {
                values: &sp(rx),
                gradient: theme::NET,
            }
            .render(Rect::new(x, info.y + 2 + g, 8, 1), buf);
            x += 9;
            let txs = format!("▲{:<9}", fmt::rate(i.tx_rate));
            put(
                buf,
                x,
                info.y + 2 + g,
                &txs,
                fg(theme::grad(theme::UP, 0.6)),
            );
            x += width(&txs) as u16;
            Sparkline {
                values: &sp(tx),
                gradient: theme::UP,
            }
            .render(Rect::new(x, info.y + 2 + g, 8, 1), buf);
            x += 9;
            putn(
                buf,
                x,
                info.y + 2 + g,
                &i.name,
                end.saturating_sub(x) as usize,
                dim(),
            );
        }
    }
    if info.height > 3 + g {
        // DSK
        put(
            buf,
            info.x,
            info.y + 3 + g,
            "DSK",
            bold(theme::rgb(theme::DISK[1])),
        );
        let mut s = format!(
            "R {:.1} W {:.1}M ",
            sys.disks.read_rate / 1048576.0,
            sys.disks.write_rate / 1048576.0
        );
        for f in &sys.disks.filesystems {
            s.push_str(&format!(" {} {:.0}%", f.mountpoint, f.used_frac() * 100.0));
        }
        putn(
            buf,
            info.x + 4,
            info.y + 3 + g,
            &s,
            w.saturating_sub(4) as usize,
            fg(theme::FG),
        );
    }
    if info.height > 4 + g {
        put(
            buf,
            info.x,
            info.y + 4 + g,
            "─".repeat(w as usize),
            fg(theme::EMPTY),
        );
    }
    proc_table(buf, table, app, small_columns(), true, &HashMap::new());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn altezza_cpu_dal_contenuto() {
        let a = Rect::new(0, 0, 182, 50);
        // 4 core: 4 righe + intestazione + riga vuota + bordi = 8 (minimo)
        assert_eq!(cpu_panel_height(a, 4, 1), 8);
        // 24 core: cresce con le righe della griglia ma resta entro un terzo dello schermo
        let h24 = cpu_panel_height(a, 24, 2);
        assert!((8..=a.height / 3 + 1).contains(&h24), "{h24}");
        // 128 core: fino a metà schermo
        let h128 = cpu_panel_height(a, 128, 3);
        assert!(h128 > h24 && h128 <= a.height / 2, "{h128}");
        // terminale enorme o con moltissimi core: nessun overflow
        let big = Rect::new(0, 0, 60000, 60000);
        let _ = cpu_panel_height(big, 70000, 5);
        let _ = core_layout(70000, 60000, 60000, 5);
    }
}
