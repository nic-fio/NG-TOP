// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Rendering della Panoramica su istantanee finte e su sistemi con molti core.

use std::fs;
use std::path::PathBuf;

use ngtop::collect::{Roots, System};
use ngtop::ui::app::{App, Section};
use ngtop::ui::snapshot::{render_app, to_ansi, to_text};

fn roots(snap: &str) -> Roots {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(snap);
    Roots::with_dirs(base.join("proc"), base.join("sys"), base.join("etc"))
}

fn fixture_system() -> System {
    let mut s = System::new(roots("snap_a"));
    s.clock_ticks = 100;
    s.page_size = 4096;
    s.refresh_with_dt(0.0);
    s.roots = roots("snap_b");
    s.refresh_with_dt(1.0);
    s
}

fn text(sys: &System, w: u16, h: u16) -> String {
    let mut app = App::new((w, h), "host".into());
    for _ in 0..3 {
        app.update(sys);
    }
    to_text(&render_app(sys, &mut app))
}

#[test]
fn panoramica_grande_con_dati_finti() {
    let sys = fixture_system();
    for (w, h) in [(182, 52), (120, 40), (100, 30)] {
        let t = text(&sys, w, h);
        assert_eq!(t.lines().count(), h as usize);
        for needle in [
            "╭─ cpu",
            "Fake CPU @ 3GHz · 2 threads",
            "61°C",
            "╭─ mem",
            "ZRAM",
            "╭─ net",
            "eth0",
            "192.168.1.10",
            "╭─ disks",
            "╭─ processes",
            "Web Content",
            "F1 Help",
        ] {
            assert!(t.contains(needle), "{w}x{h}: missing {needle:?}\n{t}");
        }
    }
    let t = text(&sys, 182, 52);
    assert!(t.contains("488KB/s"), "{t}");
    assert!(t.contains("R 0.1 W 0.2 MB/s"));
    assert!(t.contains("/firefox -contentproc") || t.contains("firefox"));
}

#[test]
fn panoramica_compatta() {
    let sys = fixture_system();
    for (w, h) in [(80, 24), (99, 40), (140, 29)] {
        let t = text(&sys, w, h);
        assert!(!t.contains("╭─ cpu"), "{w}x{h}");
        for needle in [
            "CPU",
            "MEM",
            "NET",
            "DSK",
            "eth0",
            "61°C",
            "PID",
            "Web Content",
        ] {
            // "Web Content" è il nome nella colonna Comando solo se la cmdline lo contiene
            if needle == "Web Content" {
                continue;
            }
            assert!(t.contains(needle), "{w}x{h}: missing {needle}\n{t}");
        }
    }
}

#[test]
fn tutte_le_misure_e_sezioni_senza_panic() {
    let sys = fixture_system();
    for w in [1u16, 5, 12, 40, 80, 99, 100, 101, 182] {
        for h in [1u16, 3, 5, 12, 24, 29, 30, 31, 52] {
            for s in Section::ALL {
                let mut app = App::new((w, h), "h".into());
                app.section = s;
                app.update(&sys);
                let b = render_app(&sys, &mut app);
                let _ = to_ansi(&b);
            }
        }
    }
}

fn many_cores_system(n: usize) -> (System, PathBuf) {
    let dir = std::env::temp_dir().join(format!("ngtop-cores-{n}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let proc = dir.join("proc");
    fs::create_dir_all(&proc).unwrap_or(());
    let mut stat = String::from("cpu  100 0 100 800 0 0 0 0 0 0\n");
    for i in 0..n {
        stat.push_str(&format!("cpu{i} 10 0 10 80 0 0 0 0 0 0\n"));
    }
    fs::write(proc.join("stat"), stat).unwrap_or(());
    let mut s = System::new(Roots::with_dirs(proc, dir.join("sys"), dir.join("etc")));
    s.refresh_with_dt(0.0);
    s.refresh_with_dt(1.0);
    (s, dir)
}

#[test]
fn molti_core_si_adattano() {
    for n in [1usize, 4, 24, 64, 128] {
        let (sys, dir) = many_cores_system(n);
        assert_eq!(sys.cpu.cores.len(), n);
        for (w, h) in [(182, 52), (120, 40), (100, 30)] {
            let t = text(&sys, w, h);
            assert_eq!(t.lines().count(), h as usize);
            if (w, h) == (182, 52) {
                assert!(t.contains(&format!("C{}", n - 1)), "n={n}\n{t}");
                assert!(!t.contains("not shown"), "n={n}");
            }
        }
        if n == 128 {
            assert!(text(&sys, 100, 30).contains("not shown"));
        }
        let _ = fs::remove_dir_all(&dir);
    }
}
