// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Barra di celle '■' con gradiente lungo la larghezza (funzione bar di tui.js).

use super::put;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Span;

use crate::ui::theme::{self, Grad};

pub struct Meter {
    /// Frazione riempita 0..1.
    pub frac: f64,
    pub gradient: Grad,
}

impl Meter {
    pub fn new(frac: f64, gradient: Grad) -> Self {
        Meter { frac, gradient }
    }

    /// Celle riempite su `w`.
    pub fn filled(&self, w: usize) -> usize {
        let f = if self.frac.is_finite() {
            self.frac.clamp(0.0, 1.0)
        } else {
            0.0
        };
        ((f * w as f64).round() as usize).min(w)
    }

    pub fn spans(&self, w: usize) -> Vec<Span<'static>> {
        let n = self.filled(w);
        (0..w)
            .map(|i| {
                let c = if i < n {
                    theme::grad(self.gradient, i as f64 / (w.max(2) - 1) as f64)
                } else {
                    theme::EMPTY
                };
                Span::styled("■", Style::default().fg(c))
            })
            .collect()
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        for (i, s) in self.spans(area.width as usize).into_iter().enumerate() {
            put(buf, area.x + i as u16, area.y, "■", s.style);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_celle_e_gradiente() {
        let area = Rect::new(0, 0, 10, 1);
        let mut buf = Buffer::empty(area);
        Meter::new(0.5, theme::CPU).render(area, &mut buf);
        let pieni = (0..10).filter(|&x| buf[(x, 0)].fg != theme::EMPTY).count();
        assert_eq!(pieni, 5);
        assert_eq!(buf[(0, 0)].fg, theme::grad(theme::CPU, 0.0));
        assert_eq!(buf[(9, 0)].fg, theme::EMPTY);
        let mut buf = Buffer::empty(area);
        Meter::new(1.0, theme::CPU).render(area, &mut buf);
        assert_eq!(buf[(9, 0)].fg, theme::grad(theme::CPU, 1.0));
        assert_eq!(buf[(0, 0)].symbol(), "■");
    }

    #[test]
    fn valori_strani() {
        assert_eq!(Meter::new(f64::NAN, theme::CPU).filled(10), 0);
        assert_eq!(Meter::new(9.0, theme::CPU).filled(10), 10);
        assert_eq!(Meter::new(0.5, theme::CPU).spans(0).len(), 0);
        assert_eq!(Meter::new(1.0, theme::CPU).spans(1).len(), 1);
    }
}
