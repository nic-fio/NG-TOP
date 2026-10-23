// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Carico medio (/proc/loadavg) e uptime (/proc/uptime).

use std::path::Path;

use super::read_text;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LoadAvg {
    pub one: f64,
    pub five: f64,
    pub fifteen: f64,
    pub running: u64,
    pub total: u64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Uptime {
    pub up_secs: f64,
    pub idle_secs: f64,
}

pub fn parse_loadavg(text: &str) -> LoadAvg {
    let f: Vec<&str> = text.split_whitespace().collect();
    let num = |i: usize| f.get(i).and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);
    let (running, total) = f
        .get(3)
        .and_then(|x| x.split_once('/'))
        .map(|(a, b)| (a.parse().unwrap_or(0), b.parse().unwrap_or(0)))
        .unwrap_or((0, 0));
    LoadAvg {
        one: num(0),
        five: num(1),
        fifteen: num(2),
        running,
        total,
    }
}

pub fn read_loadavg(path: &Path) -> LoadAvg {
    read_text(path)
        .map(|t| parse_loadavg(&t))
        .unwrap_or_default()
}

pub fn parse_uptime(text: &str) -> Uptime {
    let mut it = text
        .split_whitespace()
        .map(|x| x.parse::<f64>().unwrap_or(0.0));
    Uptime {
        up_secs: it.next().unwrap_or(0.0),
        idle_secs: it.next().unwrap_or(0.0),
    }
}

pub fn read_uptime(path: &Path) -> Uptime {
    read_text(path)
        .map(|t| parse_uptime(&t))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loadavg_e_uptime() {
        let l = parse_loadavg("0.52 0.58 0.59 2/345 6789\n");
        assert_eq!((l.one, l.five, l.fifteen), (0.52, 0.58, 0.59));
        assert_eq!((l.running, l.total), (2, 345));
        assert_eq!(parse_loadavg(""), LoadAvg::default());
        let u = parse_uptime("123.45 678.9\n");
        assert_eq!((u.up_secs, u.idle_secs), (123.45, 678.9));
        assert_eq!(parse_uptime("x"), Uptime::default());
    }
}
