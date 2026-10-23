// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Riga in alto: marchio e schede a sinistra, informazioni a destra.

use super::put;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use crate::ui::theme;

const BRAND: &str = "ngtop";
const VERSION: &str = " 1.0 │ ";

pub struct TabBar<'a> {
    pub tabs: &'a [&'a str],
    pub active: usize,
    /// Testo a destra (mostrato solo se c'è spazio).
    pub right: &'a str,
}

fn tab_text(i: usize, name: &str) -> String {
    format!(" {} {} ", i + 1, name)
}

impl TabBar<'_> {
    /// Intervalli (x assoluta, larghezza) di ogni scheda, per il clic del mouse.
    pub fn tab_ranges(&self, x0: u16) -> Vec<(u16, u16)> {
        let mut x = x0 + (super::width(BRAND) + super::width(VERSION)) as u16;
        self.tabs
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let w = super::width(&tab_text(i, t)) as u16;
                let r = (x, w);
                x += w;
                r
            })
            .collect()
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 {
            return;
        }
        let end = area.x + area.width;
        put(
            buf,
            area.x,
            area.y,
            BRAND,
            Style::default()
                .fg(theme::BLUE)
                .add_modifier(Modifier::BOLD),
        );
        let dim = Style::default().fg(theme::DIM);
        put(buf, area.x + BRAND.len() as u16, area.y, VERSION, dim);
        let mut last = area.x;
        for (i, ((x, w), name)) in self
            .tab_ranges(area.x)
            .into_iter()
            .zip(self.tabs)
            .enumerate()
        {
            if x + w > end {
                break;
            }
            let style = if i == self.active {
                Style::default()
                    .fg(theme::DARK)
                    .bg(theme::BLUE)
                    .add_modifier(Modifier::BOLD)
            } else {
                dim
            };
            put(buf, x, area.y, tab_text(i, name), style);
            last = x + w;
        }
        let rw = super::width(self.right) as u16;
        if rw > 0 && last + 2 + rw <= end {
            put(
                buf,
                end - rw,
                area.y,
                self.right,
                Style::default().fg(theme::FG),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schede_e_zone_di_clic() {
        let bar = TabBar {
            tabs: &["Uno", "Due"],
            active: 1,
            right: "host",
        };
        let r = bar.tab_ranges(0);
        assert_eq!(r, [(12, 7), (19, 7)]);
        let area = Rect::new(0, 0, 40, 1);
        let mut buf = Buffer::empty(area);
        bar.render(area, &mut buf);
        assert_eq!(buf[(0, 0)].symbol(), "n");
        assert_eq!(buf[(20, 0)].symbol(), "2");
        assert_eq!(buf[(20, 0)].bg, theme::BLUE);
        assert_ne!(buf[(13, 0)].bg, theme::BLUE);
        assert_eq!(buf[(39, 0)].symbol(), "t");
    }

    #[test]
    fn destra_omessa_se_stretto() {
        let bar = TabBar {
            tabs: &["Uno", "Due"],
            active: 0,
            right: "una scritta molto lunga",
        };
        let area = Rect::new(0, 0, 30, 1);
        let mut buf = Buffer::empty(area);
        bar.render(area, &mut buf);
        assert_eq!(buf[(29, 0)].symbol(), " ");
    }
}
