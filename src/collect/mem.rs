// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Memoria e swap da /proc/meminfo (valori in byte).

use std::collections::HashMap;
use std::path::Path;

use super::read_text;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MemInfo {
    pub total: u64,
    pub available: u64,
    /// total − available.
    pub used: u64,
    /// Cached + SReclaimable.
    pub cache: u64,
    pub buffers: u64,
    pub free: u64,
    pub swap_total: u64,
    pub swap_free: u64,
    pub swap_used: u64,
    /// Dimensione compressa di zswap, se presente.
    pub zswap: Option<u64>,
    /// Dati non compressi che zswap ha in pool (Zswapped), se presente.
    pub zswapped: Option<u64>,
    /// Pagine sporche in attesa di scrittura.
    pub dirty: u64,
}

pub fn parse_meminfo(text: &str) -> MemInfo {
    let mut m: HashMap<&str, u64> = HashMap::new();
    for line in text.lines() {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let mut parts = v.split_whitespace();
        let Some(n) = parts.next().and_then(|n| n.parse::<u64>().ok()) else {
            continue;
        };
        let mult = if parts.next() == Some("kB") { 1024 } else { 1 };
        m.insert(k.trim(), n.saturating_mul(mult));
    }
    let g = |k: &str| m.get(k).copied().unwrap_or(0);
    let total = g("MemTotal");
    let free = g("MemFree");
    let buffers = g("Buffers");
    let cache = g("Cached") + g("SReclaimable");
    let available = m
        .get("MemAvailable")
        .copied()
        .unwrap_or_else(|| (free + buffers + g("Cached")).min(total));
    let swap_total = g("SwapTotal");
    let swap_free = g("SwapFree");
    MemInfo {
        total,
        available,
        used: total.saturating_sub(available),
        cache,
        buffers,
        free,
        swap_total,
        swap_free,
        swap_used: swap_total.saturating_sub(swap_free),
        zswap: m.get("Zswap").copied(),
        zswapped: m.get("Zswapped").copied(),
        dirty: g("Dirty"),
    }
}

pub fn read_meminfo(path: &Path) -> MemInfo {
    read_text(path)
        .map(|t| parse_meminfo(&t))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meminfo() {
        let t = "MemTotal: 1000 kB\nMemFree: 100 kB\nMemAvailable: 400 kB\nBuffers: 10 kB\nCached: 200 kB\nSReclaimable: 50 kB\nSwapTotal: 300 kB\nSwapFree: 100 kB\nZswap: 5 kB\n";
        let m = parse_meminfo(t);
        assert_eq!(m.total, 1000 * 1024);
        assert_eq!(m.used, 600 * 1024);
        assert_eq!(m.cache, 250 * 1024);
        assert_eq!(m.swap_used, 200 * 1024);
        assert_eq!(m.zswap, Some(5 * 1024));
    }

    #[test]
    fn meminfo_vuoto_o_senza_available() {
        assert_eq!(parse_meminfo(""), MemInfo::default());
        let m = parse_meminfo("MemTotal: 100 kB\nMemFree: 10 kB\nBuffers: 5 kB\nCached: 20 kB\n");
        assert_eq!(m.available, 35 * 1024);
        assert_eq!(m.zswap, None);
    }
}
