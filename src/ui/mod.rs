// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Interfaccia a terminale: ciclo degli eventi e ripristino del terminale.

pub mod app;
pub mod doc;
pub mod fmt;
pub mod history;
pub mod keys;
pub mod sections;
pub mod snapshot;
pub mod theme;
pub mod tz;
pub mod view;
pub mod widgets;

use std::io::{self, Stdout, stdout};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::collect::{Roots, System};
use crate::config::{self, Config, TempUnit};
use app::App;
use theme::ColorMode;

/// Opzioni di avvio della TUI, già risolte (file di configurazione + riga di comando).
pub struct RunOptions {
    /// Valori effettivi (la riga di comando ha già prevalso).
    pub effective: Config,
    /// Contenuto del file (o default): è ciò che `W` riscrive.
    pub file: Config,
    pub config_path: Option<std::path::PathBuf>,
    pub colors: ColorMode,
    /// Dove leggere: questa macchina (`Roots::local()`) oppure una remota (`Roots::remote(..)`).
    pub roots: Roots,
}

/// Riporta il terminale allo stato normale; ignora gli errori (può essere chiamata più volte).
fn restore_terminal() {
    let _ = execute!(stdout(), DisableMouseCapture, LeaveAlternateScreen);
    let _ = disable_raw_mode();
    let _ = execute!(stdout(), crossterm::cursor::Show);
}

/// Ripristina il terminale quando esce di scope: anche con `?` e con panic (unwind).
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

pub fn hostname(sys: &System) -> String {
    sys.roots
        .src
        .read_text(&sys.roots.proc.join("sys/kernel/hostname"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Sezione Servizi per la macchina in uso (backend remoto a comandi se si è in remoto).
pub fn services_for(sys: &System) -> sections::servizi::Servizi {
    if let Some(st) = sys.remote_status() {
        let mut s = sections::servizi::Servizi::remote(
            sys.roots.src.clone(),
            sys.roots.run.clone(),
            sys.cpu.logical.max(1),
        );
        s.machine = Some((st.target.clone(), st.root));
        s
    } else {
        sections::servizi::Servizi::new()
    }
}

pub fn run(opts: RunOptions) -> io::Result<()> {
    // Con un panic il terminale va ripristinato prima di stampare il messaggio.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        default_hook(info);
    }));

    // SIGTERM/SIGINT/SIGHUP: il ciclo vede il flag ed esce normalmente, ripristinando il terminale.
    let stop = Arc::new(AtomicBool::new(false));
    for sig in [
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGHUP,
    ] {
        // Registrato prima del flag: il primo segnale imposta il flag, il secondo termina subito.
        signal_hook::flag::register_conditional_shutdown(sig, 1, Arc::clone(&stop))?;
        signal_hook::flag::register(sig, Arc::clone(&stop))?;
    }

    // Tutto ciò che può fallire o allocare molto (fuso orario, primo campione) avviene
    // prima di toccare il terminale.
    let mut sys = System::new(opts.roots);
    sys.refresh();
    let size = crossterm::terminal::size().unwrap_or((80, 24));
    let mut app = App::new(size, hostname(&sys));
    app.remote = sys.remote_status().is_some();
    app.proc.act = sys.roots.act.clone();
    app.servizi = services_for(&sys);
    app.tz = tz::Tz::load(&sys.roots);
    let e = &opts.effective;
    app.interval_ms = config::round_interval(e.interval_ms);
    app.left_pct = e.left_width.clamp(20, 70);
    app.config = opts.file.clone();
    app.config_path = opts.config_path.clone();
    fmt::set_fahrenheit(e.temperature == TempUnit::Fahrenheit);
    app.set_section(config::section_from_number(e.section));
    app.update(&sys);

    enable_raw_mode()?;
    let _guard = TerminalGuard;
    execute!(stdout(), EnterAlternateScreen)?;
    if opts.effective.mouse {
        execute!(stdout(), EnableMouseCapture)?;
    }
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    event_loop(&mut terminal, &stop, sys, app, opts.colors)
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    stop: &AtomicBool,
    mut sys: System,
    mut app: App,
    colors: ColorMode,
) -> io::Result<()> {
    // Primo aggiornamento rapido, così le percentuali compaiono subito.
    let mut next_tick = Instant::now() + Duration::from_millis(250);
    let mut dirty = true;

    while !app.should_quit && !stop.load(Ordering::Relaxed) {
        if dirty {
            terminal.draw(|f| {
                view::draw(f, &mut app, &sys);
                // Unico punto di conversione dei colori per i terminali senza truecolor.
                theme::convert_buffer(f.buffer_mut(), colors);
            })?;
            dirty = false;
        }
        // Tetto di 250 ms per accorgersi dei segnali di terminazione.
        let mut timeout = next_tick
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(250));
        // Con risposte dei servizi in attesa si controlla più spesso (senza ridisegnare a vuoto).
        if app.background_active() {
            timeout = timeout.min(Duration::from_millis(100));
        }
        // Nessun ridisegno a vuoto: si ridisegna solo con eventi o allo scadere del tick.
        if event::poll(timeout)? {
            loop {
                dirty |= match event::read()? {
                    Event::Key(k) if k.kind == KeyEventKind::Press => app.on_key(k),
                    Event::Mouse(m) => app.on_mouse(m),
                    Event::Resize(w, h) => {
                        app.resize(w, h);
                        true
                    }
                    _ => false,
                };
                if !event::poll(Duration::ZERO)? {
                    break;
                }
            }
        }
        dirty |= app.poll_background();
        // Un cambio di intervallo vale subito, non dopo il vecchio tick.
        next_tick = next_tick.min(Instant::now() + Duration::from_millis(app.interval_ms));
        if Instant::now() >= next_tick {
            sys.collect_conns = app.section == app::Section::ReteDischi;
            let t0 = Instant::now();
            sys.refresh();
            let took = t0.elapsed();
            app.update(&sys);
            app.adapt_interval(took);
            next_tick = Instant::now() + Duration::from_millis(app.interval_ms);
            dirty = true;
        }
    }
    Ok(())
}
