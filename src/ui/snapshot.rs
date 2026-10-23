// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Un solo fotogramma su un backend di prova, stampabile come testo o con colori ANSI.

use std::fmt::Write;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};

use crate::collect::System;

use super::app::{App, Section};
use super::view;

/// Un fotogramma con un solo campione di storico.
pub fn render(sys: &System, w: u16, h: u16, section: Section, hostname: &str) -> Buffer {
    let mut app = App::new((w, h), hostname.to_string());
    app.section = section;
    app.update(sys);
    render_app(sys, &mut app)
}

/// Disegna `app` (già alimentata con gli storici) alla sua dimensione.
pub fn render_app(sys: &System, app: &mut App) -> Buffer {
    let (w, h) = app.size;
    // L'errore del TestBackend è Infallible: il pattern è esaustivo.
    let Ok(mut term) = Terminal::new(TestBackend::new(w, h));
    let Ok(_) = term.draw(|f| view::draw(f, app, sys));
    term.backend().buffer().clone()
}

/// Serie sintetica credibile di `n` valori che oscilla attorno a `base` (±`amp` in proporzione):
/// due onde lente più un po' di rumore deterministico (`seed`).
fn synth(base: f64, amp: f64, n: usize, seed: u64) -> Vec<f64> {
    let mut x = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    (0..n)
        .map(|i| {
            x = x
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let noise = ((x >> 33) as f64 / (1u64 << 31) as f64) - 0.5;
            let t = i as f64;
            let wave = 0.6 * (t * 0.11 + seed as f64).sin() + 0.4 * (t * 0.37).sin();
            base * (1.0 + amp * (wave + 0.5 * noise))
        })
        .collect()
}

/// Per le figure dei manuali (`--snapshot` con `--roots`): sostituisce gli storici dei grafici con
/// ~300 campioni sintetici che oscillano attorno all'ultimo valore (o a un valore plausibile se
/// è zero, perché la fixture è ferma). Non tocca i dati correnti, solo la storia.
pub fn synthesize_history(app: &mut App) {
    const N: usize = 300;
    // (storia, valore tipico se l'ultimo è ~0, ampiezza relativa, seme)
    fn fill(h: &mut super::history::History, typical: f64, amp: f64, seed: u64) {
        let last = h.last_n(1).first().copied().unwrap_or(0.0);
        let base = if last > typical * 0.05 { last } else { typical };
        h.replace(synth(base, amp, N, seed));
    }
    fill(&mut app.cpu_hist, 0.42, 0.7, 1);
    // la memoria cambia poco
    let m = app.mem_hist.last_n(1).first().copied().unwrap_or(0.0);
    if m > 0.0 {
        app.mem_hist.replace(synth(m, 0.02, N, 2));
    }
    let mut seed = 10;
    for (rx, tx) in app.net_hist.values_mut() {
        seed += 2;
        fill(rx, 4.0e6, 0.9, seed);
        fill(tx, 0.8e6, 0.9, seed + 1);
    }
    for h in app.disk_hist.values_mut() {
        seed += 1;
        fill(h, 6.0e6, 0.9, seed);
    }
    for g in app.gpu_hist.values_mut() {
        seed += 4;
        let had_use = g.usage.max_last(5) > 0.0 || g.temp.max_last(5) > 0.0;
        if had_use {
            fill(&mut g.usage, 0.55, 0.5, seed);
            let v = g.vram.last_n(1).first().copied().unwrap_or(0.0);
            if v > 0.0 {
                g.vram.replace(synth(v, 0.02, N, seed + 1));
            }
            for (h, s) in [(&mut g.temp, seed + 2), (&mut g.power, seed + 3)] {
                let v = h.last_n(1).first().copied().unwrap_or(0.0);
                if v > 0.0 {
                    h.replace(synth(v, 0.08, N, s));
                }
            }
        }
    }
    let p = app.proc.cpu_hist.last_n(1).first().copied().unwrap_or(0.0);
    if p > 0.0 {
        app.proc.cpu_hist.replace(synth(p, 0.5, N, 99));
    }
}

/// Testo semplice, una riga per riga del buffer (senza spazi finali).
pub fn to_text(buf: &Buffer) -> String {
    let a = buf.area;
    let mut out = String::new();
    for y in 0..a.height {
        let mut line = String::new();
        for x in 0..a.width {
            let c = &buf[(x + a.x, y + a.y)];
            // Nel testo semplice le celle vuote dei meter si distinguono dalle piene.
            if c.symbol() == "■" && c.fg == super::theme::EMPTY {
                line.push('·');
            } else {
                line.push_str(c.symbol());
            }
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

/// Con sequenze SGR truecolor per colori e grassetto.
pub fn to_ansi(buf: &Buffer) -> String {
    let a = buf.area;
    let mut out = String::new();
    for y in 0..a.height {
        let mut cur: (Color, Color, bool) = (Color::Reset, Color::Reset, false);
        for x in 0..a.width {
            let c = &buf[(x + a.x, y + a.y)];
            let want = (c.fg, c.bg, c.modifier.contains(Modifier::BOLD));
            if want != cur {
                out.push_str("\x1b[0");
                if want.2 {
                    out.push_str(";1");
                }
                for (color, base) in [(want.0, 38), (want.1, 48)] {
                    if let Color::Rgb(r, g, b) = color {
                        let _ = write!(out, ";{base};2;{r};{g};{b}");
                    }
                }
                out.push('m');
                cur = want;
            }
            out.push_str(c.symbol());
        }
        out.push_str("\x1b[0m\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collect::Roots;

    #[test]
    fn fotogramma_non_vuoto_e_senza_panic_a_ogni_misura() {
        let mut sys = System::new(Roots::default());
        sys.refresh();
        for (w, h) in [
            (182, 52),
            (100, 30),
            (99, 29),
            (80, 24),
            (20, 6),
            (10, 4),
            (5, 3),
            (1, 1),
        ] {
            for s in Section::ALL {
                let b = render(&sys, w, h, s, "host");
                let t = to_text(&b);
                assert_eq!(t.lines().count(), h as usize);
                let _ = to_ansi(&b);
            }
        }
        let t = to_text(&render(&sys, 182, 52, Section::Panoramica, "host"));
        assert!(t.contains("╭─ cpu"));
        assert!(t.contains("╭─ mem"));
        assert!(t.contains("╭─ net"));
        assert!(t.contains("╭─ disks"));
        assert!(t.contains("╭─ processes"));
        let c = to_text(&render(&sys, 80, 24, Section::Panoramica, "host"));
        assert!(!c.contains("╭─ cpu"));
        assert!(c.contains("MEM"));
        let g = to_text(&render(&sys, 120, 40, Section::Servizi, "host"));
        // Subito dopo l'avvio: "caricamento…" oppure, senza systemd, il messaggio di assenza.
        assert!(g.contains("loading…") || g.contains("systemd not found"));
        // La sezione GPU esiste sempre: pannelli oppure "Nessuna GPU rilevata".
        let g = to_text(&render(&sys, 120, 40, Section::Gpu, "host"));
        assert!(g.contains("GPU"));
    }
}
