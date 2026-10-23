// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Memoria e CPU% di un'unità, letti direttamente dal suo cgroup (niente D-Bus per riga).

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::collect::read_text;

/// Valore `usage_usec` di un `cpu.stat`.
pub fn parse_usage_usec(text: &str) -> Option<u64> {
    text.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        (it.next() == Some("usage_usec"))
            .then(|| it.next().and_then(|v| v.parse().ok()))
            .flatten()
    })
}

/// CPU% (100 = tutta la capacità di `ncores` core) tra due letture di `usage_usec`.
pub fn cpu_percent(prev_usec: u64, cur_usec: u64, dt_secs: f64, ncores: f64) -> f64 {
    if dt_secs <= 0.0 || cur_usec < prev_usec {
        return 0.0;
    }
    ((cur_usec - prev_usec) as f64 / 1e6 / dt_secs / ncores.max(1.0) * 100.0).clamp(0.0, 100.0)
}

/// Legge i cgroup sotto `root` (di norma /sys/fs/cgroup) tenendo l'ultima lettura della CPU.
pub struct CgroupSampler {
    root: PathBuf,
    ncores: f64,
    prev: HashMap<String, (u64, f64)>,
}

impl CgroupSampler {
    pub fn new(root: PathBuf, ncores: usize) -> Self {
        CgroupSampler {
            root,
            ncores: ncores.max(1) as f64,
            prev: HashMap::new(),
        }
    }

    fn dir(&self, cgroup: &str) -> PathBuf {
        self.root.join(cgroup.trim_start_matches('/'))
    }

    fn read_u64(&self, cgroup: &str, file: &str) -> Option<u64> {
        read_text(&self.dir(cgroup).join(file))?.trim().parse().ok()
    }

    /// Memoria corrente (byte).
    pub fn mem_current(&self, cgroup: &str) -> Option<u64> {
        self.read_u64(cgroup, "memory.current")
    }

    /// Picco di memoria (byte), se il kernel lo espone.
    pub fn mem_peak(&self, cgroup: &str) -> Option<u64> {
        self.read_u64(cgroup, "memory.peak")
    }

    /// (memoria, CPU%) all'istante `now` (secondi, qualunque origine). La CPU% richiede una
    /// lettura precedente dello stesso cgroup, altrimenti è `None`.
    pub fn sample(&mut self, cgroup: &str, now: f64) -> (Option<u64>, Option<f64>) {
        let mem = self.mem_current(cgroup);
        let usage = fs::read_to_string(self.dir(cgroup).join("cpu.stat"))
            .ok()
            .and_then(|t| parse_usage_usec(&t));
        let cpu = usage.and_then(|u| {
            let old = self.prev.insert(cgroup.to_string(), (u, now));
            old.map(|(pu, pt)| cpu_percent(pu, u, now - pt, self.ncores))
        });
        (mem, cpu)
    }

    /// Numero di cgroup con una lettura precedente.
    pub fn tracked(&self) -> usize {
        self.prev.len()
    }

    /// Dimentica i cgroup per cui `keep` è falso.
    pub fn retain(&mut self, keep: impl Fn(&str) -> bool) {
        self.prev.retain(|k, _| keep(k));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fixture(n: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/cgroup")
            .join(n)
    }

    #[test]
    fn cpu_stat() {
        let t = "usage_usec 1500000\nuser_usec 1000000\nsystem_usec 500000\n";
        assert_eq!(parse_usage_usec(t), Some(1_500_000));
        assert_eq!(parse_usage_usec("niente"), None);
    }

    #[test]
    fn percentuale_da_due_letture() {
        // 0,5 s di CPU in 2 s su 4 core: 6,25%.
        assert!((cpu_percent(1_000_000, 1_500_000, 2.0, 4.0) - 6.25).abs() < 1e-9);
        assert_eq!(cpu_percent(5, 3, 2.0, 4.0), 0.0);
        assert_eq!(cpu_percent(1, 2, 0.0, 4.0), 0.0);
        assert_eq!(cpu_percent(0, 100_000_000, 1.0, 1.0), 100.0);
    }

    #[test]
    fn campionamento_dal_cgroup_con_fixture() {
        let mut a = CgroupSampler::new(fixture("a"), 4);
        let (mem, cpu) = a.sample("/system.slice/demo.service", 100.0);
        assert_eq!(mem, Some(104_857_600));
        assert_eq!(cpu, None);
        assert_eq!(a.mem_peak("/system.slice/demo.service"), Some(209_715_200));
        // Seconda lettura: usage 1.000.000 → 1.500.000 in 2 s su 4 core.
        let mut b = CgroupSampler::new(fixture("b"), 4);
        b.prev = a.prev.clone();
        let (mem, cpu) = b.sample("/system.slice/demo.service", 102.0);
        assert_eq!(mem, Some(110_000_000));
        assert!((cpu.unwrap() - 6.25).abs() < 1e-9);
        // Cgroup inesistente: niente panic.
        assert_eq!(b.sample("/non/esiste", 103.0), (None, None));
    }
}
