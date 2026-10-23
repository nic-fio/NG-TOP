// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Disegno di un fotogramma: schede in alto, corpo della sezione, barra dei tasti in basso.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;

use crate::collect::System;
use crate::source::{RemoteState, RemoteStatus};

use super::app::{App, LayoutMode, Section};
use super::fmt;
use super::keys::{
    KEYS_BIG, KEYS_FILTER, KEYS_GPU, KEYS_RETE, KEYS_SERVIZI, KEYS_SMALL, help_lines,
};
use super::sections::{gpu, panoramica, processi, retedischi, servizi};
use super::theme;
use super::widgets::{Dialog, KeyBar, TabBar, put, putn};

pub fn draw(frame: &mut Frame, app: &mut App, sys: &System) {
    let area = frame.area();
    let buf = frame.buffer_mut();
    app.hits.table = None;
    app.hits.tabs.clear();
    if area.width < 10 || area.height < 4 {
        return;
    }
    let [top, body, bottom] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(area);

    draw_tabs(buf, top, app, sys);
    match app.section {
        Section::Panoramica => {
            panoramica::draw(buf, body, app, sys, app.layout == LayoutMode::Grande)
        }
        Section::Gpu => gpu::draw(buf, body, app, sys),
        Section::ReteDischi => retedischi::draw(buf, body, app, sys),
        Section::Processi => processi::draw(buf, body, app, sys),
        Section::Servizi => servizi::draw(buf, body, app, sys),
    }
    let keys = if app.filtering || (app.section == Section::Servizi && app.servizi.searching) {
        KEYS_FILTER
    } else if app.section == Section::Servizi {
        KEYS_SERVIZI
    } else if app.section == Section::Gpu {
        KEYS_GPU
    } else if app.section == Section::ReteDischi {
        KEYS_RETE
    } else if app.section == Section::Processi {
        processi::KEYS_PROC
    } else if app.layout == LayoutMode::Grande {
        KEYS_BIG
    } else {
        KEYS_SMALL
    };
    KeyBar { items: keys }.render(bottom, buf);
    // Esito di segnali e nice: riga di stato al posto della barra dei tasti, per qualche secondo.
    if let Some((text, ok)) = app.proc.status_text() {
        let color = if ok {
            theme::rgb(theme::USED[0])
        } else {
            theme::RED
        };
        status_line(buf, bottom, text, color);
    }
    // Connessione remota persa o fallita: avviso fisso finché dura.
    if let Some(st) = sys.remote_status() {
        match &st.state {
            RemoteState::Connected => {}
            RemoteState::Reconnecting(why) => status_line(
                buf,
                bottom,
                &format!("connection lost — reconnecting… ({})", first_line(why)),
                theme::rgb(theme::USED[1]),
            ),
            RemoteState::Failed(why) => status_line(
                buf,
                bottom,
                &format!("connection failed: {} — press q to quit", first_line(why)),
                theme::RED,
            ),
        }
    }
    if app.help {
        let lines = help_lines(area.width as usize);
        let title = match sys.remote_status() {
            Some(st) => format!("Help — {}", remote_name(&st)),
            None => "Help".to_string(),
        };
        Dialog {
            title: &title,
            lines: &lines,
            buttons: &["Close"],
            selected: 0,
        }
        .render(area, buf);
    } else if let Some(d) = &app.dialog {
        d.render(area, buf);
    }
}

/// Riga di stato in basso, al posto della barra dei tasti.
fn status_line(buf: &mut Buffer, bottom: Rect, text: &str, color: ratatui::style::Color) {
    for x in bottom.x..bottom.x + bottom.width {
        put(buf, x, bottom.y, " ", Style::default());
    }
    putn(
        buf,
        bottom.x + 1,
        bottom.y,
        text,
        bottom.width.saturating_sub(2) as usize,
        Style::default().fg(color),
    );
}

fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or("").trim()
}

/// Nome mostrato per la macchina remota: il suo hostname se noto, altrimenti la destinazione.
fn remote_name(st: &RemoteStatus) -> String {
    match st.hostname.as_deref().filter(|h| !h.is_empty()) {
        Some(h) => h.to_string(),
        None => st.target.clone(),
    }
}

/// Colore della latenza: verde sotto 50 ms, giallo sotto 200, rosso oltre.
pub fn latency_color(ms: u32) -> ratatui::style::Color {
    if ms < 50 {
        theme::rgb(theme::USED[0])
    } else if ms < 200 {
        theme::rgb(theme::USED[1])
    } else {
        theme::RED
    }
}

pub(super) fn interval_text(ms: u64) -> String {
    if ms.is_multiple_of(1000) {
        format!("{}s", ms / 1000)
    } else {
        format!("{ms}ms")
    }
}

fn draw_tabs(buf: &mut Buffer, area: Rect, app: &mut App, sys: &System) {
    let names: Vec<&str> = Section::ALL.iter().map(|s| s.title()).collect();
    let unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let rest = format!(
        "up {} │ load {:.2} {:.2} {:.2} │ every {} │ {}",
        fmt::uptime(sys.uptime.up_secs),
        sys.load.one,
        sys.load.five,
        sys.load.fifteen,
        interval_text(app.interval_ms),
        app.tz.clock(unix)
    );
    let (lead, accent, right);
    match sys.remote_status() {
        Some(st) => {
            lead = format!("⇄ {} ", remote_name(&st));
            accent = match (&st.state, st.latency_ms) {
                (RemoteState::Connected, Some(ms)) => Some((format!("{ms} ms"), latency_color(ms))),
                (RemoteState::Connected, None) => None,
                _ => Some(("offline".to_string(), theme::RED)),
            };
            right = if accent.is_some() {
                format!(" │ {rest}")
            } else {
                format!("│ {rest}")
            };
        }
        None => {
            lead = String::new();
            accent = None;
            right = format!("{} │ {rest}", app.hostname);
        }
    }
    let bar = TabBar {
        tabs: &names,
        active: app.section.index(),
        right: &right,
        lead: &lead,
        accent: accent.as_ref().map(|(t, c)| (t.as_str(), *c)),
    };
    let limit = bar.tabs_end(area);
    app.hits.tabs = bar.tab_ranges(area.x);
    app.hits.tabs.retain(|&(x, w)| x + w <= limit);
    bar.render(area, buf);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collect::Roots;
    use crate::source::{FsStat, LocalRunner, LocalSource, Source};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::ffi::OsString;
    use std::io;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    /// Sorgente locale con uno stato remoto finto.
    struct Finta(RemoteStatus);

    impl Source for Finta {
        fn read(&self, p: &Path) -> io::Result<Vec<u8>> {
            LocalSource.read(p)
        }
        fn read_dir(&self, p: &Path) -> io::Result<Vec<OsString>> {
            LocalSource.read_dir(p)
        }
        fn read_link(&self, p: &Path) -> io::Result<PathBuf> {
            LocalSource.read_link(p)
        }
        fn owner_uid(&self, p: &Path) -> io::Result<u32> {
            LocalSource.owner_uid(p)
        }
        fn statvfs(&self, m: &Path) -> io::Result<FsStat> {
            LocalSource.statvfs(m)
        }
        fn remote_status(&self) -> Option<RemoteStatus> {
            Some(self.0.clone())
        }
    }

    fn stato(latency: Option<u32>, state: RemoteState) -> RemoteStatus {
        RemoteStatus {
            target: "alex@server".into(),
            hostname: Some("srv1".into()),
            latency_ms: latency,
            last_bytes: 0,
            state,
        }
    }

    fn sistema(st: RemoteStatus) -> System {
        let mut sys = System::new(Roots::remote(Arc::new(Finta(st)), Arc::new(LocalRunner)));
        sys.refresh();
        sys
    }

    fn righe(buf: &ratatui::buffer::Buffer) -> Vec<String> {
        super::super::snapshot::to_text(buf)
            .lines()
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn barra_remota_a_due_misure() {
        for (w, h) in [(182, 52), (80, 24)] {
            let sys = sistema(stato(Some(23), RemoteState::Connected));
            let mut app = App::new((w, h), "locale".into());
            app.remote = true;
            app.update(&sys);
            let r = righe(&super::super::snapshot::render_app(&sys, &mut app));
            assert!(r[0].contains("⇄ srv1"), "{w}x{h}: {}", r[0]);
            assert!(r[0].contains("23 ms"), "{w}x{h}: {}", r[0]);
            assert!(!r[0].contains("locale"));
        }
    }

    #[test]
    fn colori_della_latenza() {
        assert_eq!(latency_color(10), theme::rgb(theme::USED[0]));
        assert_eq!(latency_color(49), theme::rgb(theme::USED[0]));
        assert_eq!(latency_color(50), theme::rgb(theme::USED[1]));
        assert_eq!(latency_color(199), theme::rgb(theme::USED[1]));
        assert_eq!(latency_color(200), theme::RED);
    }

    #[test]
    fn connessione_persa_e_fallita_nella_riga_di_stato() {
        let sys = sistema(stato(None, RemoteState::Reconnecting("broken pipe".into())));
        let mut app = App::new((80, 24), "x".into());
        app.update(&sys);
        let r = righe(&super::super::snapshot::render_app(&sys, &mut app));
        assert!(
            r[23].contains("connection lost — reconnecting"),
            "{}",
            r[23]
        );
        assert!(r[0].contains("offline"), "{}", r[0]);
        let sys = sistema(stato(None, RemoteState::Failed("denied".into())));
        let r = righe(&super::super::snapshot::render_app(&sys, &mut app));
        assert!(r[23].contains("connection failed: denied"), "{}", r[23]);
    }

    #[test]
    fn aiuto_con_il_nome_della_macchina() {
        let sys = sistema(stato(Some(5), RemoteState::Connected));
        let mut app = App::new((100, 40), "x".into());
        app.update(&sys);
        app.on_key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
        let r = righe(&super::super::snapshot::render_app(&sys, &mut app));
        assert!(r.iter().any(|l| l.contains("Help — srv1")));
    }

    #[test]
    fn locale_senza_novita() {
        let mut sys = System::new(Roots::local());
        sys.refresh();
        let mut app = App::new((182, 52), "locale".into());
        app.update(&sys);
        let r = righe(&super::super::snapshot::render_app(&sys, &mut app));
        assert!(r[0].contains("locale") && !r[0].contains('⇄'));
    }
}
