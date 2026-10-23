// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Raccoglitore GPU e viste GPU su fixture realistiche (amdgpu, i915, xe, sospesa, NVIDIA).

use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ngtop::collect::gpu::{GpuState, WorkKind};
use ngtop::collect::{Roots, System};
use ngtop::source::{CommandOutput, CommandRunner};
use ngtop::ui::app::{App, Section};
use ngtop::ui::snapshot::{render_app, to_ansi, to_text};

type Handler = Box<dyn Fn(&[&str]) -> io::Result<CommandOutput> + Send + Sync>;

/// Comandi finti: registra le chiamate e risponde con `f`.
struct FakeRun {
    calls: Mutex<Vec<Vec<String>>>,
    timeouts: Mutex<Vec<Duration>>,
    f: Handler,
}

impl FakeRun {
    fn new(
        f: impl Fn(&[&str]) -> io::Result<CommandOutput> + Send + Sync + 'static,
    ) -> Arc<FakeRun> {
        Arc::new(FakeRun {
            calls: Mutex::new(Vec::new()),
            timeouts: Mutex::new(Vec::new()),
            f: Box::new(f),
        })
    }
    /// Nessun comando esiste.
    fn none() -> Arc<FakeRun> {
        FakeRun::new(|_| Err(io::Error::new(io::ErrorKind::NotFound, "nvidia-smi")))
    }
    fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
    }
}

impl CommandRunner for FakeRun {
    fn run(&self, argv: &[&str], t: Duration) -> io::Result<CommandOutput> {
        // `sh -c GUARDIA sh N <file...> nvidia-smi ...`: si registra solo il comando dopo la guardia
        // (la guardia vera è provata nei test di unità del worker)
        assert_eq!(&argv[..2], ["sh", "-c"], "{argv:?}");
        let n: usize = argv[4].parse().unwrap();
        let argv = &argv[5 + n..];
        self.timeouts.lock().unwrap().push(t);
        self.calls
            .lock()
            .unwrap()
            .push(argv.iter().map(|s| s.to_string()).collect());
        (self.f)(argv)
    }
}

fn out(text: &str) -> io::Result<CommandOutput> {
    Ok(CommandOutput {
        status: Some(0),
        stdout: text.as_bytes().to_vec(),
        stderr: Vec::new(),
    })
}

fn out_rc(rc: i32, stdout: &str, stderr: &str) -> io::Result<CommandOutput> {
    Ok(CommandOutput {
        status: Some(rc),
        stdout: stdout.as_bytes().to_vec(),
        stderr: stderr.as_bytes().to_vec(),
    })
}

fn nv_fixture(name: &str) -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/nvidia")
            .join(name),
    )
    .unwrap()
}

fn roots(name: &str) -> Roots {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/gpu")
        .join(name);
    let mut r = Roots::with_dirs(base.join("proc"), base.join("sys"), base.join("etc"));
    // mai nvidia-smi vero nei test
    r.run = FakeRun::none();
    r
}

fn system_with(name: &str, run: Arc<FakeRun>) -> System {
    let mut s = new_system(name);
    s.roots.run = run;
    s
}

/// Rinfresca finché `cond` è vera (al massimo 5 s).
fn wait_for(s: &mut System, what: &str, cond: impl Fn(&System) -> bool) {
    let t0 = Instant::now();
    loop {
        s.refresh_with_dt(1.0);
        if cond(s) {
            return;
        }
        assert!(t0.elapsed() < Duration::from_secs(5), "timeout: {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn new_system(name: &str) -> System {
    let mut s = System::new(roots(name));
    s.clock_ticks = 100;
    s.page_size = 4096;
    s
}

/// Due campioni: `<nome>_a` poi `<nome>_b`, a 1 s di distanza.
fn pair(name: &str) -> System {
    let mut s = new_system(&format!("{name}_a"));
    s.refresh_with_dt(0.0);
    s.roots = roots(&format!("{name}_b"));
    s.refresh_with_dt(1.0);
    s
}

fn single(name: &str) -> System {
    let mut s = new_system(name);
    s.refresh_with_dt(0.0);
    s.refresh_with_dt(1.0);
    s
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn amdgpu_dispositivo() {
    let s = pair("amd");
    assert_eq!(s.gpu.devices.len(), 1);
    let g = &s.gpu.devices[0];
    assert_eq!(g.state, GpuState::Active);
    assert_eq!(g.name, "AMD Radeon RX 6800/6800 XT / 6900 XT");
    assert_eq!(
        (g.driver.as_str(), g.pdev.as_str()),
        ("amdgpu", "0000:03:00.0")
    );
    assert!(!g.integrated);
    assert_eq!(g.usage, Some(86.0));
    assert_eq!(g.vram_used, Some(12_670_153_728));
    assert_eq!(g.vram_total, Some(17_163_091_968));
    assert_eq!(g.clock_mhz, Some(1800));
    assert_eq!(g.clock_max_mhz, Some(2400));
    assert_eq!(g.mem_clock_mhz, Some(1000));
    assert_eq!(g.temp, Some(71.0));
    assert_eq!(g.temps.len(), 3);
    assert_eq!(g.power_w, Some(220.0));
    assert_eq!(g.power_cap_w, Some(203.0));
    assert_eq!(g.fan_rpm, Some(1650));
    assert!(near(g.fan_pct.unwrap_or(0.0), 122.0 / 255.0 * 100.0));
    assert_eq!(g.pcie.as_deref(), Some("4.0 ×16"));
    // motori sommando i client: gfx 60 + 5 + 2, compute 50, enc 30
    let e = |n: &str| g.engines.iter().find(|(k, _)| k == n).map(|(_, v)| *v);
    assert!(near(e("gfx").unwrap_or(-1.0), 67.0));
    assert!(near(e("compute").unwrap_or(-1.0), 50.0));
    assert!(near(g.enc.unwrap_or(-1.0), 30.0));
}

#[test]
fn amdgpu_processi_dedup_e_tipo() {
    let s = pair("amd");
    let get = |pid: u32| s.gpu.procs.iter().find(|p| p.pid == pid);
    // il client 7 compare in 3 fd di 2 processi: contato una volta, sul pid più basso
    let game = get(1000).expect("1000");
    assert!(near(game.gpu_pct, 60.0));
    assert!(near(game.enc_pct, 30.0));
    assert_eq!(game.kind, WorkKind::Graphics);
    assert_eq!(game.mem, 4 * 1024 * 1024 * 1024);
    assert!(get(1003).is_none());
    let blender = get(1001).expect("1001");
    assert!(near(blender.gpu_pct, 50.0));
    assert_eq!(blender.kind, WorkKind::Compute);
    assert_eq!(blender.mem, 2 * 1024 * 1024 * 1024);
    // due client dello stesso processo: 5 % + 2 %
    let ff = get(1002).expect("1002");
    assert!(near(ff.gpu_pct, 7.0));
    assert_eq!(ff.mem, 512 * 1024 * 1024);
    // niente GPU, o fdinfo illeggibile: assenti, senza errori
    assert!(get(1004).is_none() && get(1005).is_none());
    assert_eq!(s.gpu.procs.len(), 3);
    // i valori arrivano anche sui processi
    let p = s.processes.iter().find(|p| p.pid == 1000).unwrap();
    assert!(near(p.gpu_percent, 60.0));
    assert_eq!(p.gpu_mem, 4 * 1024 * 1024 * 1024);
}

#[test]
fn primo_campione_a_zero() {
    let mut s = new_system("amd_a");
    s.refresh_with_dt(0.0);
    assert!(
        s.gpu
            .procs
            .iter()
            .all(|p| p.gpu_pct == 0.0 && p.enc_pct == 0.0)
    );
    assert!(s.gpu.devices[0].engines.iter().all(|(_, v)| *v == 0.0));
}

#[test]
fn i915_questo_pc() {
    let s = pair("i915");
    let g = &s.gpu.devices[0];
    assert_eq!(g.name, "Intel UHD Graphics");
    assert!(g.integrated);
    assert_eq!(g.clock_mhz, Some(650));
    assert_eq!(g.clock_max_mhz, Some(750));
    assert!(g.pcie.is_none());
    let e = |n: &str| g.engines.iter().find(|(k, _)| k == n).map(|(_, v)| *v);
    // render: 25 % + 10 %; video 20 %
    assert!(near(e("render").unwrap_or(-1.0), 35.0));
    assert!(near(e("video").unwrap_or(-1.0), 20.0));
    // uso = massimo dei motori
    assert!(near(g.usage.unwrap_or(-1.0), 35.0));
    assert!(near(g.video.unwrap_or(-1.0), 20.0));
    // memoria residente dei client (stolen escluso)
    assert_eq!(g.sys_mem, Some((212860 + 1000) * 1024));
    let ff = s.gpu.procs.iter().find(|p| p.pid == 2001).unwrap();
    assert!(near(ff.gpu_pct, 10.0));
    assert!(near(ff.dec_pct, 20.0));
    assert_eq!(ff.kind, WorkKind::Graphics);
}

#[test]
fn xe_cicli() {
    let s = pair("xe");
    let g = &s.gpu.devices[0];
    assert_eq!(g.driver, "xe");
    assert_eq!(g.clock_mhz, Some(1100));
    assert_eq!(g.clock_max_mhz, Some(1500));
    // (400-100) / (2000-1000) = 30 %
    assert!(near(g.usage.unwrap_or(-1.0), 30.0));
    let p = &s.gpu.procs[0];
    assert!(near(p.gpu_pct, 30.0));
    assert_eq!(p.mem, 2048 * 1024);
}

#[test]
fn gpu_sospesa_non_viene_interrogata() {
    let s = single("susp");
    let g = &s.gpu.devices[0];
    assert_eq!(g.state, GpuState::Suspended);
    // i file esistono ma non vanno letti
    assert_eq!(g.usage, None);
    assert_eq!(g.vram_total, None);
    assert_eq!(g.temp, None);
    assert_eq!(g.power_w, None);
    assert_eq!(g.clock_mhz, None);
    assert_eq!(g.name, "AMD Radeon RX 6800/6800 XT / 6900 XT");
}

#[test]
fn integrata_con_runtime_pm_si_legge_comunque() {
    let s = single("i915_idle");
    let g = &s.gpu.devices[0];
    // niente "sospesa": è inattiva, uso 0, frequenza non disponibile
    assert_eq!(g.state, GpuState::Idle);
    assert_eq!(g.usage, Some(0.0));
    assert_eq!(g.clock_mhz, None);
    assert_eq!(g.clock_max_mhz, Some(750));
    let t = text(&s, 120, 40, Section::Gpu);
    assert!(t.contains("idle"), "{t}");
    assert!(!t.contains("not queried"), "{t}");
}

#[test]
fn intel_discreta_su_altro_bus() {
    let s = pair("i915_arc");
    let g = &s.gpu.devices[0];
    assert!(!g.integrated);
    assert_eq!(g.pcie.as_deref(), Some("3.0 ×16"));
    // drm-resident-local0 conta come VRAM del processo
    let p = s.gpu.procs.iter().find(|p| p.pid == 2000).unwrap();
    assert_eq!(p.mem, 212860 * 1024);
}

#[test]
fn testo_sospesa_d3cold_o_solo_rpm() {
    let a = text(&single("susp"), 120, 40, Section::Gpu);
    assert!(a.contains("suspended (D3cold) — not queried"), "{a}");
    let b = text(&single("susp_rpm"), 120, 40, Section::Gpu);
    assert!(b.contains("suspended — not queried"), "{b}");
    assert!(!b.contains("D3cold"));
}

#[test]
fn nvidia_senza_nvidia_smi() {
    let mut s = new_system("nvidia");
    wait_for(&mut s, "NoTool", |s| {
        s.gpu.devices[0].state == GpuState::NoTool
    });
    let g = &s.gpu.devices[0];
    assert_eq!(g.name, "NVIDIA GeForce RTX 3090");
    assert_eq!(g.usage, None);
}

#[test]
fn senza_gpu_e_nome_di_ripiego() {
    let s = single("none");
    assert!(s.gpu.devices.is_empty());
    let mut s = new_system("nvidia");
    // senza pci.ids: nome generico con gli id
    s.roots.etc = std::env::temp_dir().join("ngtop-nessun-etc/etc");
    s.refresh_with_dt(0.0);
    assert_eq!(s.gpu.devices[0].name, "NVIDIA GPU [10de:2204]");
}

fn text(sys: &System, w: u16, h: u16, section: Section) -> String {
    let mut app = App::new((w, h), "host".into());
    app.section = section;
    for _ in 0..3 {
        app.update(sys);
    }
    to_text(&render_app(sys, &mut app))
}

#[test]
fn viste_con_amdgpu() {
    let s = pair("amd");
    for (w, h) in [(182, 52), (120, 40), (100, 30), (80, 24)] {
        let t = text(&s, w, h, Section::Gpu);
        assert_eq!(t.lines().count(), h as usize);
        assert!(t.contains("Radeon RX 6800"), "{w}x{h}\n{t}");
        assert!(t.contains("GPU processes"), "{w}x{h}");
        assert!(t.contains("blender"), "{w}x{h}\n{t}");
        assert!(
            !t.contains("editor"),
            "{w}x{h}: process without GPU in table"
        );
        let t = text(&s, w, h, Section::Panoramica);
        assert_eq!(t.lines().count(), h as usize);
        assert!(t.contains("GPU"), "{w}x{h}");
    }
    let t = text(&s, 182, 52, Section::Gpu);
    for needle in [
        "4.0 ×16",
        "71°C",
        "1650 RPM",
        "220/203 W",
        "GPU 0 history",
        "engines · GPU 0",
        "power",
        "Compute",
        "Graphics",
    ] {
        assert!(t.contains(needle), "missing {needle}\n{t}");
    }
    let p = text(&s, 182, 52, Section::Panoramica);
    for needle in ["╭─ gpu0", "VRAM", "Fan", "GPU%"] {
        assert!(p.contains(needle), "missing {needle}\n{p}");
    }
    let c = text(&s, 80, 24, Section::Panoramica);
    assert!(c.contains("GPU "), "{c}");
}

#[test]
fn viste_gpu_sospesa_nvidia_e_nessuna() {
    let s = single("susp");
    assert!(text(&s, 120, 40, Section::Gpu).contains("not queried, to avoid waking it"));
    assert!(text(&s, 182, 52, Section::Panoramica).contains("suspended"));
    let mut n = new_system("nvidia");
    wait_for(&mut n, "NoTool", |s| {
        s.gpu.devices[0].state == GpuState::NoTool
    });
    assert!(text(&n, 120, 40, Section::Gpu).contains("install nvidia-smi"));
    let z = single("none");
    assert!(text(&z, 120, 40, Section::Gpu).contains("No GPU detected"));
}

#[test]
fn tasto_d_cambia_dispositivo_e_sweep() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    let s = pair("amd");
    let mut app = App::new((120, 40), "h".into());
    app.section = Section::Gpu;
    app.update(&s);
    app.on_key(KeyEvent {
        code: KeyCode::Char('d'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });
    assert_eq!(app.gpu_sel.as_deref(), Some("0000:03:00.0"));
    for name in ["amd_b", "susp_a", "nvidia", "none", "i915_b", "xe_b"] {
        let sys = single(name);
        for w in [1u16, 5, 20, 60, 80, 99, 100, 182] {
            for h in [1u16, 3, 6, 12, 24, 30, 52] {
                for sec in [Section::Gpu, Section::Panoramica] {
                    let mut a = App::new((w, h), "h".into());
                    a.section = sec;
                    a.update(&sys);
                    let _ = to_ansi(&render_app(&sys, &mut a));
                }
            }
        }
    }
}

// ---------------------------------------------------------------- nvidia-smi

/// Risposte standard: query completa, pmon misto.
fn smi_ok() -> Arc<FakeRun> {
    FakeRun::new(|argv| {
        if argv.iter().any(|a| a.starts_with("--query-gpu")) {
            out(&nv_fixture("query_full_3090.txt"))
        } else if argv.contains(&"pmon") {
            out(&nv_fixture("pmon_mixed.txt"))
        } else {
            out("")
        }
    })
}

#[test]
fn nvidia_attiva_con_dati_e_processi() {
    let run = smi_ok();
    let mut s = system_with("nvidia_procs", run.clone());
    wait_for(&mut s, "dati", |s| s.gpu.devices[0].usage.is_some());
    let g = &s.gpu.devices[0];
    assert_eq!(g.state, GpuState::Active);
    // il nome di pci.ids resta
    assert_eq!(g.name, "NVIDIA GeForce RTX 3090");
    assert_eq!(g.usage, Some(37.0));
    assert_eq!(g.vram_total, Some(24576 * 1024 * 1024));
    assert_eq!(g.temp, Some(54.0));
    assert_eq!(g.power_w, Some(112.45));
    assert_eq!(g.fan_pct, Some(41.0));
    assert!(!g.procs_compute_only);
    // processi: solo i pid noti (1234 e 3456; 2345 non esiste), pdev dalla tabella indice
    wait_for(&mut s, "processi", |s| !s.gpu.procs.is_empty());
    let pids: Vec<u32> = s.gpu.procs.iter().map(|p| p.pid).collect();
    assert_eq!(pids, vec![1234, 3456]);
    let p = &s.gpu.procs[1];
    assert_eq!(p.pdev, "0000:01:00.0");
    assert_eq!(p.kind, WorkKind::Compute);
    assert_eq!(p.gpu_pct, 85.0);
    assert_eq!(p.mem, 4096 * 1024 * 1024);
    assert!(s.gpu.procs.iter().all(|p| p.gpu_pct <= 100.0));
    // query e pmon sono passati da nvidia-smi
    assert!(run.calls().iter().all(|c| c[0] == "nvidia-smi"));
}

#[test]
fn nvidia_sospesa_non_lancia_nvidia_smi() {
    let run = smi_ok();
    let mut s = system_with("nvidia_susp", run.clone());
    for _ in 0..5 {
        s.refresh_with_dt(1.0);
        std::thread::sleep(Duration::from_millis(30));
    }
    assert_eq!(s.gpu.devices[0].state, GpuState::Suspended);
    assert_eq!(s.gpu.devices[0].usage, None);
    assert!(run.calls().is_empty(), "{:?}", run.calls());
}

#[test]
fn nvidia_timeout_e_not_responding() {
    let run = FakeRun::new(|_| Err(io::Error::new(io::ErrorKind::TimedOut, "timeout")));
    let mut s = system_with("nvidia", run);
    wait_for(&mut s, "NotResponding", |s| {
        s.gpu.devices[0].state == GpuState::NotResponding
    });
    assert_eq!(s.gpu.devices[0].usage, None);
}

#[test]
fn nvidia_guardia_addormentata_non_e_un_errore_e_non_fa_raffiche() {
    // rc 97 = la guardia ha trovato la scheda addormentata (nulla è stato lanciato)
    let run = FakeRun::new(|_| out_rc(97, "", ""));
    let mut s = system_with("nvidia", run.clone());
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_millis(1500) {
        s.refresh_with_dt(1.0);
        assert_ne!(s.gpu.devices[0].state, GpuState::NotResponding);
        std::thread::sleep(Duration::from_millis(20));
    }
    // dopo un "addormentata" si aspetta un giro intero (2 s), non 50 ms
    assert!(run.calls().len() <= 2, "{:?}", run.calls());
}

#[test]
fn nvidia_comando_assente_in_remoto_rc_127() {
    let run = FakeRun::new(|_| out_rc(127, "", "sh: nvidia-smi: not found"));
    let mut s = system_with("nvidia", run);
    wait_for(&mut s, "NoTool", |s| {
        s.gpu.devices[0].state == GpuState::NoTool
    });
}

#[test]
fn nvidia_campo_rifiutato_si_ripassa_ai_campi_base() {
    let run = FakeRun::new(|argv| {
        if let Some(q) = argv.iter().find(|a| a.starts_with("--query-gpu")) {
            if q.contains("utilization.encoder") {
                return out_rc(2, "", &nv_fixture("query_old_driver_stderr.txt"));
            }
            return out(&nv_fixture("query_base_old_driver.txt").replace(":02:00.0", ":01:00.0"));
        }
        out_rc(
            0,
            "Monitoring processes is not supported on this device.\n",
            "",
        )
    });
    let mut s = system_with("nvidia", run.clone());
    wait_for(&mut s, "dati base", |s| s.gpu.devices[0].usage.is_some());
    assert_eq!(s.gpu.devices[0].usage, Some(11.0));
    assert_eq!(s.gpu.devices[0].enc, None);
    let q: Vec<String> = run
        .calls()
        .iter()
        .filter_map(|c| c.iter().find(|a| a.starts_with("--query-gpu")).cloned())
        .collect();
    assert!(q[0].contains("utilization.encoder"), "{q:?}");
    assert!(
        q[1..].iter().all(|x| !x.contains("utilization.encoder")),
        "{q:?}"
    );
}

#[test]
fn nvidia_senza_pmon_usa_compute_apps() {
    let run = FakeRun::new(|argv| {
        if argv.iter().any(|a| a.starts_with("--query-gpu")) {
            out(&nv_fixture("query_full_3090.txt"))
        } else if argv.contains(&"pmon") {
            out(&nv_fixture("pmon_not_supported.txt"))
        } else {
            out(&nv_fixture("compute_apps.txt"))
        }
    });
    let mut s = system_with("nvidia_procs", run.clone());
    wait_for(&mut s, "compute-apps", |s| {
        s.gpu.devices[0].procs_compute_only && !s.gpu.procs.is_empty()
    });
    // 3456 sulla scheda 01:00.0; 7788 (scheda sconosciuta e pid assente) scartato
    assert_eq!(s.gpu.procs.len(), 1);
    assert_eq!(s.gpu.procs[0].pid, 3456);
    assert_eq!(s.gpu.procs[0].mem, 4096 * 1024 * 1024);
    assert!(
        run.calls()
            .iter()
            .any(|c| c.iter().any(|a| a.starts_with("--query-compute-apps")))
    );
}

/// Copia ricorsiva di una fixture in una cartella sotto target/ (modificabile dal test).
fn copy_fixture(name: &str, to: &str) -> PathBuf {
    fn cp(a: &std::path::Path, b: &std::path::Path) {
        std::fs::create_dir_all(b).unwrap();
        for e in std::fs::read_dir(a).unwrap() {
            let e = e.unwrap();
            let t = b.join(e.file_name());
            if e.file_type().unwrap().is_dir() {
                cp(&e.path(), &t);
            } else {
                std::fs::copy(e.path(), &t).unwrap();
            }
        }
    }
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dst = base
        .join("target/test-tmp")
        .join(format!("{}-{to}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dst);
    cp(&base.join("tests/fixtures/gpu").join(name), &dst);
    dst
}

#[test]
fn nvidia_due_schede_una_sospesa_interroga_solo_la_sveglia() {
    let run = FakeRun::new(|argv| {
        if argv.iter().any(|a| a.starts_with("--query-gpu")) {
            out(&nv_fixture("query_two_gpus.txt"))
        } else if argv.contains(&"pmon") {
            out(&nv_fixture("pmon_not_supported.txt"))
        } else {
            out(&nv_fixture("compute_apps.txt"))
        }
    });
    let mut s = system_with("nvidia_two", run.clone());
    wait_for(&mut s, "dati", |s| {
        s.gpu.devices.iter().any(|g| g.usage.is_some())
    });
    wait_for(&mut s, "compute-apps", |_| {
        run.calls()
            .iter()
            .any(|c| c.iter().any(|a| a.starts_with("--query-compute-apps")))
    });
    let calls = run.calls();
    assert!(calls.len() >= 3, "{calls:?}");
    for c in &calls {
        let i = c.iter().position(|a| a == "-i").expect("manca -i");
        assert_eq!(c[i + 1], "00000000:01:00.0", "{c:?}");
        assert!(!c.iter().any(|a| a.contains("08:00.0")), "{c:?}");
    }
    let sus = s
        .gpu
        .devices
        .iter()
        .find(|g| g.pdev == "0000:08:00.0")
        .unwrap();
    assert_eq!(sus.state, GpuState::Suspended);
    assert_eq!(sus.usage, None);
}

#[test]
fn nvidia_scheda_che_si_sospende_non_viene_piu_interrogata() {
    let dir = copy_fixture("nvidia", "sospende");
    let run = smi_ok();
    let mut r = Roots::with_dirs(dir.join("proc"), dir.join("sys"), dir.join("etc"));
    r.run = run.clone();
    let mut s = System::new(r);
    s.clock_ticks = 100;
    s.page_size = 4096;
    wait_for(&mut s, "dati", |s| s.gpu.devices[0].usage.is_some());
    std::fs::write(
        dir.join("sys/class/drm/card0/device/power/runtime_status"),
        "suspended\n",
    )
    .unwrap();
    // lascia finire un eventuale giro già in corso, poi nessuna nuova chiamata anche senza refresh
    std::thread::sleep(Duration::from_millis(300));
    let n = run.calls().len();
    std::thread::sleep(Duration::from_millis(2600));
    assert_eq!(run.calls().len(), n, "{:?}", run.calls());
    s.refresh_with_dt(1.0);
    assert_eq!(s.gpu.devices[0].state, GpuState::Suspended);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn nvidia_suspending_conta_come_sospesa() {
    let dir = copy_fixture("nvidia", "suspending");
    std::fs::write(
        dir.join("sys/class/drm/card0/device/power/runtime_status"),
        "suspending\n",
    )
    .unwrap();
    let run = smi_ok();
    let mut r = Roots::with_dirs(dir.join("proc"), dir.join("sys"), dir.join("etc"));
    r.run = run.clone();
    let mut s = System::new(r);
    s.refresh_with_dt(0.0);
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(s.gpu.devices[0].state, GpuState::Suspended);
    assert!(run.calls().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn nvidia_timeout_dei_comandi_al_massimo_3_secondi() {
    let run = smi_ok();
    let mut s = system_with("nvidia_procs", run.clone());
    wait_for(&mut s, "processi", |s| !s.gpu.procs.is_empty());
    let t = run.timeouts.lock().unwrap().clone();
    assert!(t.len() >= 2);
    assert!(t.iter().all(|d| *d <= Duration::from_secs(3)), "{t:?}");
}

#[test]
fn nvidia_uscita_oltre_il_limite_non_si_legge() {
    // righe valide solo dopo MAX_OUTPUT byte di riempitivo
    let run = FakeRun::new(|argv| {
        if argv.iter().any(|a| a.starts_with("--query-gpu")) {
            let pad = "#".repeat(ngtop::collect::nvidia::MAX_OUTPUT + 16);
            out(&format!("{pad}\n{}", nv_fixture("query_full_3090.txt")))
        } else {
            out("")
        }
    });
    let mut s = system_with("nvidia", run);
    wait_for(&mut s, "NotResponding", |s| {
        s.gpu.devices[0].state == GpuState::NotResponding
    });
    assert_eq!(s.gpu.devices[0].usage, None);
}
