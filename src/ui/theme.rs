// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Palette e gradienti del mockup (oggetto G in mockup/tui.js), colori truecolor.

use ratatui::style::Color;

/// Gradiente: elenco di colori 0xRRGGBB tra cui si interpola.
pub type Grad = &'static [u32];

pub const CPU: Grad = &[0x3fb950, 0xd29922, 0xf85149];
pub const MEM: Grad = &[0x1b7c83, 0x39c5cf, 0xa5f3fc];
pub const USED: Grad = &[0x56d364, 0xe3b341, 0xf85149];
pub const GPU: Grad = &[0x8957e5, 0xbc8cff, 0xf778ba];
pub const NET: Grad = &[0x1f6feb, 0x58a6ff, 0xa5d6ff];
pub const UP: Grad = &[0x8250df, 0xd2a8ff, 0xf0b6ff];
pub const DISK: Grad = &[0x9e6a03, 0xe3b341, 0xf0883e];
pub const WR: Grad = &[0xbd561d, 0xf0883e, 0xffa657];
pub const TEMP: Grad = &[0x58a6ff, 0x3fb950, 0xd29922, 0xf85149];
pub const PROC: Grad = &[0x9a4b10, 0xf0883e, 0xffc680];

pub const FG: Color = Color::Rgb(0xc9, 0xd1, 0xd9);
pub const DIM: Color = Color::Rgb(0x6e, 0x76, 0x81);
pub const WHITE: Color = Color::Rgb(0xf0, 0xf6, 0xfc);
pub const DARK: Color = Color::Rgb(0x0b, 0x0f, 0x14);
pub const EMPTY: Color = Color::Rgb(0x21, 0x26, 0x2d);
pub const RED: Color = Color::Rgb(0xf8, 0x51, 0x49);
pub const BLUE: Color = Color::Rgb(0x58, 0xa6, 0xff);
pub const CYAN: Color = Color::Rgb(0x39, 0xc5, 0xcf);
pub const KEY_LABEL_BG: Color = Color::Rgb(0x1c, 0x27, 0x33);
/// #1f6feb al 40% su sfondo scuro.
pub const SEL_BG: Color = Color::Rgb(19, 53, 106);

pub fn rgb(h: u32) -> Color {
    Color::Rgb((h >> 16) as u8, (h >> 8) as u8, h as u8)
}

/// Colore del gradiente alla posizione `t` (0..1, valori fuori scala o NaN vengono ricondotti).
pub fn grad(g: &[u32], t: f64) -> Color {
    match g {
        [] => FG,
        [one] => rgb(*one),
        _ => {
            let t = if t.is_finite() {
                t.clamp(0.0, 1.0)
            } else {
                0.0
            };
            let p = t * (g.len() - 1) as f64;
            let i = (p.floor() as usize).min(g.len() - 2);
            let f = p - i as f64;
            let (a, b) = (g[i], g[i + 1]);
            let mix = |sh: u32| {
                let (x, y) = (((a >> sh) & 0xff) as f64, ((b >> sh) & 0xff) as f64);
                (x + (y - x) * f).round() as u32
            };
            rgb((mix(16) << 16) | (mix(8) << 8) | mix(0))
        }
    }
}

// ---------------------------------------------------------------- profondità di colore

/// Profondità di colore del terminale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    /// 24 bit (RGB diretto).
    True,
    /// Palette xterm a 256 colori.
    Ansi256,
    /// 16 colori di base (console Linux).
    Ansi16,
}

impl ColorMode {
    /// Sceglie la modalità: impostazione esplicita, altrimenti da COLORTERM e TERM.
    pub fn detect(
        setting: crate::config::ColorSetting,
        colorterm: Option<&str>,
        term: Option<&str>,
    ) -> ColorMode {
        use crate::config::ColorSetting;
        match setting {
            ColorSetting::Truecolor => return ColorMode::True,
            ColorSetting::C256 => return ColorMode::Ansi256,
            ColorSetting::Auto => {}
        }
        let ct = colorterm.unwrap_or("").to_ascii_lowercase();
        let t = term.unwrap_or("").to_ascii_lowercase();
        if ct.contains("truecolor") || ct.contains("24bit") {
            ColorMode::True
        } else if t == "linux" {
            ColorMode::Ansi16
        } else if t.contains("truecolor") || t.contains("direct") {
            ColorMode::True
        } else {
            ColorMode::Ansi256
        }
    }
}

const CUBE: [i32; 6] = [0, 95, 135, 175, 215, 255];

fn dist2(a: (i32, i32, i32), b: (i32, i32, i32)) -> i32 {
    (a.0 - b.0).pow(2) + (a.1 - b.1).pow(2) + (a.2 - b.2).pow(2)
}

/// Indice (16–255) della palette xterm più vicino a un colore RGB: cubo 6×6×6 o scala di grigi.
pub fn xterm256(r: u8, g: u8, b: u8) -> u8 {
    let want = (i32::from(r), i32::from(g), i32::from(b));
    let near = |v: i32| -> usize { (0..6).min_by_key(|&i| (CUBE[i] - v).abs()).unwrap_or(0) };
    let (ri, gi, bi) = (near(want.0), near(want.1), near(want.2));
    let cube = (CUBE[ri], CUBE[gi], CUBE[bi]);
    let cube_idx = 16 + 36 * ri + 6 * gi + bi;
    let avg = (want.0 + want.1 + want.2) / 3;
    let gi = (((avg - 8 + 5) / 10).clamp(0, 23)) as usize;
    let level = 8 + 10 * gi as i32;
    if dist2(want, (level, level, level)) < dist2(want, cube) {
        (232 + gi) as u8
    } else {
        cube_idx as u8
    }
}

/// I 16 colori di base (valori tipici di xterm) con il loro colore ratatui.
const BASE16: [((i32, i32, i32), Color); 16] = [
    ((0, 0, 0), Color::Black),
    ((205, 0, 0), Color::Red),
    ((0, 205, 0), Color::Green),
    ((205, 205, 0), Color::Yellow),
    ((0, 0, 238), Color::Blue),
    ((205, 0, 205), Color::Magenta),
    ((0, 205, 205), Color::Cyan),
    ((229, 229, 229), Color::Gray),
    ((127, 127, 127), Color::DarkGray),
    ((255, 0, 0), Color::LightRed),
    ((0, 255, 0), Color::LightGreen),
    ((255, 255, 0), Color::LightYellow),
    ((92, 92, 255), Color::LightBlue),
    ((255, 0, 255), Color::LightMagenta),
    ((0, 255, 255), Color::LightCyan),
    ((255, 255, 255), Color::White),
];

/// Colore di base più vicino; `only8` limita ai primi 8 (sfondi della console Linux).
pub fn ansi16(r: u8, g: u8, b: u8, only8: bool) -> Color {
    let want = (i32::from(r), i32::from(g), i32::from(b));
    let n = if only8 { 8 } else { 16 };
    BASE16[..n]
        .iter()
        .min_by_key(|(rgb, _)| dist2(want, *rgb))
        .map_or(Color::Reset, |(_, c)| *c)
}

/// Converte un colore alla modalità richiesta (i colori non RGB restano invariati).
pub fn convert_color(c: Color, mode: ColorMode, is_bg: bool) -> Color {
    match (c, mode) {
        (Color::Rgb(r, g, b), ColorMode::Ansi256) => Color::Indexed(xterm256(r, g, b)),
        (Color::Rgb(r, g, b), ColorMode::Ansi16) => ansi16(r, g, b, is_bg),
        _ => c,
    }
}

/// Passaggio finale su un fotogramma: porta tutti i colori alla modalità del terminale.
pub fn convert_buffer(buf: &mut ratatui::buffer::Buffer, mode: ColorMode) {
    if mode == ColorMode::True {
        return;
    }
    for cell in buf.content.iter_mut() {
        cell.fg = convert_color(cell.fg, mode, false);
        cell.bg = convert_color(cell.bg, mode, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estremi_e_centro() {
        assert_eq!(grad(CPU, 0.0), Color::Rgb(0x3f, 0xb9, 0x50));
        assert_eq!(grad(CPU, 1.0), Color::Rgb(0xf8, 0x51, 0x49));
        assert_eq!(grad(CPU, 0.5), Color::Rgb(0xd2, 0x99, 0x22));
        assert_eq!(grad(CPU, -3.0), grad(CPU, 0.0));
        assert_eq!(grad(CPU, f64::NAN), grad(CPU, 0.0));
        assert_eq!(grad(&[0xffffff], 0.4), Color::Rgb(255, 255, 255));
        assert_eq!(grad(&[], 0.4), FG);
    }

    #[test]
    fn conversione_256() {
        // grigi e primari
        assert_eq!(xterm256(0, 0, 0), 16);
        assert_eq!(xterm256(255, 255, 255), 231);
        assert_eq!(xterm256(255, 0, 0), 196);
        assert_eq!(xterm256(0, 255, 0), 46);
        assert_eq!(xterm256(0, 0, 255), 21);
        // 0x3fb950 (verde) → cubo (63,185,80) ≈ (95,175,95) = 16+36*1+6*3+1 = 71
        assert_eq!(xterm256(0x3f, 0xb9, 0x50), 71);
        // un grigio medio va sulla scala dei grigi
        let g = xterm256(128, 128, 128);
        assert!((232..=255).contains(&g), "{g}");
        // un colore del cubo esatto resta nel cubo
        assert_eq!(xterm256(95, 135, 175), 16 + 36 + 12 + 3);
        // la distanza dal colore originale è piccola per tutti i colori del tema
        for gname in [CPU, MEM, USED, GPU, NET, UP, DISK, WR, TEMP, PROC] {
            for h in gname {
                let (r, g, b) = ((h >> 16) as u8, (h >> 8) as u8, *h as u8);
                let idx = xterm256(r, g, b);
                assert!(idx >= 16);
            }
        }
    }

    #[test]
    fn conversione_16_e_buffer() {
        assert_eq!(ansi16(0, 0, 0, false), Color::Black);
        assert_eq!(ansi16(250, 10, 10, false), Color::LightRed);
        // gli sfondi usano solo gli 8 colori di base
        assert_eq!(ansi16(250, 10, 10, true), Color::Red);
        assert_eq!(ansi16(0x39, 0xc5, 0xcf, true), Color::Cyan);
        assert_eq!(
            convert_color(Color::Reset, ColorMode::Ansi16, false),
            Color::Reset
        );
        assert_eq!(
            convert_color(Color::Rgb(1, 2, 3), ColorMode::True, false),
            Color::Rgb(1, 2, 3)
        );
        let mut buf = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 2, 1));
        buf[(0, 0)].fg = Color::Rgb(255, 0, 0);
        buf[(0, 0)].bg = SEL_BG;
        convert_buffer(&mut buf, ColorMode::Ansi256);
        assert_eq!(buf[(0, 0)].fg, Color::Indexed(196));
        assert!(matches!(buf[(0, 0)].bg, Color::Indexed(_)));
        assert_eq!(buf[(1, 0)].fg, Color::Reset);
        buf[(1, 0)].fg = Color::Rgb(255, 0, 0);
        convert_buffer(&mut buf, ColorMode::Ansi16);
        assert_eq!(buf[(1, 0)].fg, Color::LightRed);
        // True non tocca nulla
        buf[(1, 0)].fg = Color::Rgb(9, 9, 9);
        convert_buffer(&mut buf, ColorMode::True);
        assert_eq!(buf[(1, 0)].fg, Color::Rgb(9, 9, 9));
    }

    #[test]
    fn rilevamento_modalita() {
        use crate::config::ColorSetting::*;
        let d = ColorMode::detect;
        assert_eq!(
            d(Auto, Some("truecolor"), Some("xterm-256color")),
            ColorMode::True
        );
        assert_eq!(d(Auto, Some("24bit"), None), ColorMode::True);
        assert_eq!(d(Auto, None, Some("xterm-256color")), ColorMode::Ansi256);
        assert_eq!(d(Auto, None, None), ColorMode::Ansi256);
        assert_eq!(d(Auto, None, Some("linux")), ColorMode::Ansi16);
        assert_eq!(d(Auto, None, Some("xterm-direct")), ColorMode::True);
        assert_eq!(
            d(C256, Some("truecolor"), Some("linux")),
            ColorMode::Ansi256
        );
        assert_eq!(d(Truecolor, None, Some("linux")), ColorMode::True);
    }
}
