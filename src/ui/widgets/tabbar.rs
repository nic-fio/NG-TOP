// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Riga in alto: marchio e schede a sinistra, informazioni a destra.

use super::put;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use crate::ui::theme;

const BRAND: &str = "ngtop";
// Presa da Cargo.toml, così non resta indietro.
const VERSION: &str = concat!(" ", env!("CARGO_PKG_VERSION"), " │ ");

pub struct TabBar<'a> {
    pub tabs: &'a [&'a str],
    pub active: usize,
    /// Testo a destra (mostrato solo se c'è spazio).
    pub right: &'a str,
    /// In remoto: macchina e latenza, scritte subito prima di `right` (la latenza ha un colore).
    pub lead: &'a str,
    pub accent: Option<(&'a str, ratatui::style::Color)>,
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

    /// Ultima colonna (esclusa) usabile dalle schede: in remoto lascia posto a macchina e latenza.
    pub fn tabs_end(&self, area: Rect) -> u16 {
        let end = area.x + area.width;
        let lead =
            (super::width(self.lead) + self.accent.map_or(0, |(t, _)| super::width(t))) as u16;
        if lead == 0 {
            end
        } else {
            end.saturating_sub(lead + 2)
        }
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
            if x + w > self.tabs_end(area) {
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
        let fg = Style::default().fg(theme::FG);
        let lw = super::width(self.lead) as u16;
        let aw = self.accent.map_or(0, |(t, _)| super::width(t) as u16);
        let rw = super::width(self.right) as u16;
        // Se non c'è posto per tutto, in remoto restano almeno macchina e latenza.
        let rw = if lw + aw > 0 && last + 2 + lw + aw + rw > end {
            0
        } else {
            rw
        };
        let total = lw + aw + rw;
        if total > 0 && last + 2 + total <= end {
            let mut x = end - total;
            put(buf, x, area.y, self.lead, fg);
            x += lw;
            if let Some((t, c)) = self.accent {
                put(buf, x, area.y, t, Style::default().fg(c));
                x += aw;
            }
            put(buf, x, area.y, self.right, fg);
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
            lead: "",
            accent: None,
        };
        // Le schede partono dopo marchio e versione, qualunque sia la versione.
        let o = (super::super::width(BRAND) + super::super::width(VERSION)) as u16;
        let r = bar.tab_ranges(0);
        assert_eq!(r, [(o, 7), (o + 7, 7)]);
        let area = Rect::new(0, 0, 40, 1);
        let mut buf = Buffer::empty(area);
        bar.render(area, &mut buf);
        assert_eq!(buf[(0, 0)].symbol(), "n");
        assert_eq!(buf[(o + 8, 0)].symbol(), "2");
        assert_eq!(buf[(o + 8, 0)].bg, theme::BLUE);
        assert_ne!(buf[(o + 1, 0)].bg, theme::BLUE);
        assert_eq!(buf[(39, 0)].symbol(), "t");
    }

    #[test]
    fn destra_omessa_se_stretto() {
        let bar = TabBar {
            tabs: &["Uno", "Due"],
            active: 0,
            right: "una scritta molto lunga",
            lead: "",
            accent: None,
        };
        let area = Rect::new(0, 0, 30, 1);
        let mut buf = Buffer::empty(area);
        bar.render(area, &mut buf);
        assert_eq!(buf[(29, 0)].symbol(), " ");
    }
}
