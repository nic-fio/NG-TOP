// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Avvio del binario senza terminale: messaggio chiaro e codice 2.

use std::process::{Command, Stdio};

#[test]
fn senza_terminale_non_apre_la_tui() {
    let out = Command::new(env!("CARGO_BIN_EXE_ngtop"))
        .stdin(Stdio::null())
        .output()
        .expect("avvio");
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("an interactive terminal is required"), "{err}");
    assert!(err.contains("ngtop --dump"), "{err}");
}

#[test]
fn aiuto_e_versione_funzionano_senza_terminale() {
    for (arg, needle) in [
        ("--help", "Usage: ngtop"),
        ("-V", "ngtop 1.6"),
        ("-V", "Licence: PolyForm Noncommercial 1.0.0"),
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_ngtop"))
            .arg(arg)
            .stdin(Stdio::null())
            .output()
            .expect("avvio");
        assert_eq!(out.status.code(), Some(0), "{arg}");
        assert!(
            String::from_utf8_lossy(&out.stdout).contains(needle),
            "{arg}"
        );
    }
}

#[test]
fn snapshot_con_finestre_aperte() {
    let roots = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/snap_a");
    for (ov, section, needle) in [
        ("help", "1", "Everywhere:"),
        ("signal", "3", "Send signal"),
        ("filter", "1", "filter: fire"),
        ("confirm", "5", "Confirm"),
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_ngtop"))
            .args([
                "--snapshot",
                "120x40",
                "--section",
                section,
                "--overlay",
                ov,
            ])
            .args(["--roots", roots])
            .output()
            .expect("avvio");
        assert_eq!(out.status.code(), Some(0), "{ov}");
        let t = String::from_utf8_lossy(&out.stdout);
        assert!(t.contains(needle), "{ov}: {t}");
    }
}

#[test]
fn snapshot_seleziona_il_pid_richiesto() {
    let roots = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/snap_a");
    let out = Command::new(env!("CARGO_BIN_EXE_ngtop"))
        .args(["--snapshot", "120x40", "--section", "3"])
        .args([
            "--select-pid",
            "100",
            "--overlay",
            "signal",
            "--roots",
            roots,
        ])
        .output()
        .expect("avvio");
    let t = String::from_utf8_lossy(&out.stdout);
    assert!(t.contains("Process: 100 Web Content"), "{t}");
}

#[test]
fn snapshot_con_roots_riempie_il_grafico_cpu() {
    let roots = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/snap_a");
    let out = Command::new(env!("CARGO_BIN_EXE_ngtop"))
        .args(["--snapshot", "182x52", "--section", "1", "--roots", roots])
        .output()
        .expect("avvio");
    let t = String::from_utf8_lossy(&out.stdout);
    // riga alta del grafico: almeno un braille pieno su molte celle
    let full = t.chars().filter(|&c| c == '\u{28FF}').count();
    assert!(full > 100, "grafico CPU quasi vuoto: {full}");
}
