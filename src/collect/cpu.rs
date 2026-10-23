// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! CPU: tempi da /proc/stat, modello e topologia da /proc/cpuinfo, frequenze da /sys.

use std::collections::HashSet;
use std::path::Path;

use super::read_text;

/// Tempi cumulativi (tick) di una CPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CpuTimes {
    pub total: u64,
    pub idle: u64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CoreStats {
    pub id: usize,
    /// Uso in % (0–100).
    pub usage: f64,
    pub freq_mhz: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct CpuStats {
    pub model: String,
    pub logical: usize,
    pub physical: Option<usize>,
    pub total_usage: f64,
    pub cores: Vec<CoreStats>,
}

/// Interpreta una riga "cpu..." di /proc/stat dopo l'etichetta.
fn parse_times(rest: &str) -> Option<CpuTimes> {
    let v: Vec<u64> = rest
        .split_whitespace()
        .map(|x| x.parse::<u64>().unwrap_or(0))
        .collect();
    if v.len() < 4 {
        return None;
    }
    // user nice system idle iowait irq softirq steal (guest è già incluso in user)
    let total: u64 = v.iter().take(8).sum();
    let idle = v[3] + v.get(4).copied().unwrap_or(0);
    Some(CpuTimes { total, idle })
}

/// Restituisce (totale, [(indice core, tempi)]).
pub fn parse_stat(text: &str) -> (Option<CpuTimes>, Vec<(usize, CpuTimes)>) {
    let mut total = None;
    let mut cores = Vec::new();
    for line in text.lines() {
        let mut it = line.splitn(2, char::is_whitespace);
        let (Some(label), Some(rest)) = (it.next(), it.next()) else {
            continue;
        };
        if label == "cpu" {
            total = parse_times(rest);
        } else if let Some(n) = label
            .strip_prefix("cpu")
            .and_then(|n| n.parse::<usize>().ok())
            && let Some(t) = parse_times(rest)
        {
            cores.push((n, t));
        }
    }
    (total, cores)
}

pub fn read_stat(path: &Path) -> (Option<CpuTimes>, Vec<(usize, CpuTimes)>) {
    read_text(path)
        .map(|t| parse_stat(&t))
        .unwrap_or((None, Vec::new()))
}

/// Uso % tra due campioni (0 se non è passato tempo).
pub fn usage_between(prev: &CpuTimes, now: &CpuTimes) -> f64 {
    let dt = now.total.saturating_sub(prev.total);
    if dt == 0 {
        return 0.0;
    }
    let didle = now.idle.saturating_sub(prev.idle).min(dt);
    (dt - didle) as f64 * 100.0 / dt as f64
}

/// Primo "model name" di cpuinfo (vuoto se assente).
pub fn parse_model(text: &str) -> String {
    for line in text.lines() {
        if let Some((k, v)) = line.split_once(':')
            && k.trim() == "model name"
        {
            return v.trim().to_string();
        }
    }
    String::new()
}

pub fn read_model(path: &Path) -> String {
    read_text(path).map(|t| parse_model(&t)).unwrap_or_default()
}

/// (core logici, core fisici se ricavabili) da cpuinfo.
pub fn parse_topology(text: &str) -> (usize, Option<usize>) {
    let mut logical = 0;
    let mut pairs: HashSet<(String, String)> = HashSet::new();
    let mut phys_id = String::new();
    let mut core_id: Option<String> = None;
    let mut in_block = false;
    let mut flush = |phys: &mut String, core: &mut Option<String>| {
        if let Some(c) = core.take() {
            pairs.insert((std::mem::take(phys), c));
        }
        phys.clear();
    };
    for line in text.lines().chain(std::iter::once("")) {
        if line.trim().is_empty() {
            if in_block {
                flush(&mut phys_id, &mut core_id);
            }
            in_block = false;
            continue;
        }
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let (k, v) = (k.trim(), v.trim());
        if k == "processor" {
            logical += 1;
            in_block = true;
        } else if k == "physical id" {
            phys_id = v.to_string();
        } else if k == "core id" {
            core_id = Some(v.to_string());
        }
    }
    let physical = if pairs.is_empty() {
        None
    } else {
        Some(pairs.len())
    };
    (logical, physical)
}

pub fn read_topology(path: &Path) -> (usize, Option<usize>) {
    read_text(path)
        .map(|t| parse_topology(&t))
        .unwrap_or((0, None))
}

/// Frequenza attuale del core in MHz (scaling_cur_freq è in kHz).
pub fn read_freq_mhz(sys: &Path, core: usize) -> Option<u64> {
    let p = sys.join(format!(
        "devices/system/cpu/cpu{core}/cpufreq/scaling_cur_freq"
    ));
    let khz: u64 = read_text(&p)?.trim().parse().ok()?;
    Some(khz / 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat_parse_e_uso() {
        let a = "cpu  100 0 50 800 50 0 0 0 0 0\ncpu0 50 0 25 400 25 0 0 0 0 0\nintr 1\n";
        let b = "cpu  150 0 50 850 50 0 0 0 0 0\ncpu0 100 0 25 400 25 0 0 0 0 0\n";
        let (ta, ca) = parse_stat(a);
        let (tb, cb) = parse_stat(b);
        assert_eq!(ca.len(), 1);
        assert_eq!(ta.unwrap().total, 1000);
        assert_eq!(ta.unwrap().idle, 850);
        assert!((usage_between(&ta.unwrap(), &tb.unwrap()) - 50.0).abs() < 1e-9);
        assert!((usage_between(&ca[0].1, &cb[0].1) - 100.0).abs() < 1e-9);
    }

    #[test]
    fn stat_malformato() {
        let (t, c) = parse_stat("cpu\ncpuX 1 2 3 4\ncpu1 a b c d\n\n");
        assert!(t.is_none());
        assert_eq!(c.len(), 1);
        assert_eq!(
            usage_between(&CpuTimes::default(), &CpuTimes::default()),
            0.0
        );
    }

    #[test]
    fn cpuinfo() {
        let t = "processor\t: 0\nmodel name\t: Foo CPU\nphysical id\t: 0\ncore id\t\t: 0\n\nprocessor\t: 1\nmodel name\t: Foo CPU\nphysical id\t: 0\ncore id\t\t: 0\n\nprocessor\t: 2\nphysical id\t: 0\ncore id\t\t: 1\n";
        assert_eq!(parse_model(t), "Foo CPU");
        assert_eq!(parse_topology(t), (3, Some(2)));
        assert_eq!(parse_topology("processor : 0\n"), (1, None));
        assert_eq!(parse_model(""), "");
    }
}
