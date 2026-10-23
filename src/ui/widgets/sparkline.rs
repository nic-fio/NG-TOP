// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Sparkline ▁▂▃▄▅▆▇█ con colore per valore.

use super::put;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Span;

use crate::ui::theme::{self, Grad};

pub const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

pub struct Sparkline<'a> {
    pub values: &'a [f64],
    pub gradient: Grad,
}

impl Sparkline<'_> {
    pub fn spans(&self, w: usize) -> Vec<Span<'static>> {
        let tail = &self.values[self.values.len().saturating_sub(w)..];
        let mut out: Vec<Span<'static>> = (0..w - tail.len())
            .map(|_| Span::styled(" ", Style::default()))
            .collect();
        for &v in tail {
            let v = if v.is_finite() {
                v.clamp(0.0, 1.0)
            } else {
                0.0
            };
            let ch = BLOCKS[((v * 8.0) as usize).min(7)];
            out.push(Span::styled(
                ch.to_string(),
                Style::default().fg(theme::grad(self.gradient, v)),
            ));
        }
        out
    }

    /// Disegna allineato a destra (i valori recenti a destra).
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        for (i, s) in self.spans(area.width as usize).into_iter().enumerate() {
            put(buf, area.x + i as u16, area.y, s.content.as_ref(), s.style);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn livelli_e_colori() {
        let area = Rect::new(0, 0, 4, 1);
        let mut buf = Buffer::empty(area);
        Sparkline {
            values: &[0.0, 0.5, 1.0],
            gradient: theme::CPU,
        }
        .render(area, &mut buf);
        assert_eq!(buf[(0, 0)].symbol(), " ");
        assert_eq!(buf[(1, 0)].symbol(), "▁");
        assert_eq!(buf[(2, 0)].symbol(), "▅");
        assert_eq!(buf[(3, 0)].symbol(), "█");
        assert_eq!(buf[(3, 0)].fg, theme::grad(theme::CPU, 1.0));
    }
}
