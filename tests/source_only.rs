// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! I raccoglitori leggono SOLO attraverso `Source`: prova con una sorgente finta in memoria
//! e controllo sui sorgenti che nessuna lettura diretta sia rimasta fuori da `src/source/`.

use std::collections::HashMap;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ngtop::collect::{Roots, System};
use ngtop::source::{FsStat, LocalRunner, Source};

/// Sorgente finta: percorso → contenuto; le cartelle derivano dai percorsi.
struct MemSource(HashMap<PathBuf, Vec<u8>>);

impl MemSource {
    fn new(files: &[(&str, &str)]) -> Self {
        MemSource(
            files
                .iter()
                .map(|(p, c)| (PathBuf::from(p), c.as_bytes().to_vec()))
                .collect(),
        )
    }
}

impl Source for MemSource {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.0
            .get(path)
            .cloned()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    }
    fn read_dir(&self, path: &Path) -> io::Result<Vec<OsString>> {
        let mut names: Vec<OsString> = self
            .0
            .keys()
            .filter_map(|k| k.strip_prefix(path).ok())
            .filter_map(|r| r.components().next())
            .map(|c| c.as_os_str().to_os_string())
            .collect();
        names.sort();
        names.dedup();
        if names.is_empty() {
            Err(io::Error::from(io::ErrorKind::NotFound))
        } else {
            Ok(names)
        }
    }
    fn read_link(&self, _: &Path) -> io::Result<PathBuf> {
        Err(io::Error::from(io::ErrorKind::NotFound))
    }
    fn owner_uid(&self, _: &Path) -> io::Result<u32> {
        Err(io::Error::from(io::ErrorKind::NotFound))
    }
    fn statvfs(&self, _: &Path) -> io::Result<FsStat> {
        Err(io::Error::from(io::ErrorKind::NotFound))
    }
}

#[test]
fn cpu_e_memoria_da_sorgente_in_memoria() {
    let src = MemSource::new(&[
        (
            "/proc/stat",
            "cpu  100 0 100 800 0 0 0 0 0 0\ncpu0 100 0 100 800 0 0 0 0 0 0\n",
        ),
        (
            "/proc/cpuinfo",
            "processor\t: 0\nmodel name\t: Finta CPU 9000\n",
        ),
        (
            "/proc/meminfo",
            "MemTotal:       2048000 kB\nMemFree:        1024000 kB\nMemAvailable:   1536000 kB\n",
        ),
        ("/proc/loadavg", "0.50 0.25 0.10 1/100 42\n"),
        ("/proc/uptime", "1234.5 5000.0\n"),
        (
            "/proc/7/stat",
            "7 (finto) S 1 7 7 0 -1 0 0 0 0 0 5 5 0 0 20 0 1 0 100 4096 10 0 0 0 0 0 0 0 0 0 0 0 0 0 17 0 0 0\n",
        ),
        ("/proc/7/status", "Uid:\t5\t5\t5\t5\n"),
        ("/proc/7/cmdline", "finto\0--x\0"),
        ("/etc/passwd", "cinque:x:5:5::/h:/bin/sh\n"),
    ]);
    let mut roots = Roots::with_dirs("/proc".into(), "/sys".into(), "/etc".into());
    roots.src = Arc::new(src);
    roots.run = Arc::new(LocalRunner);
    let mut s = System::new(roots);
    s.refresh_with_dt(1.0);
    assert_eq!(s.cpu.model, "Finta CPU 9000");
    assert_eq!(s.cpu.cores.len(), 1);
    assert_eq!(s.mem.total, 2_048_000 * 1024);
    assert!((s.load.one - 0.5).abs() < 1e-9);
    assert!((s.uptime.up_secs - 1234.5).abs() < 1e-9);
    assert_eq!(s.processes.len(), 1);
    assert_eq!(s.processes[0].name, "finto");
    assert_eq!(s.processes[0].user, "cinque");
    assert_eq!(s.processes[0].cmdline, "finto --x");
    assert!(s.remote_status().is_none());
}

/// Nessuna lettura diretta del filesystem nei raccoglitori e nel fuso orario.
#[test]
fn nessuna_lettura_diretta_fuori_da_source() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = vec![root.join("ui/tz.rs")];
    for e in std::fs::read_dir(root.join("collect")).unwrap().flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x == "rs") {
            files.push(p);
        }
    }
    assert!(files.len() > 8);
    // `std::fs::`, `fs::read…`, `File::open`, o chiamate libere (non metodi) delle funzioni di sistema.
    let direct = |l: &str| {
        if l.contains("std::fs::") || l.contains("fs::read") || l.contains("File::open") {
            return true;
        }
        for name in ["read_dir(", "read_link(", "metadata(", "statvfs("] {
            let mut from = 0;
            while let Some(i) = l[from..].find(name) {
                let at = from + i;
                let prev = l[..at].chars().next_back();
                if !prev.is_some_and(|c| c == '.' || c.is_alphanumeric() || c == '_') {
                    return true;
                }
                from = at + name.len();
            }
        }
        false
    };
    let mut bad = Vec::new();
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap();
        // I test interni (in fondo al file) preparano le cartelle finte con std::fs: si saltano.
        let prod = text.split("#[cfg(test)]").next().unwrap();
        for (n, l) in prod.lines().enumerate() {
            let code = l.split("//").next().unwrap_or("");
            if direct(code) {
                bad.push(format!("{}:{}: {}", f.display(), n + 1, l.trim()));
            }
        }
    }
    assert!(bad.is_empty(), "letture dirette:\n{}", bad.join("\n"));
}
