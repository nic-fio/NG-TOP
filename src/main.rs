// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

use std::fmt::Write as _;
use std::io::Write as _;
use std::thread::sleep;
use std::time::Duration;

use ngtop::collect::process::{SortKey, sort_processes, tree_order};
use ngtop::collect::{Roots, System};
use ngtop::ui::app::Section;

fn main() {
    let start = std::time::Instant::now();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--doc-json") {
        write_stdout(&ngtop::ui::doc::doc_json());
    } else if args.iter().any(|a| a == "--dump") {
        let roots = roots_for(&args);
        dump(args.iter().any(|a| a == "--tree"), roots);
    } else if args.iter().any(|a| a == "--bench-start") {
        bench_start(start);
    } else if args.iter().any(|a| a == "--bench") {
        bench();
    } else if let Some(i) = args.iter().position(|a| a == "--snapshot") {
        snapshot(&args, i);
    } else {
        interactive(&args);
    }
}

/// Radici di lettura per `--dump` / `--snapshot`: la macchina remota se c'è un TARGET
/// (posizionale o `--host`), altrimenti questa.
fn roots_for(args: &[String]) -> Roots {
    match ngtop::config::find_target(args) {
        Ok(Some(t)) => connect_remote(&t),
        Ok(None) => Roots::local(),
        Err(e) => {
            eprintln!("ngtop: {e}\nTry 'ngtop --help'.");
            std::process::exit(2);
        }
    }
}

/// Apre la connessione ssh PRIMA di toccare il terminale (ssh può chiedere password o host key).
/// Errore: messaggio chiaro su stderr ed exit 2.
fn connect_remote(target: &str) -> Roots {
    use ngtop::remote::{ConnectOptions, Target};
    let t = Target::parse(target).unwrap_or_else(|e| {
        eprintln!("ngtop: invalid TARGET '{target}': {e}");
        std::process::exit(2);
    });
    eprintln!("Connecting to {}…", t.display());
    match ngtop::remote::connect(&t, &ConnectOptions::default()) {
        Ok((src, run)) => {
            let act = ngtop::remote::actions_for(&src);
            Roots::remote(src, run).with_actions(act)
        }
        Err(e) => {
            // l'errore contiene l'ultima riga di stderr di ssh, che può venire dal server
            let e: String = e
                .to_string()
                .chars()
                .map(|c| if c.is_control() { '?' } else { c })
                .collect();
            eprintln!("ngtop: cannot connect to {}: {e}", t.display());
            std::process::exit(2);
        }
    }
}

/// Avvio normale: riga di comando, file di configurazione, poi la TUI.
fn interactive(args: &[String]) {
    use ngtop::config;
    let cli = match config::parse_args(args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("ngtop: {e}\nTry 'ngtop --help'.");
            std::process::exit(2);
        }
    };
    if cli.help {
        write_stdout(config::USAGE);
        return;
    }
    if cli.version {
        write_stdout(&format!(
            "ngtop {}\nLicence: PolyForm Noncommercial 1.0.0 — free for noncommercial use\n",
            env!("CARGO_PKG_VERSION")
        ));
        return;
    }
    // Senza un terminale interattivo la TUI non può partire: messaggio chiaro invece di un errore di I/O.
    {
        use std::io::IsTerminal;
        if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
            eprintln!(
                "ngtop: an interactive terminal is required (run it from a terminal window). \
                 For a text summary use: ngtop --dump"
            );
            std::process::exit(2);
        }
    }
    // File: quello indicato con --config (deve esistere) oppure il predefinito (può mancare).
    let path = cli.config.clone().or_else(config::default_path_from_env);
    let file = match &path {
        Some(p) => {
            let (cfg, warnings) = config::load(p, cli.config.is_some()).unwrap_or_else(|e| {
                eprintln!("ngtop: configuration {}: {e}", p.display());
                std::process::exit(2);
            });
            for w in warnings {
                eprintln!("ngtop: configuration {}: warning: {w}", p.display());
            }
            cfg
        }
        None => config::Config::default(),
    };
    let effective = config::merge(file.clone(), &cli);
    // Con un TARGET (riga di comando o file) ci si collega ora, prima della modalità raw.
    let roots = match &effective.host {
        Some(h) => connect_remote(h),
        None => Roots::local(),
    };
    let colors = ngtop::ui::theme::ColorMode::detect(
        effective.colors,
        std::env::var("COLORTERM").ok().as_deref(),
        std::env::var("TERM").ok().as_deref(),
    );
    let opts = ngtop::ui::RunOptions {
        effective,
        file,
        config_path: path,
        colors,
        roots,
    };
    if let Err(e) = ngtop::ui::run(opts) {
        eprintln!("ngtop: terminal error: {e}");
        std::process::exit(1);
    }
}

/// `--snapshot LxA [--section N] [--ansi]`: un fotogramma su backend di prova.
/// Scrive su stdout ignorando gli errori (pipe chiusa → niente panic).
fn write_stdout(s: &str) {
    let mut o = std::io::stdout().lock();
    let _ = o.write_all(s.as_bytes());
    let _ = o.flush();
}

// I testi del riepilogo vengono anche da una macchina remota (nomi di processo, righe di comando):
// i caratteri di controllo diventano '?', così nessuna sequenza di escape arriva al terminale.
macro_rules! outln {
    ($o:expr, $($a:tt)*) => {{
        let line: String = format!($($a)*)
            .chars()
            .map(|c| if c.is_control() { '?' } else { c })
            .collect();
        let _ = writeln!($o, "{line}");
    }};
}

fn snapshot(args: &[String], i: usize) {
    let size = args.get(i + 1).and_then(|s| {
        let (w, h) = s.split_once('x')?;
        Some((w.parse::<u16>().ok()?, h.parse::<u16>().ok()?))
    });
    let Some((w, h)) = size.filter(|&(w, h)| w > 0 && h > 0) else {
        eprintln!("usage: ngtop --snapshot WxH [--section N] [--ansi]");
        std::process::exit(2);
    };
    let section = args
        .iter()
        .position(|a| a == "--section")
        .and_then(|j| args.get(j + 1))
        .and_then(|n| n.parse::<usize>().ok())
        .and_then(|n| n.checked_sub(1))
        .and_then(|n| Section::ALL.get(n).copied())
        .unwrap_or(Section::Panoramica);
    // `--roots DIR` (nascosto): legge DIR/proc, DIR/sys e DIR/etc invece del sistema vero.
    let roots = match args
        .iter()
        .position(|a| a == "--roots")
        .and_then(|j| args.get(j + 1))
    {
        Some(d) => {
            let d = std::path::PathBuf::from(d);
            Roots::with_dirs(d.join("proc"), d.join("sys"), d.join("etc"))
        }
        None => roots_for(args),
    };
    let use_roots = args.iter().any(|a| a == "--roots");
    let mut sys = System::new(roots);
    let mut app = ngtop::ui::app::App::new((w, h), ngtop::ui::hostname(&sys));
    app.remote = sys.remote_status().is_some();
    app.section = section;
    if app.remote {
        app.servizi = ngtop::ui::services_for(&sys);
    }
    // Con `--roots` la sezione Services usa il backend finto (dati dimostrativi, niente D-Bus).
    if use_roots && section == Section::Servizi {
        app.servizi = ngtop::ui::sections::servizi::Servizi::with_backend(
            std::sync::Arc::new(ngtop::collect::services::fake::FakeBackend::demo()),
            std::env::temp_dir(),
            std::path::PathBuf::from("/nonexistent"),
        );
        let _ = std::fs::create_dir_all(std::env::temp_dir().join("systemd/system"));
    }
    app.tz = ngtop::ui::tz::Tz::load(&sys.roots);
    // Circa 20 campioni ravvicinati, così gli storici dei grafici non sono vuoti.
    sys.collect_conns = section == Section::ReteDischi;
    for _ in 0..20 {
        sys.refresh();
        app.update(&sys);
        sleep(Duration::from_millis(100));
    }
    // `--select-pid N` (nascosto): seleziona quel processo restando nella sezione scelta.
    if let Some(pid) = args
        .iter()
        .position(|a| a == "--select-pid")
        .and_then(|j| args.get(j + 1))
        .and_then(|n| n.parse::<u32>().ok())
    {
        app.jump_to_pid(pid);
        if app.section != section {
            app.set_section(section);
        }
        app.update(&sys);
    }
    // `--overlay help|signal|confirm|filter` (nascosto): finestra aperta come con i tasti veri.
    if let Some(ov) = args
        .iter()
        .position(|a| a == "--overlay")
        .and_then(|j| args.get(j + 1))
    {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut press = |c: KeyCode| {
            app.on_key(KeyEvent::new(c, KeyModifiers::NONE));
        };
        match ov.as_str() {
            "help" => press(KeyCode::F(1)),
            "signal" => press(KeyCode::F(9)),
            "confirm" => press(KeyCode::Char('r')),
            "filter" => {
                press(KeyCode::Char('/'));
                for c in "fire".chars() {
                    press(KeyCode::Char(c));
                }
            }
            other => {
                eprintln!("unknown overlay '{other}' (help, signal, confirm, filter)");
                std::process::exit(2);
            }
        }
        app.update(&sys);
    }
    // Con `--roots` (figure dei manuali) gli storici dei grafici si riempiono con campioni sintetici.
    if use_roots {
        ngtop::ui::snapshot::synthesize_history(&mut app);
    }
    let buf = ngtop::ui::snapshot::render_app(&sys, &mut app);
    if args.iter().any(|a| a == "--ansi") {
        write_stdout(&ngtop::ui::snapshot::to_ansi(&buf));
    } else {
        write_stdout(&ngtop::ui::snapshot::to_text(&buf));
    }
}

/// `--bench-start` (nascosto): tempo dall'avvio di `main` al primo fotogramma su un backend di
/// prova 182x52 (stessa sequenza dell'avvio vero, senza terminale). Il primo valore è "a freddo".
fn bench_start(main_start: std::time::Instant) {
    let first_frame = || {
        let mut sys = System::new(Roots::local());
        sys.refresh();
        let mut app = ngtop::ui::app::App::new((182, 52), ngtop::ui::hostname(&sys));
        app.tz = ngtop::ui::tz::Tz::load(&sys.roots);
        app.update(&sys);
        let _ = ngtop::ui::snapshot::render_app(&sys, &mut app);
    };
    first_frame();
    let cold = main_start.elapsed();
    let mut times = Vec::new();
    for _ in 0..10 {
        let t = std::time::Instant::now();
        first_frame();
        times.push(t.elapsed());
    }
    let mean = times.iter().sum::<std::time::Duration>() / 10;
    let min = times.iter().min().copied().unwrap_or_default();
    write_stdout(&format!(
        "startup to first frame: cold {:.1} ms; mean of 10 {:.1} ms (min {:.1} ms)\n",
        cold.as_secs_f64() * 1000.0,
        mean.as_secs_f64() * 1000.0,
        min.as_secs_f64() * 1000.0
    ));
}

/// `--bench` (nascosto): tempo medio per ciclo (refresh + aggiornamento + disegno a 182x52)
/// con ciascuna sezione visibile.
fn bench() {
    use std::time::Instant;
    let mut sys = System::new(Roots::local());
    let mut app = ngtop::ui::app::App::new((182, 52), ngtop::ui::hostname(&sys));
    sys.refresh();
    app.update(&sys);
    let n = 30u32;
    let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0 / f64::from(n);
    let mut out = String::new();
    for sec in Section::ALL {
        app.set_section(sec);
        sys.collect_conns = sec == Section::ReteDischi;
        for _ in 0..3 {
            sys.refresh();
            app.update(&sys);
            let _ = ngtop::ui::snapshot::render_app(&sys, &mut app);
        }
        let (mut tr, mut tu, mut td) = (
            std::time::Duration::ZERO,
            std::time::Duration::ZERO,
            std::time::Duration::ZERO,
        );
        for _ in 0..n {
            let t = Instant::now();
            sys.refresh();
            tr += t.elapsed();
            let t = Instant::now();
            app.update(&sys);
            tu += t.elapsed();
            let t = Instant::now();
            let _ = ngtop::ui::snapshot::render_app(&sys, &mut app);
            td += t.elapsed();
        }
        outln!(
            out,
            "{} {:<18} refresh {:5.2} + update {:4.2} + draw {:4.2} = {:5.2} ms",
            Section::ALL
                .iter()
                .position(|s| *s == sec)
                .map_or(0, |i| i + 1),
            sec.title(),
            ms(tr),
            ms(tu),
            ms(td),
            ms(tr + tu + td)
        );
    }
    outln!(
        out,
        "({} processes, {} GPU(s))",
        sys.processes.len(),
        sys.gpu.devices.len()
    );
    write_stdout(&out);
}

fn gib(b: u64) -> String {
    format!("{:.2} GiB", b as f64 / 1_073_741_824.0)
}

fn dump(tree: bool, roots: Roots) {
    let mut out = String::new();
    let mut sys = System::new(roots);
    sys.refresh();
    sleep(Duration::from_millis(500));
    sys.refresh();

    if let Some(st) = sys.remote_status() {
        outln!(
            out,
            "Host: {} (via ssh, {})",
            ngtop::ui::hostname(&sys),
            st.target
        );
    }
    let c = &sys.cpu;
    outln!(
        out,
        "CPU: {} ({} logical, {} physical)",
        c.model,
        c.logical,
        c.physical.map_or("?".to_string(), |p| p.to_string())
    );
    outln!(out, "  total {:5.1}%", c.total_usage);
    for core in &c.cores {
        let f = core
            .freq_mhz
            .map_or("— MHz".to_string(), |f| format!("{f} MHz"));
        outln!(out, "  core {:2}  {:5.1}%  {}", core.id, core.usage, f);
    }
    let m = &sys.mem;
    outln!(
        out,
        "Memory: total {} used {} available {} cache {} buffers {} free {}",
        gib(m.total),
        gib(m.used),
        gib(m.available),
        gib(m.cache),
        gib(m.buffers),
        gib(m.free)
    );
    outln!(
        out,
        "Swap: total {} used {} free {}{}",
        gib(m.swap_total),
        gib(m.swap_used),
        gib(m.swap_free),
        m.zswap
            .map_or(String::new(), |z| format!(" (zswap {})", gib(z)))
    );
    let l = &sys.load;
    outln!(
        out,
        "Load: {:.2} {:.2} {:.2}  processes {}/{}",
        l.one,
        l.five,
        l.fifteen,
        l.running,
        l.total
    );
    let up = sys.uptime.up_secs as u64;
    outln!(
        out,
        "Uptime: {}d {:02}:{:02}:{:02}",
        up / 86400,
        up % 86400 / 3600,
        up % 3600 / 60,
        up % 60
    );

    outln!(
        out,
        "{:>7} {:<10} {:>6} {:>10} COMMAND",
        "PID",
        "USER",
        "CPU%",
        "MEM"
    );
    let mut procs = sys.processes.clone();
    let rows: Vec<(usize, String)> = if tree {
        // Albero: mostra i 15 con più CPU, mantenendo l'ordine di albero.
        let mut top: Vec<u32> = {
            let mut v = procs.clone();
            sort_processes(&mut v, SortKey::Cpu, true);
            v.iter().take(15).map(|p| p.pid).collect()
        };
        top.sort_unstable();
        tree_order(&procs)
            .into_iter()
            .filter(|(i, _)| top.binary_search(&procs[*i].pid).is_ok())
            .collect()
    } else {
        sort_processes(&mut procs, SortKey::Cpu, true);
        (0..procs.len().min(15))
            .map(|i| (i, String::new()))
            .collect()
    };
    for (i, prefix) in rows {
        let p = &procs[i];
        outln!(
            out,
            "{:>7} {:<10} {:>6.1} {:>10} {}{}",
            p.pid,
            p.user.chars().take(10).collect::<String>(),
            p.cpu_percent,
            gib(p.rss),
            prefix,
            p.cmdline.chars().take(60).collect::<String>()
        );
    }
    write_stdout(&out);
}
