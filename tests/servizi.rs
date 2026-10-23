// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Sezione Servizi con backend finto: rendering, filtri, azioni, assenza di systemd, nessun blocco.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ngtop::collect::services::fake::FakeBackend;
use ngtop::collect::services::{
    Action, ActionError, Backend, BootTimestamps, DENIED_TEXT, JournalLine, Scope, SessionInfo,
    UnitDetail, UnitInfo,
};
use ngtop::collect::{Roots, System};
use ngtop::ui::app::{App, Section};
use ngtop::ui::sections::servizi::{Availability, Servizi};
use ngtop::ui::snapshot::{render_app, to_ansi, to_text};

fn fixture_system() -> System {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/snap_b");
    let mut s = System::new(Roots::with_dirs(
        base.join("proc"),
        base.join("sys"),
        base.join("etc"),
    ));
    s.clock_ticks = 100;
    s.page_size = 4096;
    s.refresh_with_dt(0.0);
    s
}

/// Cartella con `systemd/system` dentro, come /run su un sistema con systemd.
fn fake_run_dir() -> PathBuf {
    let d = std::env::temp_dir().join(format!("ngtop-run-{}", std::process::id()));
    std::fs::create_dir_all(d.join("systemd/system")).unwrap();
    d
}

fn app_with(backend: Arc<dyn Backend>, size: (u16, u16)) -> App {
    let mut app = App::new(size, "host".into());
    app.section = Section::Servizi;
    app.servizi = Servizi::with_backend(backend, fake_run_dir(), PathBuf::from("/nonexistent"));
    app
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// Fa girare il polling finché `cond` è vera (al massimo 5 s).
fn wait(app: &mut App, cond: impl Fn(&App) -> bool) {
    let t = Instant::now();
    while !cond(app) && t.elapsed() < Duration::from_secs(5) {
        app.poll_background();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(cond(app), "data did not arrive in time");
}

fn loaded(a: &App) -> bool {
    let s = &a.servizi;
    s.units.is_some()
        && s.detail.is_some()
        && s.journal.is_some()
        && s.boot.is_some()
        && s.blame.is_some()
        && s.sessions.is_some()
}

#[test]
fn rendering_a_quattro_misure_senza_panic() {
    let sys = fixture_system();
    for (w, h) in [(182, 52), (120, 40), (100, 30), (80, 24), (40, 12), (20, 6)] {
        let mut app = app_with(Arc::new(FakeBackend::demo()), (w, h));
        app.update(&sys);
        let first = to_text(&render_app(&sys, &mut app));
        if w >= 80 {
            assert!(first.contains("loading…"), "{w}x{h}");
        }
        wait(&mut app, loaded);
        let buf = render_app(&sys, &mut app);
        let t = to_text(&buf);
        let _ = to_ansi(&buf);
        assert_eq!(t.lines().count(), h as usize);
        if w >= 80 {
            assert!(t.contains("docker.service"), "{w}x{h}\n{t}");
            assert!(t.contains("All 8"), "{w}x{h}");
            assert!(t.contains("✖ failed"), "{w}x{h}");
            assert!(t.contains("◷ waiting"), "{w}x{h}");
        }
        if w >= 120 && h >= 36 {
            assert!(t.contains("╭─ boot"));
            assert!(t.contains("users & sessions"));
            assert!(t.contains("NetworkManager-wait-online"));
            assert!(t.contains("seat0 tty2 wayland"));
        }
    }
}

#[test]
fn filtri_ricerca_e_selezione() {
    let sys = fixture_system();
    let mut app = app_with(Arc::new(FakeBackend::demo()), (182, 52));
    wait(&mut app, loaded);
    // f cicla i chip: Attivi, Falliti.
    app.on_key(key(KeyCode::Char('f')));
    app.on_key(key(KeyCode::Char('f')));
    let t = to_text(&render_app(&sys, &mut app));
    assert!(t.contains("llama-swap.service"));
    assert!(!t.contains("bluetooth.service"));
    assert!(t.contains("Failed 2"));
    // Ricerca con "/".
    for _ in 0..4 {
        app.on_key(key(KeyCode::Char('f')));
    }
    assert_eq!(app.servizi.visible().len(), 8);
    app.on_key(key(KeyCode::Char('/')));
    for c in "dock".chars() {
        app.on_key(key(KeyCode::Char(c)));
    }
    assert_eq!(app.servizi.visible().len(), 2);
    app.on_key(key(KeyCode::Enter));
    assert!(!app.servizi.searching);
    let t = to_text(&render_app(&sys, &mut app));
    assert!(t.contains("/ dock"));
    // I tasti globali tornano a funzionare: "q" esce.
    app.on_key(key(KeyCode::Char('q')));
    assert!(app.should_quit);
}

#[test]
fn azione_con_conferma_ed_esito() {
    let sys = fixture_system();
    let fake = Arc::new(FakeBackend::demo());
    let mut app = app_with(fake.clone(), (182, 52));
    wait(&mut app, loaded);
    app.on_key(key(KeyCode::Char('r')));
    let t = to_text(&render_app(&sys, &mut app));
    assert!(t.contains("Restart"));
    assert!(t.contains("Confirm"));
    // Annulla (Esc): nessuna azione.
    app.on_key(key(KeyCode::Esc));
    assert!(app.servizi.confirm.is_none());
    // Conferma: Tab sul pulsante "Confirm" e Invio.
    app.on_key(key(KeyCode::Char('r')));
    app.on_key(key(KeyCode::Tab));
    app.on_key(key(KeyCode::Enter));
    wait(&mut app, |a| {
        a.servizi
            .status
            .as_ref()
            .is_some_and(|s| s.text.starts_with("Restart requested"))
    });
    let acts = fake.actions.lock().unwrap().clone();
    assert_eq!(
        acts,
        vec![("bluetooth.service".to_string(), Action::Restart)]
    );
    // "e" su un'unità abilitata propone Disabilita.
    app.on_key(key(KeyCode::Char('e')));
    assert_eq!(
        app.servizi.confirm.as_ref().unwrap().action,
        Action::Disable
    );
}

#[test]
fn permesso_negato_nella_riga_di_stato() {
    let sys = fixture_system();
    let fake = FakeBackend {
        action_error: Some(ActionError::Denied),
        ..FakeBackend::demo()
    };
    let mut app = app_with(Arc::new(fake), (182, 52));
    wait(&mut app, loaded);
    app.on_key(key(KeyCode::Char('t')));
    app.on_key(key(KeyCode::Right));
    app.on_key(key(KeyCode::Enter));
    wait(&mut app, |a| {
        a.servizi.status.as_ref().is_some_and(|s| !s.ok)
    });
    let t = to_text(&render_app(&sys, &mut app));
    assert!(t.contains(DENIED_TEXT), "{t}");
}

#[test]
fn senza_systemd_la_sezione_lo_dice() {
    let sys = fixture_system();
    let mut app = App::new((120, 40), "host".into());
    app.section = Section::Servizi;
    let vuota = std::env::temp_dir().join(format!("ngtop-vuota-{}", std::process::id()));
    std::fs::create_dir_all(&vuota).unwrap();
    app.servizi = Servizi::with_backend(
        Arc::new(FakeBackend::demo()),
        vuota,
        PathBuf::from("/nonexistent"),
    );
    app.update(&sys);
    assert_eq!(app.servizi.avail, Availability::NoSystemd);
    let t = to_text(&render_app(&sys, &mut app));
    assert!(t.contains("Services unavailable: systemd not found"));
    // Il resto funziona: tasti globali e altre sezioni.
    app.on_key(key(KeyCode::Char('/')));
    assert!(!app.servizi.searching);
    app.on_key(key(KeyCode::Char('1')));
    assert_eq!(app.section, Section::Panoramica);
    let t = to_text(&render_app(&sys, &mut app));
    assert!(t.contains("╭─ cpu"));
}

/// Backend lentissimo: l'interfaccia deve restare reattiva.
struct Lento;

impl Backend for Lento {
    fn list_units(&self, _: Scope) -> Result<Vec<UnitInfo>, String> {
        std::thread::sleep(Duration::from_millis(1500));
        Ok(vec![])
    }
    fn unit_detail(&self, _: Scope, _: &str) -> Result<UnitDetail, String> {
        Err("lento".into())
    }
    fn action(&self, _: Scope, _: &str, _: Action) -> Result<(), ActionError> {
        Ok(())
    }
    fn boot_times(&self) -> Result<BootTimestamps, String> {
        Err("lento".into())
    }
    fn sessions(&self) -> Result<Vec<SessionInfo>, String> {
        Err("lento".into())
    }
    fn journal(&self, _: Scope, _: &str, _: usize) -> Result<Vec<JournalLine>, String> {
        Err("lento".into())
    }
    fn blame(&self) -> Result<Vec<(String, f64)>, String> {
        Err("lento".into())
    }
}

#[test]
fn l_interfaccia_non_attende_il_backend() {
    let sys = fixture_system();
    let mut app = app_with(Arc::new(Lento), (120, 40));
    let t = Instant::now();
    for _ in 0..20 {
        app.update(&sys);
        let _ = render_app(&sys, &mut app);
        app.on_key(key(KeyCode::Down));
    }
    assert!(
        t.elapsed() < Duration::from_millis(1000),
        "{:?}",
        t.elapsed()
    );
    let txt = to_text(&render_app(&sys, &mut app));
    assert!(txt.contains("loading…"));
}

/// Conta le chiamate (e quelle simultanee) a dettaglio e registro.
#[derive(Default)]
struct Conta {
    inner: FakeBackend,
    detail_calls: AtomicUsize,
    journal_calls: AtomicUsize,
    inflight: AtomicUsize,
    max_inflight: AtomicUsize,
}

impl Conta {
    fn enter(&self) {
        let n = self.inflight.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_inflight.fetch_max(n, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(80));
    }
    fn leave(&self) {
        self.inflight.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Backend for Conta {
    fn list_units(&self, s: Scope) -> Result<Vec<UnitInfo>, String> {
        self.inner.list_units(s)
    }
    fn unit_detail(&self, s: Scope, n: &str) -> Result<UnitDetail, String> {
        self.detail_calls.fetch_add(1, Ordering::SeqCst);
        self.enter();
        let r = self.inner.unit_detail(s, n);
        self.leave();
        r
    }
    fn action(&self, s: Scope, n: &str, a: Action) -> Result<(), ActionError> {
        self.inner.action(s, n, a)
    }
    fn boot_times(&self) -> Result<BootTimestamps, String> {
        self.inner.boot_times()
    }
    fn sessions(&self) -> Result<Vec<SessionInfo>, String> {
        self.inner.sessions()
    }
    fn journal(&self, s: Scope, n: &str, k: usize) -> Result<Vec<JournalLine>, String> {
        self.journal_calls.fetch_add(1, Ordering::SeqCst);
        self.enter();
        let r = self.inner.journal(s, n, k);
        self.leave();
        r
    }
    fn blame(&self) -> Result<Vec<(String, f64)>, String> {
        self.inner.blame()
    }
}

#[test]
fn tenendo_premuto_giu_non_partono_richieste_a_raffica() {
    let sys = fixture_system();
    let b = Arc::new(Conta {
        inner: FakeBackend::demo(),
        ..Default::default()
    });
    let mut app = app_with(b.clone(), (182, 52));
    app.update(&sys);
    wait(&mut app, loaded);
    let (d0, j0) = (
        b.detail_calls.load(Ordering::SeqCst),
        b.journal_calls.load(Ordering::SeqCst),
    );
    // 7 pressioni ravvicinate (10 ms): attraversano tutte le unità.
    for _ in 0..7 {
        app.on_key(key(KeyCode::Down));
        app.poll_background();
        std::thread::sleep(Duration::from_millis(10));
    }
    let last = app.servizi.selected_name.clone().unwrap();
    assert_eq!(last, "update-notifier.service");
    wait(&mut app, |a| {
        matches!(&a.servizi.detail, Some((n, _)) if *n == last)
            && matches!(&a.servizi.journal, Some((n, _)) if *n == last)
    });
    assert!(b.detail_calls.load(Ordering::SeqCst) - d0 <= 2);
    assert!(b.journal_calls.load(Ordering::SeqCst) - j0 <= 2);
    // Mai due richieste dello stesso tipo insieme (dettaglio e registro possono sovrapporsi).
    assert!(b.max_inflight.load(Ordering::SeqCst) <= 2);
}
