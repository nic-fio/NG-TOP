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
    let (src, run) = connect(
        &t,
        &ConnectOptions {
            compress: false,
            ..Default::default()
        },
    )
    .expect("connect local-sh");
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
        s.collect_conns = true;
        s.clock_ticks = 100;
        s.page_size = 4096;
        s.refresh_with_dt(0.0);
        s.refresh_with_dt(1.0);
    }
    // Uguaglianza piena: la fixture è statica e il tempo è fissato (dt = 1 s).
    assert_eq!(format!("{:?}", loc.cpu), format!("{:?}", rem.cpu), "cpu");
    assert_eq!(format!("{:?}", loc.mem), format!("{:?}", rem.mem), "mem");
    assert_eq!(format!("{:?}", loc.load), format!("{:?}", rem.load), "load");
    assert_eq!(
        format!("{:?}", loc.uptime),
        format!("{:?}", rem.uptime),
        "uptime"
    );
    assert!(!loc.processes.is_empty());
    let sorted = |s: &System| {
        let mut v = s.processes.clone();
        v.sort_by_key(|p| p.pid);
        v
    };
    let (lp, rp) = (sorted(&loc), sorted(&rem));
    assert_eq!(lp.len(), rp.len(), "process count");
    for (a, b) in lp.iter().zip(&rp) {
        // PartialEq di Process: pid, nome, cmdline, utente, rss, stato, thread, cpu, io…
        assert_eq!(a, b, "process {}", a.pid);
    }
    assert!(!loc.net.ifaces.is_empty());
    assert_eq!(format!("{:?}", loc.net), format!("{:?}", rem.net), "net");
    assert_eq!(
        format!("{:?}", loc.disks.io),
        format!("{:?}", rem.disks.io),
        "disk io"
    );
    assert_eq!(loc.disks.read_rate, rem.disks.read_rate);
    assert_eq!(loc.disks.write_rate, rem.disks.write_rate);
    assert_eq!(format!("{:?}", loc.gpu), format!("{:?}", rem.gpu), "gpu");
    assert_eq!(loc.cpu_temp, rem.cpu_temp, "temperature sensor");
    assert_eq!(loc.zram, rem.zram, "zram");
    assert_eq!(
        format!("{:?}", loc.conns.conns),
        format!("{:?}", rem.conns.conns),
        "connections"
    );
    // Filesystem: stessi mount, tipo, sorgente e opzioni. Lo spazio no: in locale viene da
    // statvfs.json, in remoto da `df` sulla macchina vera, quindi i numeri non possono coincidere.
    let fs = |s: &System| {
        s.disks
            .filesystems
            .iter()
            .map(|f| {
                (
                    f.mountpoint.clone(),
                    f.source.clone(),
                    f.fstype.clone(),
                    f.options.clone(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(fs(&loc), fs(&rem), "filesystem list");
}

// ---------------------------------------------------------------------------------------------
// Prove end-to-end delle azioni remote contro un container vero (podman + sshd).
// Si lanciano con `scripts/test-remote.sh --actions` (prepara i container e le variabili) oppure a
// mano con `cargo test --test remote -- --ignored e2e`. Senza le variabili sono no-op: `cargo test`
// resta verde. Le azioni toccano solo `sleep` avviati dal test e il servizio di prova del container.
//   NGTOP_SSH           comando ssh (es. "ssh -F /tmp/.../ssh_config")
//   NGTOP_E2E_HOST      alias ssh del container/utente
//   NGTOP_E2E_SUDO      "1" = connessione con `--sudo` (la password, se serve, va sul tty)
//   NGTOP_E2E_UNIT      servizio di prova (solo container systemd)
//   NGTOP_E2E_FOREIGN_PID  pid di uno `sleep` di un altro utente (avviato dallo script)
//   NGTOP_E2E_NICE_LOWER   "0" = il container non lascia abbassare il nice nemmeno a root (podman rootless)
// ---------------------------------------------------------------------------------------------

mod e2e {
    use std::process::Command;
    use std::thread::sleep;
    use std::time::{Duration, Instant};

    use ngtop::collect::Roots;
    use ngtop::collect::services::{Action, ActionError, Scope, backend_for};
    use ngtop::remote::{ConnectOptions, Target, actions_for, connect};
    use ngtop::source::actions::{ERR_GONE, ERR_PERM, ERR_REUSED};

    struct Env {
        host: String,
        sudo: bool,
        unit: Option<String>,
        foreign: Option<u32>,
    }

    fn env() -> Option<Env> {
        let host = std::env::var("NGTOP_E2E_HOST")
            .ok()
            .filter(|h| !h.is_empty())?;
        if std::env::var("NGTOP_SSH").map_or(true, |s| s.trim().is_empty()) {
            return None;
        }
        Some(Env {
            host,
            sudo: std::env::var("NGTOP_E2E_SUDO").is_ok_and(|v| v == "1"),
            unit: std::env::var("NGTOP_E2E_UNIT")
                .ok()
                .filter(|u| !u.is_empty()),
            foreign: std::env::var("NGTOP_E2E_FOREIGN_PID")
                .ok()
                .and_then(|p| p.parse().ok()),
        })
    }

    /// Esegue un comando sul server con lo stesso ssh di ngtop (utente della connessione, niente sudo).
    fn ssh(e: &Env, cmd: &str) -> String {
        let mut parts = std::env::var("NGTOP_SSH")
            .unwrap()
            .split_whitespace()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let prog = parts.remove(0);
        let out = Command::new(prog)
            .args(parts)
            .arg(&e.host)
            .arg(cmd)
            .output()
            .expect("ssh");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn roots(e: &Env) -> Roots {
        let t = Target::parse(&e.host).expect("target");
        let (src, run) = connect(
            &t,
            &ConnectOptions {
                compress: false,
                sudo: e.sudo,
            },
        )
        .unwrap_or_else(|err| panic!("connect {} (sudo={}): {err}", e.host, e.sudo));
        let act = actions_for(&src);
        Roots::remote(src, run).with_actions(act)
    }

    fn spawn_sleep(e: &Env) -> u32 {
        ssh(e, "sh -c 'sleep 600 </dev/null >/dev/null 2>&1 & echo $!'")
            .parse()
            .expect("pid of sleep")
    }

    fn alive(e: &Env, pid: u32) -> bool {
        ssh(e, &format!("test -d /proc/{pid} && echo y")) == "y"
    }

    fn nice_of(e: &Env, pid: u32) -> String {
        ssh(
            e,
            &format!("sed 's/.*) //' /proc/{pid}/stat | cut -d' ' -f17"),
        )
    }

    fn cleanup(e: &Env, pid: u32) {
        ssh(e, &format!("kill -9 {pid} 2>/dev/null; true"));
    }

    #[test]
    #[ignore = "needs a container: scripts/test-remote.sh --actions"]
    fn e2e_process_signal_nice_reuse_gone_protected() {
        let Some(e) = env() else { return };
        let r = roots(&e);
        let st = r.src.remote_status().expect("remote status");
        assert_eq!(st.root, e.sudo, "RemoteStatus.root must follow --sudo");

        // PID 1 protetto, un normale sleep no.
        assert_eq!(r.act.protected(1), Some("PID 1 (init)"));
        let pid = spawn_sleep(&e);
        assert_eq!(r.act.protected(pid), None);

        let start = r.act.live_starts(&[pid, 4_000_000])[0].expect("live start of sleep");
        assert_eq!(r.act.live_starts(&[4_000_000]), vec![None]);

        // Pid riusato: avvio diverso -> ERR_REUSED e il processo resta vivo.
        let res = r.act.signal(&[(pid, start + 1)], 15);
        assert_eq!(res, vec![Err(ERR_REUSED.to_string())], "reused pid");
        assert!(alive(&e, pid), "sleep must survive a reused-pid signal");

        // Nice: alzare vale per tutti; riabbassare solo con root.
        r.act.set_nice(pid, start, 5).expect("raise nice to 5");
        assert_eq!(nice_of(&e, pid), "5");
        assert!(r.act.set_nice(pid, start + 1, 7).is_err(), "nice reused");
        assert_eq!(nice_of(&e, pid), "5");
        let lower = r.act.set_nice(pid, start, 0);
        if e.sudo && std::env::var("NGTOP_E2E_NICE_LOWER").is_ok_and(|v| v == "0") {
            // Container rootless: nemmeno root puo' abbassare il nice (can_nice usa il namespace iniziale).
            assert!(lower.is_err());
        } else if e.sudo {
            lower.expect("root can lower nice");
            assert_eq!(nice_of(&e, pid), "0");
        } else {
            let err = lower.expect_err("non-root must not lower nice");
            assert!(!err.is_empty());
            assert_eq!(nice_of(&e, pid), "5");
        }

        // Segnale vero: SIGTERM (e il batch con due bersagli, uno inesistente).
        let res = r.act.signal(&[(pid, start), (4_000_000, 1)], 15);
        assert_eq!(res[0], Ok(()));
        assert_eq!(res[1], Err(ERR_GONE.to_string()));
        for _ in 0..20 {
            if !alive(&e, pid) {
                break;
            }
            sleep(Duration::from_millis(250));
        }
        assert!(!alive(&e, pid), "sleep must be gone after SIGTERM");
        // Ormai morto: ERR_GONE.
        assert_eq!(
            r.act.signal(&[(pid, start)], 15),
            vec![Err(ERR_GONE.to_string())]
        );
        assert!(r.act.set_nice(pid, start, 1).is_err());
        cleanup(&e, pid);
    }

    #[test]
    #[ignore = "needs a container: scripts/test-remote.sh --actions"]
    fn e2e_process_of_another_user() {
        let Some(e) = env() else { return };
        let Some(fp) = e.foreign else { return };
        let r = roots(&e);
        let start = r.act.live_starts(&[fp])[0].expect("foreign sleep must be visible");
        let res = r.act.signal(&[(fp, start)], 15);
        if e.sudo {
            assert_eq!(res, vec![Ok(())], "root may signal another user's process");
        } else {
            assert_eq!(res, vec![Err(ERR_PERM.to_string())], "EPERM without sudo");
            assert!(r.act.set_nice(fp, start, 3).is_err());
        }
    }

    fn systemctl(e: &Env, args: &str) -> String {
        ssh(e, &format!("systemctl {args} 2>&1"))
    }

    fn wait_active(e: &Env, unit: &str, want: &str) -> bool {
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(15) {
            if systemctl(e, &format!("is-active {unit}")) == want {
                return true;
            }
            sleep(Duration::from_millis(300));
        }
        false
    }

    #[test]
    #[ignore = "needs the systemd container: scripts/test-remote.sh --actions"]
    fn e2e_service_actions() {
        let Some(e) = env() else { return };
        let Some(unit) = e.unit.clone() else { return };
        let r = roots(&e);
        let b = backend_for(&r.src, &r.run);
        let name = unit.as_str();

        if !e.sudo {
            // Senza --sudo: testo chiaro, e il servizio non viene toccato.
            let before = systemctl(&e, &format!("is-active {name}"));
            for a in [Action::Stop, Action::Restart, Action::Disable] {
                match b.action(Scope::System, name, a) {
                    Err(ActionError::Failed(m)) => {
                        assert!(
                            m.contains("needs root") && m.contains("--sudo"),
                            "{a:?}: unexpected text {m:?}"
                        );
                    }
                    other => {
                        panic!("{a:?} without root: expected 'needs root' text, got {other:?}")
                    }
                }
            }
            assert_eq!(systemctl(&e, &format!("is-active {name}")), before);
            return;
        }

        assert!(
            wait_active(&e, name, "active"),
            "test service must start active"
        );
        b.action(Scope::System, name, Action::Stop).expect("stop");
        assert!(wait_active(&e, name, "inactive"), "after stop");
        b.action(Scope::System, name, Action::Start).expect("start");
        assert!(wait_active(&e, name, "active"), "after start");
        let pid1 = systemctl(&e, &format!("show -p MainPID --value {name}"));
        b.action(Scope::System, name, Action::Restart)
            .expect("restart");
        assert!(wait_active(&e, name, "active"), "after restart");
        let pid2 = systemctl(&e, &format!("show -p MainPID --value {name}"));
        assert_ne!(pid1, pid2, "restart must change MainPID");
        b.action(Scope::System, name, Action::Disable)
            .expect("disable");
        assert_eq!(systemctl(&e, &format!("is-enabled {name}")), "disabled");
        b.action(Scope::System, name, Action::Enable)
            .expect("enable");
        assert_eq!(systemctl(&e, &format!("is-enabled {name}")), "enabled");
        // Unità inesistente: errore breve, nessun panic.
        match b.action(Scope::System, "ngtop-no-such-unit.service", Action::Start) {
            Err(ActionError::Failed(m)) => assert!(!m.is_empty() && !m.contains('\n')),
            other => panic!("missing unit: expected Failed, got {other:?}"),
        }
    }
}
