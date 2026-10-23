// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Sezione 4 (Rete e dischi) su istantanee finte: tests/fixtures/rete_dischi/{a,b}.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use ngtop::collect::{Roots, System};
use ngtop::ui::app::{App, Section};
use ngtop::ui::snapshot::{render_app, to_ansi, to_text};

fn roots(snap: &str) -> Roots {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/rete_dischi")
        .join(snap);
    Roots {
        proc: base.join("proc"),
        sys: base.join("sys"),
        etc: base.join("etc"),
    }
}

/// Due campioni a 1 s di distanza, con la raccolta delle connessioni attiva.
fn sistema() -> System {
    let mut s = System::new(roots("a"));
    s.clock_ticks = 100;
    s.page_size = 4096;
    s.collect_conns = true;
    s.refresh_with_dt(0.0);
    s.roots = roots("b");
    s.refresh_with_dt(1.0);
    s
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn app_for(sys: &System, w: u16, h: u16) -> App {
    let mut a = App::new((w, h), "h".into());
    a.section = Section::ReteDischi;
    a.update(sys);
    a
}

fn text(sys: &System, app: &mut App) -> String {
    to_text(&render_app(sys, app))
}

#[test]
fn raccoglitori_rete_e_dischi() {
    let s = sistema();
    assert_eq!(s.net.dns, ["192.168.1.1", "1.1.1.1"]);
    assert_eq!(s.net.gateway.as_deref(), Some("192.168.1.1"));
    let eth = s.net.ifaces.iter().find(|i| i.name == "eth0").unwrap();
    assert_eq!(eth.speed_mbps, Some(1000));
    assert_eq!(eth.duplex.as_deref(), Some("full"));
    assert_eq!(eth.ipv6.as_deref(), Some("2001:db8::5/64"));

    let n = s.disks.io.iter().find(|d| d.name == "nvme0n1").unwrap();
    assert_eq!(
        (n.model.as_str(), n.kind),
        ("Samsung SSD 990 PRO 2TB", "NVMe")
    );
    assert_eq!(n.temp, Some(41.0));
    assert!((n.read_iops - 200.0).abs() < 1e-9 && (n.write_iops - 200.0).abs() < 1e-9);
    assert!((n.util - 0.5).abs() < 1e-9);
    assert!((n.latency_ms.unwrap() - 0.5).abs() < 1e-9);
    assert_eq!(n.in_flight, 3);
    let d = s.disks.io.iter().find(|d| d.name == "sda").unwrap();
    assert_eq!((d.kind, d.model.as_str()), ("HDD", "WDC WD40EFRX-68N"));
    assert!((d.util - 0.1).abs() < 1e-9);
    assert!((d.latency_ms.unwrap() - 50.0 / 15.0).abs() < 1e-9);
    assert_eq!(s.disks.io.len(), 2);

    // statvfs in thread: la radice c'è, il mount inesistente è scartato
    let mps: Vec<&str> = s
        .disks
        .filesystems
        .iter()
        .map(|f| f.mountpoint.as_str())
        .collect();
    assert_eq!(mps, ["/", "/tmp"]);
    assert!(
        s.disks
            .filesystems
            .iter()
            .all(|f| !f.unresponsive && f.total > 0)
    );
}

#[test]
fn connessioni_con_processo() {
    let s = sistema();
    let c = &s.conns.conns;
    assert_eq!(c.len(), 4 + 3 + 1);
    let find = |local: &str| c.iter().find(|x| x.local == local).unwrap();
    let est = find("192.168.1.10:51618");
    assert_eq!(
        (est.state, est.remote.as_str()),
        ("ESTAB", "151.101.1.140:443")
    );
    assert_eq!(est.pid, Some(100));
    assert_eq!((est.rx_queue, est.tx_queue), (0x20, 0x10));
    assert_eq!(find("0.0.0.0:22").pid, Some(100));
    assert!(find("0.0.0.0:22").is_listen());
    // socket di un altro utente / del kernel: senza processo
    assert_eq!(find("127.0.0.1:5432").pid, None);
    assert_eq!(find("[::]:80").pid, None);
    assert_eq!(find("192.168.1.10:80").state, "TIME_WAIT");
    // IPv4-mapped e loopback v6
    let m = find("[::ffff:192.168.1.10]:58534");
    assert_eq!(m.remote, "[::ffff:192.168.1.10]:443");
    assert_eq!(find("[::1]:32768").remote, "[::1]:32769");
    // UDP associato al pid 200
    let u = find("192.168.1.10:68");
    assert_eq!((u.proto.label(), u.state, u.pid), ("UDP", "—", Some(200)));
}

#[test]
fn la_raccolta_connessioni_e_spenta_se_la_sezione_non_e_visibile() {
    let mut s = System::new(roots("a"));
    s.refresh_with_dt(0.0);
    assert!(s.conns.conns.is_empty());
    s.collect_conns = true;
    s.refresh_with_dt(1.0);
    assert!(!s.conns.conns.is_empty());
}

#[test]
fn rendering_a_ogni_misura() {
    let s = sistema();
    for (w, h) in [
        (182, 52),
        (120, 40),
        (100, 30),
        (80, 24),
        (60, 12),
        (20, 6),
        (9, 3),
    ] {
        let mut a = app_for(&s, w, h);
        let b = render_app(&s, &mut a);
        assert_eq!(to_text(&b).lines().count(), h as usize);
        let _ = to_ansi(&b);
    }
    let mut a = app_for(&s, 182, 52);
    let t = text(&s, &mut a);
    for needle in [
        "interface eth0",
        "1 Gbit/s full",
        "192.168.1.10",
        "2001:db8::5/64",
        "╭─ interfaces",
        "DNS 192.168.1.1, 1.1.1.1 · gateway 192.168.1.1",
        "╭─ connections",
        "151.101.1.140:443",
        "ESTAB",
        "LISTEN",
        "Web Content",
        "╭─ nvme0n1",
        "Samsung SSD 990 PRO 2TB · NVMe",
        "41°C",
        "╭─ sda",
        "HDD",
        "╭─ filesystems",
        "Mounted on",
        "Z Reset",
    ] {
        // la barra tasti ha "z Reset": confronto senza maiuscole sull'ultimo
        let (hay, nd) = if needle == "Z Reset" {
            (t.to_lowercase(), "z reset".to_string())
        } else {
            (t.clone(), needle.to_string())
        };
        assert!(hay.contains(&nd), "missing {needle:?}\n{t}");
    }
    // a 100x30 restano rete, connessioni e barra tasti
    let mut a = app_for(&s, 100, 30);
    let t = text(&s, &mut a);
    assert!(t.contains("interface") && t.contains("connections"));
}

#[test]
fn tasti_della_sezione() {
    let s = sistema();
    let mut a = app_for(&s, 182, 52);
    let _ = text(&s, &mut a);
    let n_all = a.rd.rows.len();
    assert_eq!(n_all, 8);
    // l nasconde LISTEN (3 tabelle: 0.0.0.0:22, 127.0.0.1:5432, [::]:80)
    a.on_key(key(KeyCode::Char('l')));
    a.update(&s);
    assert_eq!(a.rd.rows.len(), 5);
    assert!(a.rd.rows.iter().all(|r| !r.conn.is_listen()));
    a.on_key(key(KeyCode::Char('l')));
    a.update(&s);
    // c: solo TCP (7), solo UDP (1), poi tutte
    a.on_key(key(KeyCode::Char('c')));
    a.update(&s);
    assert_eq!(a.rd.rows.len(), 7);
    a.on_key(key(KeyCode::Char('c')));
    a.update(&s);
    assert_eq!(a.rd.rows.len(), 1);
    a.on_key(key(KeyCode::Char('c')));
    a.update(&s);
    assert_eq!(a.rd.rows.len(), n_all);
    // F6: cambia colonna di ordinamento (per pid i senza pid vanno in fondo)
    for _ in 0..4 {
        a.on_key(key(KeyCode::F(6)));
    }
    a.update(&s);
    assert!(a.rd.rows[0].conn.pid.is_some());
    assert!(a.rd.rows.last().unwrap().conn.pid.is_none());
    // Su/Giù spostano la selezione e Invio salta al processo
    a.on_key(key(KeyCode::Home));
    a.on_key(key(KeyCode::Down));
    let pid = a.rd.rows[a.rd.sel].conn.pid.unwrap();
    assert!(a.on_key(key(KeyCode::Enter)));
    assert_eq!(a.section, Section::Processi);
    assert_eq!(a.selected_pid, Some(pid));
    // selezione di un pid presente
    let mut a = app_for(&s, 182, 52);
    a.jump_to_pid(100);
    assert_eq!(a.section, Section::Processi);
    assert_eq!(a.selected_pid.map(|p| p == 100), Some(true));
    // i cambia interfaccia
    let mut a = app_for(&s, 182, 52);
    a.on_key(key(KeyCode::Char('i')));
    assert!(a.net_iface.is_some());
    // z azzera i picchi
    a.on_key(key(KeyCode::Char('z')));
    assert!(a.rd.peaks.is_empty());
}
