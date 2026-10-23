// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Barra dei tasti in basso, stile htop.

use super::put;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::ui::theme;

pub struct KeyBar<'a> {
    pub items: &'a [(&'a str, &'a str)],
}

impl KeyBar<'_> {
    /// Disegna solo voci intere: quelle che non entrano si saltano, l'ultima (es. "q Quit")
    /// resta se possibile.
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        let end = area.x + area.width;
        let key = Style::default().fg(theme::DARK).bg(theme::CYAN);
        let label = Style::default().fg(theme::FG).bg(theme::KEY_LABEL_BG);
        let item_w = |k: &str, l: &str, tail: usize| {
            super::width(&format!(" {k}")) + super::width(&format!(" {l}")) + tail
        };
        let Some(((lk, ll), rest)) = self.items.split_last() else {
            return;
        };
        let last_w = item_w(lk, ll, 0) as u16;
        // Se l'ultima voce non entra nemmeno da sola non si riserva spazio per lei.
        let reserve = if last_w <= area.width { last_w } else { 0 };
        let mut x = area.x;
        let draw = |buf: &mut Buffer, x: &mut u16, k: &str, l: &str, tail: &str| {
            for (text, style) in [(format!(" {k}"), key), (format!(" {l}{tail}"), label)] {
                for ch in text.chars() {
                    let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0) as u16;
                    if *x + cw > end {
                        return;
                    }
                    put(buf, *x, area.y, ch.to_string(), style);
                    *x += cw;
                }
            }
        };
        for (k, l) in rest {
            if x + item_w(k, l, 2) as u16 + reserve <= end {
                draw(buf, &mut x, k, l, "  ");
            }
        }
        if x + last_w <= end {
            draw(buf, &mut x, lk, ll, "");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colori_tasto_ed_etichetta() {
        let area = Rect::new(0, 0, 12, 1);
        let mut buf = Buffer::empty(area);
        KeyBar {
            items: &[("q", "Esci"), ("x", "Troppo lungo")],
        }
        .render(area, &mut buf);
        assert_eq!(buf[(1, 0)].symbol(), "q");
        assert_eq!(buf[(1, 0)].bg, theme::CYAN);
        assert_eq!(buf[(3, 0)].symbol(), "E");
        assert_eq!(buf[(3, 0)].bg, theme::KEY_LABEL_BG);
    }
}
