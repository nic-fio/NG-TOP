// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Pannello con bordo arrotondato colorato e titoli sul bordo (.panel/.t/.tr/.tb del mockup).

use super::put;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use crate::ui::theme;

pub struct Panel<'a> {
    pub title: &'a str,
    pub right: &'a str,
    pub bottom: &'a str,
    pub color: Color,
}

impl<'a> Panel<'a> {
    pub fn new(title: &'a str, color: Color) -> Self {
        Panel {
            title,
            right: "",
            bottom: "",
            color,
        }
    }

    pub fn right(mut self, t: &'a str) -> Self {
        self.right = t;
        self
    }

    pub fn bottom(mut self, t: &'a str) -> Self {
        self.bottom = t;
        self
    }

    /// Disegna il bordo e i titoli; restituisce l'area interna.
    pub fn render(&self, area: Rect, buf: &mut Buffer) -> Rect {
        if area.width < 2 || area.height < 2 {
            return Rect::new(area.x, area.y, 0, 0);
        }
        let line = Style::default().fg(self.color);
        let (x0, y0) = (area.x, area.y);
        let (x1, y1) = (area.x + area.width - 1, area.y + area.height - 1);
        for x in x0 + 1..x1 {
            put(buf, x, y0, "─", line);
            put(buf, x, y1, "─", line);
        }
        for y in y0 + 1..y1 {
            put(buf, x0, y, "│", line);
            put(buf, x1, y, "│", line);
        }
        put(buf, x0, y0, "╭", line);
        put(buf, x1, y0, "╮", line);
        put(buf, x0, y1, "╰", line);
        put(buf, x1, y1, "╯", line);

        let dim = Style::default().fg(theme::DIM);
        let max = (area.width as usize).saturating_sub(6);
        // Il titolo sinistro ha la precedenza; destro e inferiore si accorciano o spariscono.
        let mut title_end = x0 + 2;
        if !self.title.is_empty() {
            let t = super::clip(&format!(" {} ", self.title), max);
            put(buf, x0 + 2, y0, &t, line.add_modifier(Modifier::BOLD));
            title_end = x0 + 2 + super::width(&t) as u16;
        }
        if !self.right.is_empty() {
            // Spazio tra la fine del titolo (più 2 celle di distanza) e l'angolo destro.
            let avail = (x1.saturating_sub(1) + 1).saturating_sub(title_end + 2) as usize;
            let t = super::clip(&format!(" {} ", self.right), avail.min(max));
            let w = super::width(&t) as u16;
            if w >= 6 || t == format!(" {} ", self.right) {
                put(buf, x1.saturating_sub(w + 1).max(x0 + 1), y0, &t, dim);
            }
        }
        if !self.bottom.is_empty() {
            let t = super::clip(&format!(" {} ", self.bottom), max);
            put(buf, x0 + 2, y1, &t, dim);
        }
        Rect::new(area.x + 1, area.y + 1, area.width - 2, area.height - 2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bordo_e_titoli() {
        let area = Rect::new(0, 0, 20, 4);
        let mut buf = Buffer::empty(area);
        let inner = Panel::new("CPU", theme::rgb(0x3fb950))
            .right("dx")
            .bottom("giu")
            .render(area, &mut buf);
        assert_eq!(inner, Rect::new(1, 1, 18, 2));
        assert_eq!(buf[(0, 0)].symbol(), "╭");
        assert_eq!(buf[(19, 3)].symbol(), "╯");
        assert_eq!(buf[(3, 0)].symbol(), "C");
        assert!(buf[(3, 0)].modifier.contains(Modifier::BOLD));
        assert_eq!(buf[(3, 3)].symbol(), "g");
        assert_eq!(buf[(3, 3)].fg, theme::DIM);
        assert_eq!(buf[(15, 0)].symbol(), "d");
        assert_eq!(buf[(3, 0)].fg, Color::Rgb(0x3f, 0xb9, 0x50));
    }

    #[test]
    fn area_minuscola() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 3, 3));
        let r = Panel::new("x", theme::DIM).render(Rect::new(0, 0, 1, 1), &mut buf);
        assert_eq!(r.width, 0);
    }
}
