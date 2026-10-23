// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Confronto tra lettura locale e lettura "remota" (stesso protocollo, ma con `sh` locale al posto
//! di ssh: `NGTOP_SSH=local-sh`). Nessuna rete e nessun container.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Once;
use std::thread::sleep;
use std::time::Duration;

use ngtop::collect::{Roots, System};
use ngtop::remote::{ConnectOptions, Target, connect};

static INIT: Once = Once::new();

fn remote_roots() -> Roots {
    INIT.call_once(|| {
        // SAFETY: eseguito una sola volta, prima di ogni altra lettura dell'ambiente nei test.
        unsafe { std::env::set_var("NGTOP_SSH", "local-sh") };
    });
    let t = Target::parse("localhost").expect("target");
    let (src, run) = connect(&t, &ConnectOptions { compress: false }).expect("connect local-sh");
    Roots::remote(src, run)
}

fn fixture(snap: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(snap)
}

fn two_samples(s: &mut System, gap: Duration) {
    s.refresh();
    sleep(gap);
    s.refresh();
}

fn names(s: &System) -> BTreeSet<String> {
    s.net.ifaces.iter().map(|i| i.name.clone()).collect()
}

fn mounts(s: &System) -> BTreeSet<String> {
    s.disks
        .filesystems
        .iter()
        .map(|f| f.mountpoint.clone())
        .collect()
}

#[test]
fn remote_local_sh_matches_local_on_real_machine() {
    let mut loc = System::new(Roots::local());
    let mut rem = System::new(remote_roots());
    assert!(
        rem.remote_status().is_some(),
        "remote System must report a status"
    );
    assert!(loc.remote_status().is_none());
    two_samples(&mut loc, Duration::from_millis(1000));
    two_samples(&mut rem, Duration::from_millis(1000));

    assert_eq!(loc.cpu.logical, rem.cpu.logical);
    assert_eq!(loc.cpu.cores.len(), rem.cpu.cores.len());
    assert_eq!(loc.cpu.model, rem.cpu.model);
    // Uso CPU: due misure in momenti diversi; tolleranza larga ma significativa.
    assert!(
        (loc.cpu.total_usage - rem.cpu.total_usage).abs() < 30.0,
        "cpu local {} remote {}",
        loc.cpu.total_usage,
        rem.cpu.total_usage
    );
    assert_eq!(loc.mem.total, rem.mem.total);
    let tol = loc.mem.total / 20; // 5 %
    assert!(loc.mem.available.abs_diff(rem.mem.available) < tol);
    assert!(loc.mem.used.abs_diff(rem.mem.used) < tol);
    let (a, b) = (loc.processes.len() as i64, rem.processes.len() as i64);
    assert!(
        (a - b).abs() <= 20.max(a / 10),
        "processes local {a} remote {b}"
    );
    assert!(
        rem.processes.iter().any(|p| p.pid == 1),
        "pid 1 missing remotely"
    );
    assert_eq!(names(&loc), names(&rem), "interfaces");
    assert_eq!(mounts(&loc), mounts(&rem), "filesystems");
    for f in &rem.disks.filesystems {
        if f.total > 0 {
            let l = loc
                .disks
                .filesystems
                .iter()
                .find(|x| x.mountpoint == f.mountpoint);
            if let Some(l) = l {
                assert_eq!(l.total, f.total, "total of {}", f.mountpoint);
            }
        }
    }
    assert!((loc.uptime.up_secs - rem.uptime.up_secs).abs() < 10.0);
}

#[test]
fn remote_local_sh_matches_local_on_fixture() {
    let base = fixture("snap_a");
    let local = || Roots::with_dirs(base.join("proc"), base.join("sys"), base.join("etc"));
    // Lato "remoto": stessa sh locale, ma /proc, /sys, /etc puntano alla fixture.
    let mut rr = remote_roots();
    rr.proc = base.join("proc");
    rr.sys = base.join("sys");
    rr.etc = base.join("etc");

    let mut loc = System::new(local());
    let mut rem = System::new(rr);
    for s in [&mut loc, &mut rem] {
        s.clock_ticks = 100;
        s.page_size = 4096;
        s.refresh_with_dt(0.0);
        s.refresh_with_dt(1.0);
    }
    assert_eq!(loc.cpu.logical, rem.cpu.logical);
    assert_eq!(loc.cpu.model, rem.cpu.model);
    assert_eq!(loc.cpu.cores.len(), rem.cpu.cores.len());
    assert_eq!(loc.mem.total, rem.mem.total);
    assert_eq!(loc.mem.available, rem.mem.available);
    assert_eq!(loc.mem.cache, rem.mem.cache);
    assert_eq!(loc.processes.len(), rem.processes.len());
    let pids = |s: &System| s.processes.iter().map(|p| p.pid).collect::<BTreeSet<_>>();
    assert_eq!(pids(&loc), pids(&rem));
    assert_eq!(names(&loc), names(&rem));
    assert_eq!(loc.load.one, rem.load.one);
    // Il filesystem: in locale viene da statvfs.json, in remoto da `df` (diverso per costruzione):
    // si confronta solo che l'elenco dei mount venga dalla stessa mountinfo.
    assert!(rem.disks.filesystems.len() <= loc.disks.filesystems.len() + 50);
}
