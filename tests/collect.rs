// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Test dei raccoglitori su due istantanee finte (tests/fixtures/snap_a e snap_b).

use std::path::PathBuf;

use ngtop::collect::process::tree_order;
use ngtop::collect::{Roots, System};

fn roots(snap: &str) -> Roots {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(snap);
    Roots {
        proc: base.join("proc"),
        sys: base.join("sys"),
        etc: base.join("etc"),
    }
}

/// Sistema con due campioni: A poi B, a 1 s di distanza.
fn sistema() -> System {
    let mut s = System::new(roots("snap_a"));
    s.clock_ticks = 100;
    s.page_size = 4096;
    s.want_io = true;
    s.refresh_with_dt(0.0);
    s.roots = roots("snap_b");
    s.refresh_with_dt(1.0);
    s
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn primo_campione_a_zero() {
    let mut s = System::new(roots("snap_a"));
    s.clock_ticks = 100;
    s.page_size = 4096;
    s.refresh_with_dt(0.0);
    assert_eq!(s.cpu.total_usage, 0.0);
    assert!(s.cpu.cores.iter().all(|c| c.usage == 0.0));
    assert!(s.processes.iter().all(|p| p.cpu_percent == 0.0));
    assert_eq!(s.processes.len(), 7);
}

#[test]
fn cpu() {
    let s = sistema();
    assert!(near(s.cpu.total_usage, 75.0));
    assert_eq!(s.cpu.cores.len(), 2);
    assert!(near(s.cpu.cores[0].usage, 80.0));
    assert!(near(s.cpu.cores[1].usage, 70.0));
    assert_eq!(s.cpu.cores[0].freq_mhz, Some(3600));
    assert_eq!(s.cpu.cores[1].freq_mhz, None);
    assert_eq!(s.cpu.model, "Fake CPU @ 3GHz");
    assert_eq!(s.cpu.logical, 2);
    assert_eq!(s.cpu.physical, Some(1));
}

#[test]
fn memoria_e_carico() {
    let s = sistema();
    let k = 1024;
    assert_eq!(s.mem.total, 8_000_000 * k);
    assert_eq!(s.mem.available, 4_000_000 * k);
    assert_eq!(s.mem.used, 4_000_000 * k);
    assert_eq!(s.mem.cache, 2_500_000 * k);
    assert_eq!(s.mem.swap_used, 500_000 * k);
    assert_eq!(s.mem.zswap, Some(1000 * k));
    assert_eq!(s.load.one, 0.52);
    assert_eq!((s.load.running, s.load.total), (2, 345));
    assert_eq!(s.uptime.up_secs, 12345.67);
}

#[test]
fn processi() {
    let s = sistema();
    // Il processo 200 esiste solo in snap_a: sparito, nessun errore.
    assert_eq!(s.processes.len(), 6);
    assert!(s.processes.iter().all(|p| p.pid != 200));
    let get = |pid| s.processes.iter().find(|p| p.pid == pid).unwrap();

    let w = get(100);
    assert_eq!(w.name, "Web Content");
    assert_eq!(w.user, "alex");
    assert_eq!(w.cmdline, "/usr/lib/firefox/firefox -contentproc");
    assert_eq!(w.rss, 2560 * 4096);
    // 80 tick / 100 Hz / 1 s / 2 core = 40 %
    assert!(near(w.cpu_percent, 40.0));
    assert_eq!(w.io_read_rate, Some(4096.0));
    assert_eq!(w.io_write_rate, Some(1000.0));
    assert_eq!((w.nice, w.priority, w.threads, w.ppid), (0, 20, 2, 50));

    assert_eq!(get(60).name, "a) b");
    assert_eq!(get(1).user, "root");
    assert_eq!(get(1).cmdline, "[systemd]");
    assert_eq!(get(300).user, "4242");
    // senza permesso su io
    assert_eq!(get(50).io_read, None);
    assert_eq!(get(50).io_read_rate, None);
}

#[test]
fn albero() {
    let s = sistema();
    let t: Vec<(u32, String)> = tree_order(&s.processes)
        .into_iter()
        .map(|(i, p)| (s.processes[i].pid, p))
        .collect();
    let want = [
        (1, ""),
        (50, "├─ "),
        (60, "│  ├─ "),
        (100, "│  └─ "),
        (70, "└─ "),
        (300, ""),
    ];
    assert_eq!(t.len(), want.len());
    for (a, b) in t.iter().zip(want) {
        assert_eq!((a.0, a.1.as_str()), b);
    }
}

#[test]
fn pid_riusato() {
    // Il pid 70 in snap_b ha un altro starttime: è un processo nuovo, niente differenze.
    let s = sistema();
    let p = s.processes.iter().find(|p| p.pid == 70).unwrap();
    assert_eq!(p.cpu_percent, 0.0);
    assert_eq!(p.io_read_rate, None);
    assert_eq!(p.io_write_rate, None);
}

#[test]
fn rete_dischi_sensori() {
    let s = sistema();
    let eth = s.net.ifaces.iter().find(|i| i.name == "eth0").unwrap();
    assert!(near(eth.rx_rate, 500_000.0));
    assert!(near(eth.tx_rate, 200_000.0));
    assert!(near(eth.rx_peak, 500_000.0));
    assert_eq!(eth.up, Some(true));
    assert_eq!(eth.ipv4.as_deref(), Some("192.168.1.10"));
    assert_eq!(s.net.default.as_deref(), Some("eth0"));
    assert_eq!(s.net.pick(None).map(|i| i.name.as_str()), Some("eth0"));
    // solo sda (fisico): 200 settori letti e 400 scritti in 1 s
    assert_eq!(s.disks.io.len(), 1);
    assert!(near(s.disks.read_rate, 200.0 * 512.0));
    assert!(near(s.disks.write_rate, 400.0 * 512.0));
    let mps: Vec<&str> = s
        .disks
        .filesystems
        .iter()
        .map(|f| f.mountpoint.as_str())
        .collect();
    assert_eq!(mps, ["/", "/tmp"]);
    assert!(s.disks.filesystems[0].total > 0);
    assert_eq!(s.cpu_temp, Some(61.0));
    assert_eq!(s.zram, Some((1_000_000, 400_000)));
}
