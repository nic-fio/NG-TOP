// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Tabella dei processi: colonne configurabili, intestazione ordinabile, riga selezionata, scorrimento.

use super::{put, style_area};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;

use crate::collect::process::SortKey;
use crate::ui::theme;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnDef {
    pub title: &'static str,
    /// Larghezza fissa; 0 = occupa lo spazio rimasto (al massimo una colonna).
    pub width: u16,
    pub right: bool,
    pub sort: Option<SortKey>,
}

impl ColumnDef {
    pub const fn new(title: &'static str, width: u16, right: bool, sort: Option<SortKey>) -> Self {
        ColumnDef {
            title,
            width,
            right,
            sort,
        }
    }
}

/// Posizioni (x relativa, larghezza) delle colonne in `width` celle, separate da uno spazio.
pub fn column_spans(cols: &[ColumnDef], width: u16) -> Vec<(u16, u16)> {
    let gaps = cols.len().saturating_sub(1) as u16;
    let fixed: u16 = cols.iter().map(|c| c.width).sum();
    let flex = width.saturating_sub(fixed).saturating_sub(gaps);
    let mut x = 0u16;
    let mut out = Vec::with_capacity(cols.len());
    for c in cols {
        let w = if c.width == 0 { flex } else { c.width };
        let w = w.min(width.saturating_sub(x));
        out.push((x, w));
        x = x.saturating_add(w).saturating_add(1);
    }
    out
}

/// Indice della colonna sotto l'ascissa relativa `x`.
pub fn column_at(cols: &[ColumnDef], width: u16, x: u16) -> Option<usize> {
    column_spans(cols, width)
        .iter()
        .position(|&(cx, cw)| x >= cx && x < cx + cw)
}

/// Nuovo offset di scorrimento perché la riga selezionata resti visibile.
pub fn follow(selected: usize, offset: usize, visible: usize, len: usize) -> usize {
    if visible == 0 || len == 0 {
        return 0;
    }
    let mut o = offset;
    if selected < o {
        o = selected;
    } else if selected >= o + visible {
        o = selected + 1 - visible;
    }
    o.min(len.saturating_sub(visible))
}

fn line_width(l: &Line) -> usize {
    l.spans.iter().map(|s| super::width(&s.content)).sum()
}

/// Scrive la riga in `w` celle (troncata), allineata a destra se richiesto.
fn put_line(buf: &mut Buffer, x: u16, y: u16, w: u16, line: &Line, right: bool) {
    let total = line_width(line);
    let mut cx = x;
    if right && total < w as usize {
        cx += w - total as u16;
    }
    let end = x.saturating_add(w);
    for span in &line.spans {
        for ch in span.content.chars() {
            // Avanza di tante celle quanto è largo il carattere; quelli larghi che non
            // entrano interi nella colonna non vengono disegnati.
            let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0) as u16;
            if cw == 0 {
                continue;
            }
            if cx.saturating_add(cw) > end {
                return;
            }
            put(buf, cx, y, ch.to_string(), line.style.patch(span.style));
            cx += cw;
        }
    }
}

pub struct ProcTable<'a> {
    pub cols: &'a [ColumnDef],
    /// Solo le righe visibili (dall'offset in poi).
    pub rows: &'a [Vec<Line<'static>>],
    /// Riga selezionata relativa a `rows`.
    pub selected: Option<usize>,
    /// Colonna ordinata e verso (true = decrescente).
    pub sort: Option<(SortKey, bool)>,
}

impl ProcTable<'_> {
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let spans = column_spans(self.cols, area.width);
        let head = Style::default().fg(theme::DIM);
        let head_sel = Style::default()
            .fg(theme::WHITE)
            .add_modifier(Modifier::BOLD);
        for (c, &(cx, cw)) in self.cols.iter().zip(&spans) {
            let sorted = self.sort.filter(|(k, _)| c.sort == Some(*k));
            let (title, style) = match sorted {
                Some((_, desc)) => (
                    format!("{}{}", c.title, if desc { "▼" } else { "▲" }),
                    head_sel,
                ),
                None => (c.title.to_string(), head),
            };
            put_line(
                buf,
                area.x + cx,
                area.y,
                cw,
                &Line::styled(title, style),
                c.right,
            );
        }
        for (i, row) in self.rows.iter().enumerate() {
            let y = area.y + 1 + i as u16;
            if y >= area.y + area.height {
                break;
            }
            for ((cell, c), &(cx, cw)) in row.iter().zip(self.cols).zip(&spans) {
                put_line(buf, area.x + cx, y, cw, cell, c.right);
            }
            if self.selected == Some(i) {
                style_area(
                    buf,
                    Rect::new(area.x, y, area.width, 1),
                    Style::default().bg(theme::SEL_BG),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cols() -> Vec<ColumnDef> {
        vec![
            ColumnDef::new("PID", 5, true, Some(SortKey::Pid)),
            ColumnDef::new("Nome", 0, false, Some(SortKey::Name)),
            ColumnDef::new("CPU%", 5, true, Some(SortKey::Cpu)),
        ]
    }

    #[test]
    fn colonne_e_click() {
        let c = cols();
        assert_eq!(column_spans(&c, 30), [(0, 5), (6, 18), (25, 5)]);
        assert_eq!(column_at(&c, 30, 2), Some(0));
        assert_eq!(column_at(&c, 30, 5), None);
        assert_eq!(column_at(&c, 30, 10), Some(1));
        assert_eq!(column_at(&c, 30, 26), Some(2));
        assert_eq!(column_at(&c, 30, 40), None);
        // troppo stretta: non va in panic
        let s = column_spans(&c, 3);
        assert!(s.iter().all(|&(x, w)| x + w <= 3 || w == 0));
    }

    #[test]
    fn scorrimento_segue_la_selezione() {
        assert_eq!(follow(0, 0, 5, 20), 0);
        assert_eq!(follow(5, 0, 5, 20), 1);
        assert_eq!(follow(19, 1, 5, 20), 15);
        assert_eq!(follow(2, 15, 5, 20), 2);
        // selezione oltre il fondo → ricondotta
        assert_eq!(follow(99, 0, 5, 20), 15);
        assert_eq!(follow(3, 9, 0, 20), 0);
        assert_eq!(follow(0, 3, 5, 0), 0);
    }

    #[test]
    fn caratteri_larghi_non_sbordano() {
        let cols = vec![
            ColumnDef::new("A", 3, false, None),
            ColumnDef::new("B", 3, false, None),
        ];
        let rows = vec![vec![Line::from("日本"), Line::from("x")]];
        let area = Rect::new(0, 0, 7, 2);
        let mut buf = Buffer::empty(area);
        ProcTable {
            cols: &cols,
            rows: &rows,
            selected: None,
            sort: None,
        }
        .render(area, &mut buf);
        assert_eq!(buf[(0, 1)].symbol(), "日");
        // "本" non entra nella cella 2..3 e non deve invadere la colonna accanto
        assert_ne!(buf[(2, 1)].symbol(), "本");
        assert_eq!(buf[(4, 1)].symbol(), "x");
    }

    #[test]
    fn disegno_intestazione_selezione_e_allineamento() {
        let c = cols();
        let rows = vec![
            vec![Line::from("1"), Line::from("init"), Line::from("0.5")],
            vec![
                Line::from("22"),
                Line::from("molto-lungo-nome-troppo"),
                Line::from("9"),
            ],
        ];
        let area = Rect::new(0, 0, 30, 3);
        let mut buf = Buffer::empty(area);
        ProcTable {
            cols: &c,
            rows: &rows,
            selected: Some(1),
            sort: Some((SortKey::Cpu, true)),
        }
        .render(area, &mut buf);
        let row = |y: u16| {
            (0..30)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        };
        assert!(row(0).contains("CPU%▼"));
        assert!(row(0).starts_with("  PID Nome"));
        assert!(row(1).starts_with("    1 init"));
        assert!(row(1).ends_with("0.5"));
        assert_eq!(buf[(0, 2)].bg, theme::SEL_BG);
        assert_eq!(buf[(29, 2)].bg, theme::SEL_BG);
        assert_ne!(buf[(0, 1)].bg, theme::SEL_BG);
        assert_eq!(buf[(24, 2)].symbol(), " ");
        assert_eq!(buf[(6, 0)].fg, theme::DIM);
        assert_eq!(buf[(25, 0)].fg, theme::WHITE);
    }
}
