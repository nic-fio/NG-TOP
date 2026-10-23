// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Grafico braille (U+2800): 2 colonne × 4 punti per cella. Stesso algoritmo di brailleLines in tui.js.

use super::put;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::ui::theme::{self, Grad};

/// Bit dei punti, dal basso verso l'alto.
const UP: [[u8; 4]; 2] = [[0x40, 0x04, 0x02, 0x01], [0x80, 0x20, 0x10, 0x08]];
/// Bit dei punti, dall'alto verso il basso.
const DN: [[u8; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];

/// Righe di testo (dall'alto in basso, con l'indice di riga contato dal lato di base).
/// `vals` ha 2 valori (0..1) per colonna; i mancanti valgono 0.
pub fn braille_rows(vals: &[f64], w: usize, h: usize, down: bool) -> Vec<(String, usize)> {
    let mut out = Vec::with_capacity(h);
    for r in 0..h {
        let rb = if down { r } else { h - 1 - r };
        let mut line = String::with_capacity(w * 3);
        for c in 0..w {
            let mut bits = 0u8;
            for side in 0..2 {
                let v = vals.get(c * 2 + side).copied().unwrap_or(0.0);
                let v = if v.is_finite() {
                    v.clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let level = (v * (h * 4) as f64).round() as usize;
                for k in 0..4 {
                    if level > rb * 4 + k {
                        bits |= if down { DN[side][k] } else { UP[side][k] };
                    }
                }
            }
            line.push(char::from_u32(0x2800 + bits as u32).unwrap_or(' '));
        }
        out.push((line, rb));
    }
    out
}

pub struct BrailleGraph<'a> {
    /// Serie 0..1, la più recente per ultima.
    pub values: &'a [f64],
    pub gradient: Grad,
    /// Disegna verso il basso (grafico a specchio).
    pub down: bool,
}

impl BrailleGraph<'_> {
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        let (w, h) = (area.width as usize, area.height as usize);
        if w == 0 || h == 0 {
            return;
        }
        // Allinea a destra: i valori più recenti sono a destra.
        let n = w * 2;
        let tail = &self.values[self.values.len().saturating_sub(n)..];
        let mut vals = vec![0.0; n - tail.len()];
        vals.extend_from_slice(tail);
        for (r, (line, rb)) in braille_rows(&vals, w, h, self.down).into_iter().enumerate() {
            let color = theme::grad(self.gradient, (rb as f64 + 0.5) / h as f64);
            put(
                buf,
                area.x,
                area.y + r as u16,
                &line,
                Style::default().fg(color),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caratteri_attesi() {
        // 1 cella, h=1: livello 4 a sinistra (tutti i punti sinistri), 0 a destra.
        let r = braille_rows(&[1.0, 0.0], 1, 1, false);
        assert_eq!(r[0].0, "\u{2847}");
        // metà altezza sinistra e destra: 2 punti in basso per lato.
        let r = braille_rows(&[0.5, 0.5], 1, 1, false);
        assert_eq!(r[0].0, "\u{28E4}");
        // verso il basso: i punti in alto.
        let r = braille_rows(&[0.5, 0.5], 1, 1, true);
        assert_eq!(r[0].0, "\u{281B}");
        // vuoto
        assert_eq!(braille_rows(&[], 2, 1, false)[0].0, "\u{2800}\u{2800}");
    }

    #[test]
    fn due_righe_e_indici() {
        let r = braille_rows(&[1.0, 1.0], 1, 2, false);
        assert_eq!(r[0].1, 1);
        assert_eq!(r[1].1, 0);
        // 1.0 su 2 righe = 8 punti: entrambe piene
        assert_eq!(r[0].0, "\u{28FF}");
        assert_eq!(r[1].0, "\u{28FF}");
    }

    #[test]
    fn render_allinea_a_destra_con_colore() {
        let area = Rect::new(0, 0, 3, 2);
        let mut buf = Buffer::empty(area);
        let g = BrailleGraph {
            values: &[1.0, 1.0],
            gradient: theme::CPU,
            down: false,
        };
        g.render(area, &mut buf);
        assert_eq!(buf[(0, 0)].symbol(), "\u{2800}");
        assert_eq!(buf[(2, 1)].symbol(), "\u{28FF}");
        assert_eq!(buf[(2, 0)].symbol(), "\u{28FF}");
        // riga alta (rb=1) più vicina al rosso della bassa
        assert_ne!(buf[(2, 0)].fg, buf[(2, 1)].fg);
        assert_eq!(buf[(2, 0)].fg, theme::grad(theme::CPU, 0.75));
    }
}
