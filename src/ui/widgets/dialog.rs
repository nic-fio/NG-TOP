// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Finestra di dialogo centrata, con bordo rosso, che pulisce lo sfondo.

use super::{put, putn};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use crate::ui::theme;

use super::panel::Panel;

pub struct Dialog<'a> {
    pub title: &'a str,
    pub lines: &'a [String],
    pub buttons: &'a [&'a str],
    pub selected: usize,
}

/// Rettangolo `w`×`h` centrato in `area` (ridotto se non ci sta).
pub fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

impl Dialog<'_> {
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        let widest = self
            .lines
            .iter()
            .map(|l| super::width(l))
            .max()
            .unwrap_or(0);
        let w = (widest.max(30) + 4) as u16;
        let h = self.lines.len() as u16 + 4;
        let r = centered(area, w, h);
        for y in r.y..r.y + r.height {
            for x in r.x..r.x + r.width {
                put(buf, x, y, " ", Style::default());
            }
        }
        let inner = Panel::new(self.title, theme::RED).render(r, buf);
        for (i, l) in self.lines.iter().enumerate() {
            let y = inner.y + i as u16;
            if y < inner.y + inner.height.saturating_sub(1) {
                putn(
                    buf,
                    inner.x + 1,
                    y,
                    l,
                    inner.width.saturating_sub(2) as usize,
                    Style::default().fg(theme::FG),
                );
            }
        }
        let mut x = inner.x + 1;
        let y = inner.y + inner.height.saturating_sub(1);
        for (i, b) in self.buttons.iter().enumerate() {
            let t = format!(" {b} ");
            let style = if i == self.selected {
                Style::default()
                    .fg(theme::DARK)
                    .bg(theme::RED)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::FG)
            };
            if x + super::width(&t) as u16 > inner.x + inner.width {
                break;
            }
            put(buf, x, y, &t, style);
            x += super::width(&t) as u16 + 2;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centrato_e_pulisce() {
        let area = Rect::new(0, 0, 60, 20);
        let mut buf = Buffer::empty(area);
        put(&mut buf, 30, 10, "Z", Style::default());
        let lines = vec!["Invio SIGTERM".to_string()];
        Dialog {
            title: "Segnale",
            lines: &lines,
            buttons: &["Annulla", "Invia"],
            selected: 1,
        }
        .render(area, &mut buf);
        let r = centered(area, 34, 5);
        assert_eq!(buf[(r.x, r.y)].symbol(), "╭");
        assert_eq!(buf[(r.x, r.y)].fg, theme::RED);
        assert_ne!(buf[(30, 10)].symbol(), "Z");
        let btn_row = r.y + 3;
        let found = (r.x..r.x + r.width).any(|x| buf[(x, btn_row)].bg == theme::RED);
        assert!(found);
        assert_eq!(
            centered(Rect::new(0, 0, 10, 3), 50, 9),
            Rect::new(0, 0, 10, 3)
        );
    }
}
