// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Raccoglitore GPU e viste GPU su fixture realistiche (amdgpu, i915, xe, sospesa, NVIDIA).

use std::path::PathBuf;

use ngtop::collect::gpu::{GpuState, WorkKind};
use ngtop::collect::{Roots, System};
use ngtop::ui::app::{App, Section};
use ngtop::ui::snapshot::{render_app, to_ansi, to_text};

fn roots(name: &str) -> Roots {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/gpu")
        .join(name);
    Roots::with_dirs(base.join("proc"), base.join("sys"), base.join("etc"))
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
fn nvidia_solo_rilevata() {
    let s = single("nvidia");
    let g = &s.gpu.devices[0];
    assert_eq!(g.state, GpuState::Unsupported);
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
    let n = single("nvidia");
    assert!(text(&n, 120, 40, Section::Gpu).contains("support coming soon"));
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
