// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Prove di attacco permanenti alle azioni remote (segnali, nice, servizi, `--sudo`).
//!
//! Nessun container e nessuna rete: `NGTOP_SSH` punta a un "ssh" finto (tests/fixtures/remote/hostile/server.sh)
//! che IGNORA agent.sh e risponde come un server malevolo, oppure fa da proxy all'agente vero con comandi
//! finti (systemctl, renice…) davanti al PATH. Le azioni toccano solo processi `sleep` avviati dal test.
//!
//! I test marcati `#[ignore = "finding: …"]` documentano un difetto trovato nel codice di prodotto: si
//! rilanciano con `cargo test --test hostile -- --ignored`.

use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::{Duration, Instant};

use ngtop::collect::services::{Action, ActionError, Scope, backend_for};
use ngtop::collect::{Roots, System};
use ngtop::remote::{ConnectOptions, RemoteRunner, RemoteSource, Target, actions_for, connect};
use ngtop::source::actions::LocalActions;
use ngtop::source::{ProcActions, RemoteState, Source};

const SERVER: &str = include_str!("fixtures/remote/hostile/server.sh");
const PW: &str = "Sup3r-Secret-Zq9";

// ---------------------------------------------------------------------------------------------
// Infrastruttura
// ---------------------------------------------------------------------------------------------

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn connect_cmd(ssh: &str, sudo: bool) -> std::io::Result<(Arc<RemoteSource>, Arc<RemoteRunner>)> {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // SAFETY: l'accesso all'ambiente passa sempre da ENV_LOCK in questo file.
    unsafe { std::env::set_var("NGTOP_SSH", ssh) };
    connect(
        &Target::parse("h").expect("target"),
        &ConnectOptions {
            compress: false,
            sudo,
        },
    )
}

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ngtop-hostile-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("bin")).unwrap();
    d
}

fn write_exec(p: &Path, text: &str) {
    std::fs::write(p, text).unwrap();
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// Comandi finti condivisi, scritti UNA volta e provati con un'esecuzione (ritentando su ETXTBSY: un figlio
/// forkato da un altro test può tenere ancora aperto il descrittore di scrittura). Le cartelle dei test
/// contengono solo link simbolici a file già eseguibili senza scrittori.
fn shared_tools() -> &'static Path {
    static TOOLS: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    TOOLS.get_or_init(|| {
        let d = std::env::temp_dir().join(format!("ngtop-hostile-tools-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        for tool in ["systemctl", "journalctl", "loginctl", "renice"] {
            let p = d.join(tool);
            write_exec(
                &p,
                &format!(
                    "#!/bin/sh\n[ \"$1\" = --ngtop-probe ] && exit 0\nD=$(dirname \"$0\")\n{{ printf '%s\\0' {tool}; for a in \"$@\"; do printf '%s\\0' \"$a\"; done; printf '\\0'; }} >> \"$D/../calls.log\"\nexit 0\n"
                ),
            );
            let t0 = Instant::now();
            loop {
                match Command::new(&p).arg("--ngtop-probe").status() {
                    Ok(_) => break,
                    Err(e) if e.raw_os_error() == Some(26) && t0.elapsed() < Duration::from_secs(20) => {
                        sleep(Duration::from_millis(20));
                    }
                    Err(e) => panic!("cannot run fake {tool}: {e}"),
                }
            }
        }
        d
    })
}

/// Cartella con l'"ssh" finto e i comandi finti (`systemctl`, `journalctl`, `loginctl`, `renice`) che
/// registrano gli argomenti in `calls.log` (ogni argomento seguito da NUL, ogni chiamata da un NUL in più).
fn setup(name: &str) -> PathBuf {
    let d = tmpdir(name);
    std::fs::write(d.join("ssh"), SERVER).unwrap();
    for tool in ["systemctl", "journalctl", "loginctl", "renice"] {
        std::os::unix::fs::symlink(shared_tools().join(tool), d.join("bin").join(tool)).unwrap();
    }
    d
}

/// Chiamate registrate dai comandi finti: ogni chiamata = vettore di argomenti (il primo è il comando).
fn calls(dir: &Path) -> Vec<Vec<String>> {
    let raw = std::fs::read(dir.join("calls.log")).unwrap_or_default();
    let mut out = Vec::new();
    let mut cur: Vec<String> = Vec::new();
    for part in raw.split(|&b| b == 0) {
        if part.is_empty() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push(String::from_utf8_lossy(part).into_owned());
        }
    }
    out
}

struct Host {
    src: Arc<RemoteSource>,
    run: Arc<RemoteRunner>,
    act: Arc<dyn ProcActions>,
    dir: PathBuf,
    log: PathBuf,
}

impl Host {
    fn roots(&self) -> Roots {
        Roots::remote(self.src.clone(), self.run.clone())
    }
    fn status_text(&self) -> String {
        match self.src.remote_status().map(|s| s.state) {
            Some(RemoteState::Reconnecting(m)) | Some(RemoteState::Failed(m)) => m,
            _ => String::new(),
        }
    }
    /// Righe `REQ …` registrate dal server finto.
    fn reqs(&self) -> Vec<String> {
        std::fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.strip_prefix("REQ ").map(str::to_string))
            .collect()
    }
}

fn try_host(name: &str, scenario: &str, param: &str) -> std::io::Result<Host> {
    let dir = setup(name);
    let log = dir.join("log");
    let cmd = format!(
        "sh {}/ssh {scenario} {} {param}",
        dir.display(),
        log.display()
    );
    let (src, run) = connect_cmd(&cmd, false)?;
    let act = actions_for(&src);
    Ok(Host {
        src,
        run,
        act,
        dir,
        log,
    })
}

fn host(name: &str, scenario: &str, param: &str) -> Host {
    try_host(name, scenario, param).unwrap_or_else(|e| panic!("connect to {scenario}: {e}"))
}

/// Esegue `f` in un thread: se non finisce entro `secs` il test fallisce ("HANG") invece di bloccarsi.
fn within<T: Send + 'static>(secs: u64, what: &str, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        let _ = tx.send(r);
    });
    match rx.recv_timeout(Duration::from_secs(secs)) {
        Ok(Ok(v)) => v,
        Ok(Err(p)) => std::panic::resume_unwind(p),
        Err(_) => panic!("HANG: {what} did not return within {secs}s"),
    }
}

/// Testo mostrabile nell'interfaccia: nessun carattere di controllo (ESC, BEL, newline, NUL…).
fn clean(s: &str) -> bool {
    !s.chars().any(|c| c.is_control())
}

fn assert_clean(what: &str, s: &str) {
    assert!(clean(s), "{what} contains control characters: {s:?}");
}

struct Sleeper(Child);

impl Sleeper {
    fn new() -> Sleeper {
        Sleeper(
            Command::new("sleep")
                .arg("600")
                .stdin(Stdio::null())
                .spawn()
                .expect("sleep"),
        )
    }
    fn pid(&self) -> u32 {
        self.0.id()
    }
    fn alive(&mut self) -> bool {
        matches!(self.0.try_wait(), Ok(None))
    }
    fn start(&self) -> u64 {
        LocalActions::new(PathBuf::from("/proc")).live_starts(&[self.pid()])[0].expect("start")
    }
}

impl Drop for Sleeper {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn nice_of(pid: u32) -> i32 {
    let s = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    let rest = &s[s.rfind(") ").unwrap() + 2..];
    rest.split(' ').nth(16).unwrap().parse().unwrap()
}

/// Generatore deterministico (xorshift64*).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

// ---------------------------------------------------------------------------------------------
// 1. Server ostile
// ---------------------------------------------------------------------------------------------

#[test]
fn hostile_garbage_replies_never_panic_hang_or_act_locally() {
    let scenarios = [
        "badnonce",
        "dup",
        "missing",
        "unasked",
        "fewer",
        "rcbad",
        "negative",
        "ready_neg",
        "ready_huge",
    ];
    // risposte che il client DEVE rifiutare (il numero di esiti non coincide, o il protocollo è rotto)
    let must_fail = ["badnonce", "missing", "unasked", "fewer", "rcbad"];
    for sc in scenarios {
        let mut victim = Sleeper::new();
        let (pid, start) = (victim.pid(), victim.start());
        let h = host(&format!("garbage-{sc}"), sc, "-");
        let act = h.act.clone();
        let (starts, sig, nice) = within(40, sc, move || {
            (
                act.live_starts(&[pid, 1, 0, u32::MAX, 4_000_000_000]),
                act.signal(&[(pid, start)], 15),
                act.set_nice(pid, start, 3),
            )
        });
        assert_eq!(starts.len(), 5, "{sc}");
        for (what, r) in [("signal", &sig[0]), ("nice", &nice)] {
            if let Err(e) = r {
                assert_clean(&format!("{sc}: {what} error"), e);
            } else if must_fail.contains(&sc) {
                panic!("{sc}: {what} accepted a forged reply");
            }
        }
        if sc == "negative" {
            assert!(starts.iter().all(|s| s.is_none()), "{sc}: {starts:?}");
        }
        assert_clean(&format!("{sc}: state"), &h.status_text());
        assert!(victim.alive(), "{sc}: the local sleep was hit");
    }
}

#[test]
fn hostile_fake_ready_is_harmless() {
    for sc in ["ready_root", "ready_neg", "ready_huge"] {
        let mut victim = Sleeper::new();
        let (pid, start) = (victim.pid(), victim.start());
        // la catena di antenati "bugiarda" contiene il pid del sleep locale
        let h = host(&format!("ready-{sc}"), sc, &pid.to_string());
        let act = h.act.clone();
        let _ = within(40, sc, move || {
            let _ = act.protected(pid);
            act.signal(&[(pid, start)], 15)
        });
        // le azioni delle Roots remote non sono mai locali: rifiutano e non toccano il sleep
        let r = h.roots();
        assert!(r.act.signal(&[(pid, start)], 15)[0].is_err(), "{sc}");
        assert!(r.act.set_nice(pid, start, 5).is_err(), "{sc}");
        assert!(victim.alive(), "{sc}: the local sleep was hit");
        assert_eq!(nice_of(pid), 0, "{sc}: nice of local sleep changed");
    }
}

#[test]
fn hostile_huge_page_size_and_ticks_do_not_panic_the_collectors() {
    for sc in ["proxy_ready_huge", "proxy_ready_zero", "proxy_ready_neg"] {
        let h = host(&format!("huge-{sc}"), sc, "-");
        let roots = h.roots();
        within(60, sc, move || {
            let mut s = System::new(roots);
            s.refresh();
            sleep(Duration::from_millis(300));
            s.refresh();
        });
    }
}

#[test]
fn hostile_missing_end_marker_times_out() {
    let mut victim = Sleeper::new();
    let (pid, start) = (victim.pid(), victim.start());
    let h = host("nozed", "nozed", "-");
    let act = h.act.clone();
    let t0 = Instant::now();
    let r = within(20, "nozed", move || act.signal(&[(pid, start)], 15));
    assert!(r[0].is_err(), "a reply without end marker must be an error");
    assert_clean("error", r[0].as_ref().unwrap_err());
    assert!(t0.elapsed() < Duration::from_secs(16), "{:?}", t0.elapsed());
    assert!(victim.alive());
}

#[test]
fn hostile_huge_bodies_are_bounded() {
    let victim = Sleeper::new();
    let pid = victim.pid();
    // 40 MB: accettati o rifiutati, ma senza panic e senza dare un avvio
    let h = host("huge40", "huge40", "-");
    let act = h.act.clone();
    let s = within(90, "huge40", move || act.live_starts(&[pid]));
    assert_eq!(s, vec![None]);
    // 140 MB: oltre il tetto del client -> errore, non memoria illimitata
    let h = host("huge140", "huge140", "-");
    let act = h.act.clone();
    let s = within(120, "huge140", move || act.live_starts(&[pid]));
    assert_eq!(s, vec![None]);
    sleep(Duration::from_millis(200));
    let text = h.status_text();
    assert_clean("state", &text);
}

#[test]
fn hostile_endless_output_without_newline_is_cut() {
    let mut victim = Sleeper::new();
    let (pid, start) = (victim.pid(), victim.start());
    let h = host("endless", "endless", "-");
    let act = h.act.clone();
    let t0 = Instant::now();
    let r = within(90, "endless", move || act.signal(&[(pid, start)], 15));
    assert!(r[0].is_err());
    assert_clean("error", r[0].as_ref().unwrap_err());
    assert!(t0.elapsed() < Duration::from_secs(80));
    assert!(victim.alive());
}

#[test]
fn hostile_hostname_with_escapes_is_sanitised() {
    let h = host("esc-hostname", "escapes", "-");
    let name = h
        .src
        .remote_status()
        .and_then(|s| s.hostname)
        .unwrap_or_default();
    assert_clean("hostname shown in the top bar", &name);
}

#[test]
fn hostile_action_error_texts_have_no_control_characters() {
    let mut victim = Sleeper::new();
    let (pid, start) = (victim.pid(), victim.start());
    let h = host("esc-actions", "escapes", "-");
    let b = backend_for(&h.roots().src, &h.roots().run);
    for a in [Action::Stop, Action::Start, Action::Restart] {
        match b.action(Scope::System, "foo.service", a) {
            Err(ActionError::Failed(m)) => assert_clean("service error", &m),
            other => panic!("expected a clean failure, got {other:?}"),
        }
    }
    let sig = h.act.signal(&[(pid, start)], 15);
    if let Err(e) = &sig[0] {
        assert_clean("signal error", e);
    }
    if let Err(e) = h.act.set_nice(pid, start, 4) {
        assert_clean("nice error", &e);
    }
    assert!(victim.alive());
}

#[test]
fn hostile_connect_error_text_has_no_control_characters() {
    let e = match try_host("esc-connect", "escapes_connect", "-") {
        Err(e) => e.to_string(),
        Ok(_) => panic!("the hostile server must not connect"),
    };
    assert_clean("connect error", &e);
}

#[test]
fn hostile_connection_state_message_has_no_control_characters() {
    let mut victim = Sleeper::new();
    let (pid, start) = (victim.pid(), victim.start());
    let h = host("esc-state", "escapes_stderr", "-");
    let act = h.act.clone();
    let r = within(30, "escapes_stderr", move || {
        act.signal(&[(pid, start)], 15)
    });
    assert_clean("signal error", r[0].as_ref().unwrap_err());
    assert_clean("connection state message", &h.status_text());
    assert!(victim.alive());
}

// ---------------------------------------------------------------------------------------------
// 2. Fuzz degli ingressi (agente vero + comandi finti)
// ---------------------------------------------------------------------------------------------

fn valid_unit_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '.' | '\\' | '@' | '-')
}

#[test]
fn fuzz_unit_names_never_run_anything_but_the_expected_argv() {
    let h = host("fuzz-units", "proxy", "-");
    let marker = h.dir.join("INJECTED");
    let m = marker.display().to_string();
    let mut names: Vec<String> = vec![
        "ngtop-test.service".into(),
        "a@b.service".into(),
        "-h".into(),
        "--now".into(),
        "-".into(),
        "--".into(),
        "-- --version".into(),
        "x\\x2dy.service".into(),
        "café.service".into(),
        "日本.service".into(),
        "a b.service".into(),
        "a\nb.service".into(),
        "a\0b".into(),
        String::new(),
        "a".repeat(255),
        "a".repeat(256),
        "a".repeat(100_000),
        format!("a;touch {m}"),
        format!("a;touch {m};"),
        format!("$(touch {m})"),
        format!("`touch {m}`"),
        format!("a|touch {m}"),
        format!("a&&touch {m}"),
        format!("a\ntouch {m}"),
        format!("a'; touch {m}; '"),
        format!("a\"; touch {m}; \""),
        format!("a${{IFS}}touch${{IFS}}{m}"),
        format!("--unit=x --exec={m}"),
        "../../etc/passwd".into(),
        "*".into(),
        "[a-z]*".into(),
        "~".into(),
        "$HOME".into(),
        "%n".into(),
    ];
    let charset: Vec<char> = "ab.-_@:\\;|&$`'\"\n *?~%\u{e9}\u{1b}x0".chars().collect();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for _ in 0..150 {
        let len = 1 + (rng.next() % 24) as usize;
        let s: String = (0..len)
            .map(|_| charset[(rng.next() % charset.len() as u64) as usize])
            .collect();
        names.push(s);
    }
    let b = backend_for(&h.roots().src, &h.roots().run);
    let b2 = b.clone();
    let names2 = names.clone();
    within(280, "unit name fuzz", move || {
        for n in &names2 {
            for a in [
                Action::Start,
                Action::Stop,
                Action::Restart,
                Action::Enable,
                Action::Disable,
            ] {
                let _ = b2.action(Scope::System, n, a);
            }
            let _ = b2.unit_detail(Scope::System, n);
            let _ = b2.journal(Scope::System, n, 5);
        }
    });
    assert!(!marker.exists(), "INJECTION: a unit name ran a command");
    let all = calls(&h.dir);
    let mut action_calls = 0;
    for c in &all {
        for a in c {
            assert!(!a.contains("touch"), "suspicious argument {a:?} in {c:?}");
        }
        if c[0] == "systemctl"
            && c.iter().any(|a| {
                matches!(
                    a.as_str(),
                    "start" | "stop" | "restart" | "enable" | "disable"
                )
            })
            && c.iter().any(|a| a == "--no-ask-password")
        {
            action_calls += 1;
            // forma esatta: systemctl --no-ask-password --no-pager [--no-block] VERBO -- NOME
            let name = c.last().unwrap();
            assert!(
                !name.is_empty() && name.len() < 256 && name.chars().all(valid_unit_char),
                "invalid unit name reached systemctl: {name:?}"
            );
            assert!(names.contains(name), "unknown name {name:?}");
            let n = c.len();
            assert_eq!(c[n - 2], "--", "argv {c:?}");
            assert!(c.len() == 6 || c.len() == 7, "unexpected argv shape {c:?}");
            assert_eq!(&c[1..3], ["--no-ask-password", "--no-pager"], "argv {c:?}");
        }
    }
    // ogni nome valido arriva a systemctl, ogni verbo, una sola volta
    let valid = names
        .iter()
        .filter(|n| !n.is_empty() && n.len() < 256 && n.chars().all(valid_unit_char))
        .count();
    assert_eq!(action_calls, valid * 5, "action calls vs valid names");
}

#[test]
fn fuzz_process_inputs_hit_only_matching_start_times() {
    let h = host("fuzz-procs", "proxy", "-");
    let mut a = Sleeper::new();
    let mut b = Sleeper::new();
    let (pa, sa) = (a.pid(), a.start());
    let (pb, sb) = (b.pid(), b.start());
    let dead = {
        let mut c = Sleeper::new();
        let p = c.pid();
        let _ = c.0.kill();
        let _ = c.0.wait();
        p
    };
    let pids = [
        pa,
        pb,
        dead,
        0,
        u32::MAX,
        i32::MAX as u32,
        i32::MAX as u32 + 1,
        4_194_304,
    ];
    let mut starts = vec![
        0u64,
        1,
        u64::MAX,
        u64::MAX - 1,
        1 << 63,
        sa.wrapping_add(1),
        sa.wrapping_sub(1),
    ];
    let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);
    for _ in 0..20 {
        starts.push(rng.next());
    }
    let sigs = [i32::MIN, -1, 0, 1, 2, 9, 15, 19, 63, 64, 65, 1000, i32::MAX];
    let nices = [i32::MIN, -21, -20, -1, 0, 5, 19, 20, i32::MAX];
    let act = h.act.clone();
    let real = [(pa, sa), (pb, sb)];
    let res = within(240, "process fuzz", move || {
        let mut ok_hits = Vec::new();
        for &p in &pids {
            for &s in &starts {
                // mai l'avvio vero di questo pid (con wrapping ±1 può coincidere con quello dell'altro sleep)
                if real.contains(&(p, s)) {
                    continue;
                }
                for &sig in &sigs {
                    let r = act.signal(&[(p, s)], sig);
                    assert_eq!(r.len(), 1);
                    if r[0].is_ok() {
                        ok_hits.push((p, s, sig));
                    }
                }
                for &n in &nices {
                    if let Err(e) = act.set_nice(p, s, n) {
                        assert!(clean(&e), "{e:?}");
                    } else {
                        ok_hits.push((p, s, 1000 + n));
                    }
                }
            }
        }
        // batch con valori estremi mescolati
        let batch: Vec<(u32, u64)> = pids
            .iter()
            .zip(starts.iter())
            .map(|(&p, &s)| (p, s))
            .collect();
        let r = act.signal(&batch, 15);
        assert!(r.len() == batch.len() || r.iter().all(|x| x.is_err()));
        ok_hits
    });
    assert!(
        res.is_empty(),
        "something was signalled with a wrong start time: {res:?}"
    );
    assert!(
        a.alive() && b.alive(),
        "a sleep with a wrong start time was hit"
    );
    assert!(
        !calls(&h.dir).iter().any(|c| c[0] == "renice"),
        "renice ran for a stale start time"
    );
    // controllo positivo: con l'avvio giusto il segnale parte
    let r = h.act.signal(&[(pa, sa)], 15);
    assert_eq!(r, vec![Ok(())]);
    let t0 = Instant::now();
    while a.alive() {
        assert!(
            t0.elapsed() < Duration::from_secs(5),
            "SIGTERM not delivered"
        );
        sleep(Duration::from_millis(50));
    }
    assert!(b.alive());
}

// ---------------------------------------------------------------------------------------------
// 3. Pid riusato sotto carico
// ---------------------------------------------------------------------------------------------

#[test]
fn stale_start_time_under_fork_load_is_never_hit() {
    let h = host("load", "proxy", "-");
    // prima un processo morto (la sua ora di avvio resta "stantia"), poi la vittima, in un altro tick
    let dead_start = {
        let mut c = Sleeper::new();
        let s = c.start();
        let _ = c.0.kill();
        let _ = c.0.wait();
        s
    };
    sleep(Duration::from_millis(100));
    let mut victim = Sleeper::new();
    let (pid, start) = (victim.pid(), victim.start());
    assert_ne!(dead_start, start);
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let spawned = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let load: Vec<_> = (0..2)
        .map(|_| {
            let (stop, spawned) = (stop.clone(), spawned.clone());
            std::thread::spawn(move || {
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    let _ = Command::new("true").status();
                    spawned.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
            })
        })
        .collect();
    let act = h.act.clone();
    let hits = within(200, "stale pid load", move || {
        let mut hits = 0;
        let stale = [
            start - 1,
            start + 1,
            start + 100,
            dead_start,
            start.wrapping_add(1 << 32),
        ];
        for i in 0..1500 {
            let s = stale[i % stale.len()];
            if act.signal(&[(pid, s)], 15)[0].is_ok() {
                hits += 1;
            }
            if i % 50 == 0 && act.set_nice(pid, s, 7).is_ok() {
                hits += 1;
            }
        }
        hits
    });
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    for t in load {
        let _ = t.join();
    }
    assert_eq!(hits, 0, "a stale (pid, start) was accepted");
    assert!(victim.alive(), "the victim was killed by a stale signal");
    assert!(
        spawned.load(std::sync::atomic::Ordering::Relaxed) > 500,
        "load too light"
    );
    // controllo positivo
    assert_eq!(h.act.signal(&[(pid, start)], 15), vec![Ok(())]);
    let t0 = Instant::now();
    while victim.alive() {
        assert!(t0.elapsed() < Duration::from_secs(5));
        sleep(Duration::from_millis(50));
    }
}

// ---------------------------------------------------------------------------------------------
// 5. Connessione caduta a metà azione
// ---------------------------------------------------------------------------------------------

#[test]
fn dropped_connection_during_signal_is_a_clear_error_and_not_repeated() {
    let mut victim = Sleeper::new();
    let (pid, start) = (victim.pid(), victim.start());
    let h = host("drop-sig", "drop", "-");
    let act = h.act.clone();
    let r = within(30, "drop signal", move || {
        act.signal(&[(pid, start), (pid, start)], 15)
    });
    assert_eq!(r.len(), 2);
    for x in &r {
        let e = x
            .as_ref()
            .expect_err("a dropped batch must not report success");
        assert!(!e.is_empty());
        assert_clean("error", e);
    }
    // il client si riconnette in sottofondo: l'azione NON deve ripartire da sola
    sleep(Duration::from_secs(4));
    let sent: Vec<_> = h
        .reqs()
        .into_iter()
        .filter(|l| l.starts_with("a "))
        .collect();
    assert_eq!(sent.len(), 1, "signal request repeated: {sent:?}");
    // e nemmeno senza il controllo dell'avvio: il tentativo successivo ricontrolla da capo
    assert!(victim.alive());
    let r = h.act.set_nice(pid, start, 5);
    assert!(r.is_err());
    let sent: Vec<_> = h
        .reqs()
        .into_iter()
        .filter(|l| l.starts_with("r "))
        .collect();
    assert!(sent.len() <= 1, "{sent:?}");
}

#[test]
fn dropped_connection_during_service_action_is_a_clear_error_and_not_repeated() {
    let h = host("drop-svc", "drop", "-");
    let b = backend_for(&h.roots().src, &h.roots().run);
    let b2 = b.clone();
    let r = within(40, "drop service", move || {
        b2.action(Scope::System, "ngtop-test.service", Action::Restart)
    });
    match r {
        Err(ActionError::Failed(m)) => {
            assert!(!m.is_empty());
            assert_clean("error", &m);
        }
        other => panic!("expected a failure, got {other:?}"),
    }
    sleep(Duration::from_secs(4));
    let n = h
        .reqs()
        .into_iter()
        .filter(|l| l.starts_with("x ") && l.contains("no-block"))
        .count();
    assert_eq!(n, 1, "service action request repeated after the drop");
}

// ---------------------------------------------------------------------------------------------
// 4. sudo: password sbagliata, lenta, ignorata dal sudo. Serve un terminale: il test esterno rilancia
//    se stesso sotto `script` (pty) e scrive la password quando compare il prompt.
// ---------------------------------------------------------------------------------------------

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Esegue il test interno `name` sotto un pty; ritorna (successo, trascrizione).
fn run_inner(name: &str, dir: &Path, limit: u64) -> Option<(bool, String)> {
    if Command::new("script").arg("--version").output().is_err() {
        eprintln!("script(1) not found: sudo test skipped");
        return None;
    }
    let exe = std::env::current_exe().unwrap();
    let cmd = format!(
        "{} --ignored --exact {name} --nocapture --test-threads=1",
        shell_quote(&exe.to_string_lossy())
    );
    let mut ch = Command::new("script")
        .args(["-qec", &cmd, "/dev/null"])
        .env("NGTOP_HOSTILE_DIR", dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("script");
    let mut stdin = ch.stdin.take().unwrap();
    let mut out = ch.stdout.take().unwrap();
    let transcript = Arc::new(Mutex::new(Vec::<u8>::new()));
    let t2 = transcript.clone();
    let reader = std::thread::spawn(move || {
        let mut b = [0u8; 4096];
        while let Ok(n) = out.read(&mut b) {
            if n == 0 {
                break;
            }
            t2.lock().unwrap().extend_from_slice(&b[..n]);
        }
    });
    let t0 = Instant::now();
    let mut fed = 0;
    let status = loop {
        let seen = String::from_utf8_lossy(&transcript.lock().unwrap())
            .matches("password for")
            .count();
        if seen > fed {
            sleep(Duration::from_millis(300));
            let _ = writeln!(stdin, "{PW}");
            let _ = stdin.flush();
            fed = seen;
        }
        if let Ok(Some(s)) = ch.try_wait() {
            break Some(s);
        }
        if t0.elapsed() > Duration::from_secs(limit) {
            let _ = ch.kill();
            let _ = ch.wait();
            break None;
        }
        sleep(Duration::from_millis(100));
    };
    drop(stdin);
    let _ = reader.join();
    let text = String::from_utf8_lossy(&transcript.lock().unwrap()).into_owned();
    Some((status.is_some_and(|s| s.success()), text))
}

fn outer_sudo(name: &str, limit: u64) {
    let dir = tmpdir(name);
    let Some((ok, text)) = run_inner(&format!("inner_{name}"), &dir, limit) else {
        return;
    };
    assert!(
        !text.contains(PW),
        "the password appears in the terminal transcript"
    );
    if !ok {
        let tail: String = text
            .chars()
            .rev()
            .take(1500)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        panic!("inner test failed:\n{tail}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

fn inner_dir() -> Option<PathBuf> {
    let d = PathBuf::from(std::env::var("NGTOP_HOSTILE_DIR").ok()?);
    std::fs::write(d.join("ssh"), SERVER).unwrap();
    Some(d)
}

fn sudo_cmd(d: &Path, sc: &str) -> String {
    format!("sh {}/ssh {sc} {} -", d.display(), d.join("log").display())
}

fn read(d: &Path, ext: &str) -> String {
    std::fs::read_to_string(d.join(format!("log.{ext}"))).unwrap_or_default()
}

/// La password non compare né in argv né nell'ambiente di ssh, né in ciò che la shell remota ha ricevuto.
fn no_password_anywhere(d: &Path) {
    for ext in ["argv", "env", "leak"] {
        let t = read(d, ext);
        assert!(!t.contains(PW), "the password appears in log.{ext}:\n{t}");
    }
}

#[test]
fn sudo_wrong_password_sends_no_script_line_and_leaks_nothing() {
    outer_sudo("sudo_wrong", 90);
}

#[test]
#[ignore = "inner test: run by sudo_wrong_password_* under a pty"]
fn inner_sudo_wrong() {
    let Some(d) = inner_dir() else { return };
    let e = connect_cmd(&sudo_cmd(&d, "sudo_wrong"), true)
        .err()
        .expect("wrong password must fail");
    assert_eq!(e.to_string(), "sudo: wrong password");
    sleep(Duration::from_secs(4)); // il sudo finto legge per 3 s: qualunque riga seguente verrebbe registrata
    assert_eq!(
        read(&d, "leak"),
        "",
        "lines reached the remote side after a wrong password"
    );
    no_password_anywhere(&d);
}

#[test]
fn sudo_correct_password_is_not_in_argv_env_or_errors() {
    outer_sudo("sudo_right", 90);
}

#[test]
#[ignore = "inner test: run by sudo_correct_password_* under a pty"]
fn inner_sudo_right() {
    let Some(d) = inner_dir() else { return };
    let (src, _) = connect_cmd(&sudo_cmd(&d, "sudo_right"), true).expect("connect");
    let act = actions_for(&src);
    let v = Sleeper::new();
    assert!(act.live_starts(&[v.pid()])[0].is_some());
    assert_eq!(read(&d, "leak"), "");
    no_password_anywhere(&d);
}

#[test]
fn sudo_that_ignores_the_password_never_feeds_it_to_the_remote_shell() {
    outer_sudo("sudo_ignore", 90);
}

#[test]
#[ignore = "inner test: run by sudo_that_ignores_* under a pty"]
fn inner_sudo_ignore() {
    let Some(d) = inner_dir() else { return };
    let (src, _) = connect_cmd(&sudo_cmd(&d, "sudo_ignore"), true).expect("first connect");
    let act = actions_for(&src);
    let v = Sleeper::new();
    assert!(act.live_starts(&[v.pid()])[0].is_some());
    // il sudo del server ora e' NOPASSWD: alla riconnessione non chiede nulla e non legge la password
    src.kill_session_for_test();
    let t0 = Instant::now();
    while !matches!(
        src.remote_status().map(|s| s.state),
        Some(RemoteState::Connected)
    ) || act.live_starts(&[v.pid()])[0].is_none()
    {
        assert!(t0.elapsed() < Duration::from_secs(20), "never reconnected");
        sleep(Duration::from_millis(250));
    }
    assert!(
        !read(&d, "leak").contains(PW),
        "the password reached the remote shell as a command"
    );
    assert_eq!(
        read(&d, "leak"),
        "",
        "unexpected lines reached the remote shell"
    );
    no_password_anywhere(&d);
}

#[test]
fn sudo_slow_on_reconnect_gives_up_without_sending_the_script() {
    outer_sudo("sudo_slow", 120);
}

#[test]
#[ignore = "inner test: run by sudo_slow_* under a pty"]
fn inner_sudo_slow() {
    let Some(d) = inner_dir() else { return };
    let (src, _) = connect_cmd(&sudo_cmd(&d, "sudo_slow"), true).expect("first connect");
    let act = actions_for(&src);
    let v = Sleeper::new();
    assert!(act.live_starts(&[v.pid()])[0].is_some());
    src.kill_session_for_test();
    // la riconnessione trova un sudo che non risponde: entro ~20 s deve arrendersi con un errore chiaro
    let t0 = Instant::now();
    let msg = loop {
        // una richiesta fa accorgere il client che la sessione è morta e avvia la riconnessione
        let _ = act.live_starts(&[v.pid()]);
        if let Some(RemoteState::Failed(m)) = src.remote_status().map(|s| s.state) {
            break m;
        }
        assert!(
            t0.elapsed() < Duration::from_secs(45),
            "reconnect never gave up; state {:?}",
            src.remote_status().map(|s| s.state)
        );
        sleep(Duration::from_millis(250));
    };
    assert!(msg.starts_with("sudo: "), "{msg}");
    assert!(!msg.contains(PW));
    assert!(clean(&msg));
    sleep(Duration::from_secs(1));
    assert_eq!(
        read(&d, "leak"),
        "",
        "script lines reached a sudo that had not answered yet"
    );
    no_password_anywhere(&d);
}
