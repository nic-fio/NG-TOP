// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Componenti riutilizzabili, disegnati direttamente su un `Buffer` di ratatui.

pub mod braille;
pub mod dialog;
pub mod keybar;
pub mod meter;
pub mod panel;
pub mod proctable;
pub mod sparkline;
pub mod tabbar;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Scrive testo ignorando ciò che cade fuori dal buffer (mai panic su aree piccole).
pub fn put(buf: &mut Buffer, x: u16, y: u16, s: impl AsRef<str>, style: Style) {
    putn(buf, x, y, s, usize::MAX, style);
}

/// Come `put` ma al massimo `n` caratteri.
pub fn putn(buf: &mut Buffer, x: u16, y: u16, s: impl AsRef<str>, n: usize, style: Style) {
    let a = buf.area;
    if y < a.y || y >= a.y + a.height || x < a.x || x >= a.x + a.width {
        return;
    }
    let room = (a.x + a.width - x) as usize;
    let text = clip(s.as_ref(), n.min(room));
    buf.set_string(x, y, text, style);
}

/// Larghezza in celle del terminale.
pub fn width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// Tronca a `max` celle (senza spezzare i caratteri larghi).
pub fn clip(s: &str, max: usize) -> String {
    let mut w = 0;
    let mut out = String::new();
    for ch in s.chars() {
        let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
        if w + cw > max {
            break;
        }
        w += cw;
        out.push(ch);
    }
    out
}

/// Applica uno stile solo alla parte di `r` che sta nel buffer.
pub fn style_area(buf: &mut Buffer, r: Rect, style: Style) {
    let r = r.intersection(buf.area);
    buf.set_style(r, style);
}

pub use braille::BrailleGraph;
pub use dialog::Dialog;
pub use keybar::KeyBar;
pub use meter::Meter;
pub use panel::Panel;
pub use proctable::{ColumnDef, ProcTable};
pub use sparkline::Sparkline;
pub use tabbar::TabBar;
