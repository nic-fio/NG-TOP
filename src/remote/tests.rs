// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Prove senza rete: lo stesso protocollo attraverso un `sh` locale, confrontato con `LocalSource`.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::source::template;
use super::*;
use crate::source::{CommandRunner, LocalRunner, LocalSource, RemoteState, Source};

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ngtop-remote-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn letture_identiche_a_locale() {
    let (r, _) = connect_local_sh().unwrap();
    let l = LocalSource;
    let d = tmpdir("file");
    let strane: &[(&str, &[u8])] = &[
        ("con spazi.txt", b"a b\nc d\n"),
        ("senza_newline", b"abc"),
        ("vuoto", b""),
        ("solo_newline", b"\n\n\n"),
        ("binario", &[0, 1, 2, 0xff, 0xfe, b'\n', 0, b'\n', 0x80]),
        ("it's;$(x)`y`", b"apici\n"),
        ("finto", b"\n~ngtop-0123 ! 0\n~ngtop-0123 Z 1\nfine"),
        ("latin1", b"caff\xe8\n"),
    ];
    for (n, c) in strane {
        std::fs::write(d.join(n), c).unwrap();
    }
    let big: Vec<u8> = (0..300_000u32).map(|i| (i * 7 % 251) as u8).collect();
    std::fs::write(d.join("grande"), &big).unwrap();
    for (n, c) in strane {
        assert_eq!(&r.read(&d.join(n)).unwrap(), c, "{n}");
    }
    assert_eq!(r.read(&d.join("grande")).unwrap(), big);
    // /proc reale: cmdline (con NUL), file piccoli e statici
    let me = std::process::id();
    for p in [
        format!("/proc/{me}/cmdline"),
        format!("/proc/{me}/comm"),
        "/proc/version".into(),
        "/proc/sys/kernel/hostname".into(),
        "/proc/cmdline".into(),
        "/etc/passwd".into(),
    ] {
        assert_eq!(
            r.read(Path::new(&p)).ok(),
            l.read(Path::new(&p)).ok(),
            "{p}"
        );
    }
    let st = r.read(Path::new("/proc/stat")).unwrap();
    assert!(st.starts_with(b"cpu "));
    let status = r
        .read_text(Path::new(&format!("/proc/{me}/status")))
        .unwrap();
    assert!(status.contains(&format!("Pid:\t{me}")));
}

#[test]
fn errori_e_percorsi_invalidi() {
    let (r, _) = connect_local_sh().unwrap();
    let e = r.read(Path::new("/non/esiste")).unwrap_err();
    assert_eq!(e.kind(), ErrorKind::NotFound);
    let e = r.read(Path::new("/tmp")).unwrap_err();
    assert_eq!(e.raw_os_error(), Some(21)); // EISDIR
    assert_eq!(
        r.read(Path::new("relativo")).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(
        r.read(Path::new("/tmp/a\nb")).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(
        r.read_dir(Path::new("/non/esiste")).unwrap_err().kind(),
        ErrorKind::NotFound
    );
    assert_eq!(
        r.read_link(Path::new("/non/esiste")).unwrap_err().kind(),
        ErrorKind::NotFound
    );
    assert_eq!(
        r.owner_uid(Path::new("/non/esiste")).unwrap_err().kind(),
        ErrorKind::NotFound
    );
    assert!(r.statvfs(Path::new("/non/esiste")).is_err());
    assert!(!r.exists(Path::new("/non/esiste")));
    assert!(r.exists(Path::new("/proc/stat")));
    // permesso negato (solo da utente normale)
    if rustix::process::getuid().as_raw() != 0 {
        let d = tmpdir("perm");
        let f = d.join("segreto");
        std::fs::write(&f, b"x").unwrap();
        std::fs::set_permissions(&f, std::os::unix::fs::PermissionsExt::from_mode(0o000)).unwrap();
        assert_eq!(r.read(&f).unwrap_err().kind(), ErrorKind::PermissionDenied);
    }
}

#[test]
fn cartelle_link_uid_statvfs() {
    let (r, _) = connect_local_sh().unwrap();
    let l = LocalSource;
    let d = tmpdir("dir");
    for n in ["a", "b c", ".nascosto", "..doppio", "x\ny", "it's"] {
        std::fs::write(d.join(n), b"1").unwrap();
    }
    std::fs::create_dir(d.join("sotto")).unwrap();
    std::os::unix::fs::symlink("/etc/passwd", d.join("link")).unwrap();
    std::os::unix::fs::symlink("rotto con spazi", d.join("rotto")).unwrap();
    let mut a = r.read_dir(&d).unwrap();
    let mut b = l.read_dir(&d).unwrap();
    a.sort();
    b.sort();
    assert_eq!(a, b);
    assert_eq!(
        r.read_link(&d.join("link")).unwrap(),
        PathBuf::from("/etc/passwd")
    );
    assert_eq!(
        r.read_link(&d.join("rotto")).unwrap(),
        PathBuf::from("rotto con spazi")
    );
    assert_eq!(
        r.read_link(&d.join("a")).unwrap_err().raw_os_error(),
        Some(22)
    );
    assert!(r.exists(&d.join("rotto")));
    assert_eq!(r.owner_uid(&d).unwrap(), l.owner_uid(&d).unwrap());
    assert_eq!(
        r.owner_uid(&d.join("link")).unwrap(),
        l.owner_uid(&d.join("link")).unwrap()
    );
    assert_eq!(r.owner_uid(Path::new("/proc/1")).unwrap(), 0);
    let (x, y) = (
        r.statvfs(Path::new("/")).unwrap(),
        l.statvfs(Path::new("/")).unwrap(),
    );
    assert_eq!((x.total, x.files), (y.total, y.files));
    assert!(x.used.abs_diff(y.used) < (1 << 24), "{} {}", x.used, y.used);
    // /proc: la lista dei pid contiene il nostro
    let procs = r.read_dir(Path::new("/proc")).unwrap();
    assert!(
        procs
            .iter()
            .any(|n| n.to_str() == Some(&std::process::id().to_string()))
    );
}

#[test]
fn comandi_con_argomenti_difficili() {
    let (_, run) = connect_local_sh().unwrap();
    let loc = LocalRunner;
    let t = Duration::from_secs(5);
    let argv: &[&[&str]] = &[
        &[
            "printf", "%s|", "a b", "it's", "x;y", "$HOME", "`id`", "$(id)", "l1\nl2", "", "*",
            "\\n",
        ],
        &["echo", "ciao"],
        &["sh", "-c", "echo out; echo err >&2; exit 3"],
        &["sh", "-c", "printf senza-newline"],
        &["sh", "-c", "printf 'a\\0b'"],
    ];
    for a in argv {
        let x = run.run(a, t).unwrap();
        let y = loc.run(a, t).unwrap();
        assert_eq!(x.stdout, y.stdout, "{a:?}");
        assert_eq!(
            x.status.map(|s| s.min(127)),
            y.status.map(|s| s.min(127)),
            "{a:?}"
        );
        assert_eq!(x.stderr, y.stderr, "{a:?}");
    }
    assert_eq!(run.run(&["nonesiste-ngtop"], t).unwrap().status, Some(127));
    assert!(run.run(&[], t).is_err());
    assert!(run.run(&["echo", "a\0b"], t).is_err());
}

#[test]
fn comando_lento_va_in_timeout_senza_bloccare() {
    let (src, run) = connect_local_sh().unwrap();
    let t0 = Instant::now();
    let e = run
        .run(&["sleep", "20"], Duration::from_secs(1))
        .unwrap_err();
    assert_eq!(e.kind(), ErrorKind::TimedOut);
    assert!(t0.elapsed() < Duration::from_secs(6), "{:?}", t0.elapsed());
    // la sessione resta usabile
    assert!(src.read(Path::new("/proc/stat")).is_ok());
    assert_eq!(
        run.run(&["echo", "ok"], Duration::from_secs(5))
            .unwrap()
            .stdout,
        b"ok\n"
    );
}

#[test]
fn piu_thread_in_parallelo() {
    let (src, run) = connect_local_sh().unwrap();
    let mut hs = Vec::new();
    for i in 0..8 {
        let (s, r) = (src.clone(), run.clone());
        hs.push(std::thread::spawn(move || {
            for _ in 0..20 {
                assert!(s.read(Path::new("/proc/loadavg")).is_ok());
                let o = r
                    .run(&["echo", &i.to_string()], Duration::from_secs(5))
                    .unwrap();
                assert_eq!(o.stdout, format!("{i}\n").into_bytes());
            }
        }));
    }
    for h in hs {
        h.join().unwrap();
    }
}

#[test]
fn refresh_manifesto_e_cache() {
    let (src, _) = connect_local_sh().unwrap();
    let me = std::process::id();
    let leggi = || {
        let pids = src.read_dir(Path::new("/proc")).unwrap();
        let mut n = 0;
        for p in pids
            .iter()
            .filter_map(|p| p.to_str())
            .filter(|p| p.bytes().all(|b| b.is_ascii_digit()))
        {
            for f in ["stat", "cmdline", "comm", "status"] {
                if src.read(Path::new(&format!("/proc/{p}/{f}"))).is_ok() {
                    n += 1;
                }
            }
            let _ = src.owner_uid(Path::new(&format!("/proc/{p}")));
        }
        for f in [
            "stat",
            "meminfo",
            "loadavg",
            "uptime",
            "net/dev",
            "diskstats",
            "mounts",
        ] {
            src.read(Path::new(&format!("/proc/{f}"))).unwrap();
        }
        n
    };
    // 1 aggiornamento: manifesto iniziale
    let t = Instant::now();
    src.begin_refresh();
    let t1 = t.elapsed();
    let b1 = src.remote_status().unwrap().last_bytes;
    let n = leggi();
    // 2: manifesto appreso
    let t = Instant::now();
    src.begin_refresh();
    let t2 = t.elapsed();
    let s = src.remote_status().unwrap();
    eprintln!(
        "MISURA begin_refresh: 1o {t1:?} ({b1} B), 2o {t2:?} ({} B), file letti {n}, rtt {:?} ms",
        s.last_bytes, s.latency_ms
    );
    assert!(s.last_bytes > 1000);
    assert_eq!(s.state, RemoteState::Connected);
    assert!(s.hostname.as_deref().is_some_and(|h| !h.is_empty()));
    // dai dati in cache: il nostro processo, senza altre richieste
    let st = src
        .read_text(Path::new(&format!("/proc/{me}/stat")))
        .unwrap();
    assert!(st.starts_with(&format!("{me} (")));
    let t = Instant::now();
    for _ in 0..1000 {
        src.read(Path::new(&format!("/proc/{me}/stat"))).unwrap();
    }
    assert!(
        t.elapsed() < Duration::from_millis(500),
        "cache lenta: {:?}",
        t.elapsed()
    );
    // un pid inesistente coperto dal modello: NotFound senza richieste
    assert_eq!(
        src.read(Path::new("/proc/4194300/stat"))
            .unwrap_err()
            .kind(),
        ErrorKind::NotFound
    );
    assert_eq!(
        src.owner_uid(Path::new(&format!("/proc/{me}"))).unwrap(),
        rustix::process::getuid().as_raw()
    );
}

#[test]
fn modelli_per_pid() {
    assert_eq!(template(b"/proc/12/stat").unwrap(), b"/proc/[0-9]*/stat");
    assert_eq!(template(b"/proc/12").unwrap(), b"/proc/[0-9]*");
    assert_eq!(
        template(b"/proc/12/task/34/stat").unwrap(),
        b"/proc/[0-9]*/task/[0-9]*/stat"
    );
    assert_eq!(template(b"/proc/12/fd").unwrap(), b"/proc/[0-9]*/fd");
    assert!(template(b"/proc/stat").is_none());
    assert!(template(b"/proc/net/dev").is_none());
    assert!(template(b"/etc/passwd").is_none());
}

#[test]
fn processo_che_muore_riconnessione() {
    let (src, _) = connect_local_sh().unwrap();
    assert!(src.read(Path::new("/proc/loadavg")).is_ok());
    src.kill_session_for_test();
    // il primo aggiornamento scopre la caduta
    let mut visto = false;
    for _ in 0..50 {
        src.begin_refresh();
        if !matches!(src.remote_status().unwrap().state, RemoteState::Connected) {
            visto = true;
            assert!(src.read(Path::new("/proc/uptime")).is_err());
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(visto, "la caduta non è stata rilevata");
    let t = Instant::now();
    loop {
        if src.remote_status().unwrap().state == RemoteState::Connected
            && src.read(Path::new("/proc/loadavg")).is_ok()
        {
            break;
        }
        assert!(t.elapsed() < Duration::from_secs(10), "nessuna ripresa");
        src.begin_refresh();
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn ssh_inesistente_da_errore_chiaro() {
    // un comando che esce subito: errore con il testo di stderr, non un blocco
    let tr = session::Transport::Ssh {
        cmd: vec![
            "sh".into(),
            "-c".into(),
            "echo boom >&2; exit 255".into(),
            "x".into(),
        ],
        target: Target::parse("h").unwrap(),
        compress: false,
    };
    let e = source::open(tr, "h".into()).err().expect("errore");
    assert!(e.to_string().contains("boom"), "{e}");
}

#[test]
fn argomenti_con_virgole_chiocciole_uguali() {
    let (src, run) = connect_local_sh().unwrap();
    let t = Duration::from_secs(5);
    let a = [
        "env",
        "LC_ALL=C",
        "printf",
        "%s|",
        "-p",
        "A,B,C",
        "u@h.service",
        "x=y",
        "a.b@c.d",
        "--no-pager",
    ];
    let o = run.run(&a, t).unwrap();
    assert_eq!(o.stdout, b"-p|A,B,C|u@h.service|x=y|a.b@c.d|--no-pager|");
    // remote_status resta Some sempre
    assert!(src.remote_status().is_some());
    src.kill_session_for_test();
    let _ = src.read(Path::new("/proc/uptime"));
    assert!(src.remote_status().is_some());
}

#[test]
fn modo_testo_e_batch_identici_a_locale() {
    let (r, _) = connect_local_sh().unwrap();
    let l = LocalSource;
    // modo testo (builtin) su /proc e /sys
    let mut n = 0;
    for p in [
        "/proc/mounts",
        "/proc/version",
        "/proc/sys/kernel/hostname",
        "/sys/class/net/lo/mtu",
        "/sys/class/net/lo/address",
    ] {
        assert_eq!(r.read(Path::new(p)).ok(), l.read(Path::new(p)).ok(), "{p}");
        n += 1;
    }
    assert!(n > 0);
    // file aperti dal test: link /proc/<me>/fd/N in batch, e cmdline di tutti i processi
    let me = std::process::id();
    let _aperti: Vec<_> = (0..6)
        .map(|_| std::fs::File::open("/etc/passwd").unwrap())
        .collect();
    let fd = |n: u32| PathBuf::from(format!("/proc/{me}/fd/{n}"));
    r.begin_refresh();
    let fds = l
        .read_dir(&PathBuf::from(format!("/proc/{me}/fd")))
        .unwrap();
    for _ in 0..2 {
        for f in &fds {
            let n: u32 = f.to_str().unwrap().parse().unwrap();
            if l.read_link(&fd(n)).is_err() {
                continue; // il descrittore della read_dir stessa
            }
            // letture da più pid (almeno 3) per far scattare il modello
            for pid in [me, 1, 2] {
                let _ = r.read_link(&PathBuf::from(format!("/proc/{pid}/fd/{n}")));
            }
            // i test girano in parallelo: gli fd di pipe altrui possono sparire tra le due letture
            if let (Ok(a), Ok(b)) = (r.read_link(&fd(n)), l.read_link(&fd(n)))
                && b.to_string_lossy().starts_with("/etc")
            {
                assert_eq!(a, b, "fd {n}");
            }
        }
        let pids: Vec<_> = l.read_dir(Path::new("/proc")).unwrap();
        for p in pids
            .iter()
            .filter_map(|p| p.to_str())
            .filter(|p| p.bytes().all(|b| b.is_ascii_digit()))
        {
            let _ = r.read(Path::new(&format!("/proc/{p}/cmdline")));
        }
        r.begin_refresh();
    }
    let (mut tot, mut uguali) = (0, 0);
    for p in l
        .read_dir(Path::new("/proc"))
        .unwrap()
        .iter()
        .filter_map(|p| p.to_str())
        .filter(|p| p.bytes().all(|b| b.is_ascii_digit()))
    {
        let path = PathBuf::from(format!("/proc/{p}/cmdline"));
        if let Ok(a) = l.read(&path) {
            tot += 1;
            if r.read(&path).ok().as_ref() == Some(&a) {
                uguali += 1;
            }
        }
    }
    assert!(tot > 0 && uguali * 100 >= tot * 95, "{uguali}/{tot}");
    assert_eq!(
        r.read_link(&fd(fds.len() as u32 + 1000))
            .unwrap_err()
            .kind(),
        ErrorKind::NotFound
    );
}

/// Prova contro una macchina vera: `NGTOP_TEST_TARGET=utente@host NGTOP_SSH="ssh -o BatchMode=yes" cargo test -- --ignored vps`
#[test]
#[ignore]
fn vps_reale() {
    let Ok(t) = std::env::var("NGTOP_TEST_TARGET") else {
        return;
    };
    let (src, run) = connect(&Target::parse(&t).unwrap(), &ConnectOptions::default()).unwrap();
    eprintln!("MISURA vps regime: {:?}", giri_realistici(&src, 6));
    eprintln!("{:?}", src.remote_status());
    let o = run.run(&["uname", "-a"], Duration::from_secs(5)).unwrap();
    eprintln!("{}", String::from_utf8_lossy(&o.stdout));
}

/// Simula i raccoglitori: stat di tutti i pid a ogni giro, status/cmdline/comm solo per i pid nuovi.
fn giri_realistici(src: &RemoteSource, giri: usize) -> Vec<(Duration, u64)> {
    giri_con(src, giri, std::collections::HashSet::new())
}

fn giri_realistici_da(src: &RemoteSource, giri: usize) {
    let visti = src
        .read_dir(Path::new("/proc"))
        .unwrap()
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    giri_con(src, giri, visti);
}

fn giri_con(
    src: &RemoteSource,
    giri: usize,
    mut visti: std::collections::HashSet<String>,
) -> Vec<(Duration, u64)> {
    let mut out = Vec::new();
    for _ in 0..giri {
        let t0 = Instant::now();
        src.begin_refresh();
        let b = src.remote_status().unwrap().last_bytes;
        for f in [
            "stat",
            "meminfo",
            "loadavg",
            "uptime",
            "net/dev",
            "diskstats",
            "mounts",
        ] {
            let _ = src.read(Path::new(&format!("/proc/{f}")));
        }
        let pids = src.read_dir(Path::new("/proc")).unwrap();
        for p in pids
            .iter()
            .filter_map(|p| p.to_str())
            .filter(|p| p.bytes().all(|b| b.is_ascii_digit()))
        {
            let _ = src.read(Path::new(&format!("/proc/{p}/stat")));
            if visti.insert(p.to_string()) {
                for f in ["status", "cmdline", "comm"] {
                    let _ = src.read(Path::new(&format!("/proc/{p}/{f}")));
                }
                let _ = src.owner_uid(Path::new(&format!("/proc/{p}")));
            }
        }
        out.push((t0.elapsed(), b));
    }
    out
}

#[test]
fn byte_a_regime() {
    let (src, _) = connect_local_sh().unwrap();
    let v = giri_realistici(&src, 6);
    eprintln!("MISURA regime locale: {v:?}");
    let npid = src.read_dir(Path::new("/proc")).unwrap().len() as u64;
    assert!(
        v[5].1 < 20_000 + 350 * npid,
        "{} byte a regime con {npid} pid",
        v[5].1
    );
}

/// CPU (tick di clock) consumata da sh e figli per giro a regime: `cargo test cpu_per_giro -- --ignored --nocapture`
#[test]
#[ignore]
fn cpu_per_giro() {
    let (src, _) = connect_local_sh().unwrap();
    let pid = src.session_pid_for_test().unwrap();
    let ticks = || {
        let s = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
        let f: Vec<&str> = s.rsplit_once(')').unwrap().1.split_whitespace().collect();
        f[11..15]
            .iter()
            .map(|x| x.parse::<u64>().unwrap())
            .sum::<u64>()
    };
    giri_realistici(&src, 4);
    let t0 = ticks();
    let n = 20;
    giri_realistici_da(&src, n);
    eprintln!(
        "MISURA cpu: {} tick (1/100 s) per giro, {} pid",
        (ticks() - t0) as f64 / n as f64,
        src.read_dir(Path::new("/proc")).unwrap().len()
    );
}

use super::source::test_support::{absorb_for_test, mounts_net_for_test};

#[test]
fn elenco_ls_falsificato_rifiutato() {
    // un file con nome ostile non deve poter inventare la cartella fd di un altro pid
    let vero = b"/proc/10/fd:\ntotal 0\nlrwx------ 1 u g 64 Oct 1 10:00 3 -> /dev/null\n\n";
    let falso = b"/proc/10/fd:\ntotal 0\nlrwx------ 1 u g 64 Oct 1 10:00 3 -> x\n\n/proc/1/fd:\nlrwx------ 1 u g 64 Oct 1 10:00 7 -> socket:[99]\n\n";
    let (n, redo) = absorb_for_test(b'M', vero);
    assert!(n > 0 && redo.is_empty());
    let doppio = b"/proc/10/fd:\nlrwx 1 u g 64 d 3 -> a\n\n/proc/10/fd:\nlrwx 1 u g 64 d 4 -> b\n";
    let (n, redo) = absorb_for_test(b'M', doppio);
    assert_eq!(n, 0);
    assert_eq!(redo, vec![b"/proc/10/fd".to_vec()]);
    let (n, redo) = absorb_for_test(b'M', falso);
    // intestazione valida ma nome di fd non numerico? qui l'inganno è una seconda sezione: ha forma valida
    // ma e' seguita da una riga senza intestazione coerente; il record e' accettato solo se tutto e' ben formato
    assert!(n == 0 || redo.is_empty());
    let (n, redo) = absorb_for_test(b'M', b"/etc/passwd:\nlrwx 1 u g 64 d 3 -> a\n");
    assert_eq!((n, redo.len()), (0, 0));
    let (n, _) = absorb_for_test(b'M', b"/proc/5/fd:\nlrwx 1 u g 64 d x3 -> a\n");
    assert_eq!(n, 0);
}

#[test]
fn lunghezze_ostili_non_vanno_in_panic() {
    // somma che va in overflow, lunghezza oltre il corpo, seconda misura diversa
    let enorme = format!("{} /a\n{} /b\n", usize::MAX, usize::MAX);
    for (b, c, q) in [
        (
            enorme.as_bytes().to_vec(),
            b"xx".to_vec(),
            enorme.as_bytes().to_vec(),
        ),
        (
            b"99999999999999999999999 /a\n".to_vec(),
            b"x".to_vec(),
            b"99999999999999999999999 /a\n".to_vec(),
        ),
        (b"3 /a\n".to_vec(), b"abc".to_vec(), b"4 /a\n".to_vec()),
        (b"3 /a\n".to_vec(), b"ab".to_vec(), b"3 /a\n".to_vec()),
    ] {
        let (n, redo) = absorb_for_test_bcq(&b, &c, &q);
        assert_eq!(n, 0);
        assert_eq!(redo, 1);
    }
    let (n, redo) = absorb_for_test_bcq(b"3 /a\n2 /b\n", b"abcde", b"3 /a\n2 /b\n");
    assert_eq!((n, redo), (2, 0));
}

fn absorb_for_test_bcq(b: &[u8], c: &[u8], q: &[u8]) -> (usize, usize) {
    super::source::test_support::absorb_bcq_for_test(b, c, q)
}

#[test]
fn mount_di_rete_riconosciuti() {
    let m = b"/dev/sda1 / ext4 rw 0 0\nsrv:/x /mnt/nfs\\040dir nfs4 rw 0 0\nsshfs#a /mnt/s fuse.sshfs rw 0 0\nfusectl /sys/fs/fuse/connections fusectl rw 0 0\n//h/s /mnt/c cifs rw 0 0\n";
    let mut v: Vec<String> = mounts_net_for_test(m)
        .iter()
        .map(|x| String::from_utf8_lossy(x).into_owned())
        .collect();
    v.sort();
    assert_eq!(v, vec!["/mnt/c", "/mnt/nfs dir", "/mnt/s"]);
}

#[test]
fn page_size_e_tick_dal_server() {
    let (src, _) = connect_local_sh().unwrap();
    assert_eq!(src.page_size(), Some(rustix::param::page_size() as u64));
    assert_eq!(
        src.clock_ticks(),
        Some(rustix::param::clock_ticks_per_second())
    );
}

#[test]
fn troppo_output_prima_di_ready() {
    let tr = session::Transport::Ssh {
        cmd: vec![
            "sh".into(),
            "-c".into(),
            "head -c 3000000 /dev/zero | tr '\\0' x; sleep 5".into(),
            "x".into(),
        ],
        target: Target::parse("h").unwrap(),
        compress: false,
    };
    let t = Instant::now();
    let e = source::open(tr, "h".into()).err().expect("errore");
    assert_eq!(e.kind(), ErrorKind::InvalidData, "{e}");
    assert!(t.elapsed() < Duration::from_secs(4));
}

#[test]
fn sessione_occupata_non_blocca_il_giro() {
    let (src, run) = connect_local_sh().unwrap();
    src.begin_refresh();
    let r2 = run.clone();
    let h = std::thread::spawn(move || r2.run(&["sleep", "2"], Duration::from_secs(5)));
    std::thread::sleep(Duration::from_millis(300));
    let t = Instant::now();
    src.begin_refresh(); // salta il giro
    let e = src.read(Path::new("/proc/4194301/uptime-non-in-cache"));
    assert!(e.is_err());
    assert!(
        t.elapsed() < Duration::from_millis(900),
        "{:?}",
        t.elapsed()
    );
    // i dati già in cache restano leggibili
    assert!(src.read(Path::new("/proc/stat")).is_ok());
    h.join().unwrap().unwrap();
    src.begin_refresh();
    assert!(src.read(Path::new("/proc/stat")).is_ok());
}

#[test]
fn nome_di_file_ostile_non_falsifica_i_link_fd() {
    let (r, _) = connect_local_sh().unwrap();
    let d = tmpdir("evil");
    // il percorso del file, stampato da ls, contiene "\n\n/proc/1/fd:\nlrwx… 7 -> socket:[99]"
    let dir = d.join("x\n\n").join("proc").join("1");
    std::fs::create_dir_all(&dir).unwrap();
    let _f =
        std::fs::File::create(dir.join("fd:\nlrwx------ 1 u g 64 Oct 1 10:00 7 -> socket:[99]"))
            .unwrap();
    let me = std::process::id();
    let fds = r.read_dir(Path::new(&format!("/proc/{me}/fd"))).unwrap();
    assert!(!fds.is_empty());
    assert!(r.read_link(Path::new("/proc/1/fd/7")).is_err());
    assert!(
        r.read_dir(Path::new("/proc/1/fd")).is_err() || rustix::process::getuid().as_raw() == 0
    );
}

#[test]
fn mount_di_rete_mai_nel_lotto_h() {
    // il "server" dichiara /tmp come nfs4 tramite /proc/mounts e /proc/self/mountinfo (finti, ma percorsi veri)
    for (mounts, mountinfo, legge_mountinfo) in [
        (Some(&b"srv:/x /tmp nfs4 rw 0 0\n"[..]), None, false),
        (
            None,
            Some(&b"36 35 0:30 / /tmp rw - nfs4 srv:/x rw\n"[..]),
            true,
        ),
    ] {
        let (src, _) = connect_local_sh().unwrap();
        super::source::test_support::set_fake_mounts_for_test(&src, mounts, mountinfo);
        for _ in 0..6 {
            src.begin_refresh();
            if legge_mountinfo {
                // come il raccoglitore dei dischi: legge mountinfo a ogni giro
                let _ = src.read(Path::new("/proc/self/mountinfo"));
            }
            let _ = src.statvfs(Path::new("/tmp"));
            assert!(src.statvfs(Path::new("/")).is_ok());
        }
        let sent =
            String::from_utf8_lossy(&super::source::test_support::sent_for_test(&src)).into_owned();
        let h: Vec<&str> = sent
            .lines()
            .filter(|l| l.starts_with("H ") || l.starts_with("s "))
            .collect();
        assert!(!h.is_empty(), "il lotto H non è mai stato usato");
        assert!(
            h.iter().all(|l| !l.contains("'/tmp'")),
            "/tmp nel lotto H: {h:?}"
        );
        assert!(
            sent.lines().any(|l| l.starts_with("h '/tmp'")),
            "nessuna lettura con scadenza"
        );
    }
}

#[test]
fn mountinfo_riconosce_i_mount_di_rete() {
    let mi = b"36 35 98:0 /mnt1 /mnt/nfs\\040dir rw,noatime master:1 - nfs4 srv:/x rw\n1 0 8:1 / / rw - ext4 /dev/sda1 rw\n";
    let mut c = Vec::new();
    c.extend(super::source::test_support::mountinfo_net_for_test(mi));
    assert_eq!(c, vec![b"/mnt/nfs dir".to_vec()]);
}
