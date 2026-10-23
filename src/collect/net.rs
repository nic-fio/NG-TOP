// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Rete: contatori da /proc/net/dev, rotta di default da /proc/net/route, IPv4 da fib_trie.

use std::path::Path;

use super::Roots;
use crate::source::Source;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Iface {
    pub name: String,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    /// Byte al secondo.
    pub rx_rate: f64,
    pub tx_rate: f64,
    /// Picco di sessione delle velocità.
    pub rx_peak: f64,
    pub tx_peak: f64,
    /// Da operstate ("unknown" conta come attiva, es. tun).
    pub up: Option<bool>,
    pub ipv4: Option<String>,
    /// IPv6 "indirizzo/prefisso": globale se c'è, altrimenti link-local.
    pub ipv6: Option<String>,
    /// Velocità di collegamento in Mbit/s e duplex (sys/class/net/X/speed, duplex).
    pub speed_mbps: Option<u32>,
    pub duplex: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct NetInfo {
    pub ifaces: Vec<Iface>,
    /// Interfaccia della rotta di default.
    pub default: Option<String>,
    /// Gateway della rotta di default (IPv4).
    pub gateway: Option<String>,
    /// Server DNS da resolv.conf.
    pub dns: Vec<String>,
    /// Copia di rotte e indirizzi locali, riletti solo ogni 5 giri.
    routes: Vec<Route>,
    ips: Vec<[u8; 4]>,
    ips6: Vec<(String, String, bool)>,
    rounds: u64,
}

impl NetInfo {
    pub fn new(ifaces: Vec<Iface>, default: Option<String>) -> Self {
        NetInfo {
            ifaces,
            default,
            ..NetInfo::default()
        }
    }

    /// Interfaccia da mostrare: la preferita se esiste, poi la rotta di default, poi la prima non `lo`.
    pub fn pick(&self, preferred: Option<&str>) -> Option<&Iface> {
        let by_name = |n: &str| self.ifaces.iter().find(|i| i.name == n);
        preferred
            .and_then(by_name)
            .or_else(|| self.default.as_deref().and_then(by_name))
            .or_else(|| self.ifaces.iter().find(|i| i.name != "lo"))
            .or_else(|| self.ifaces.first())
    }

    /// Nome dell'interfaccia successiva (o precedente) a `current`, saltando `lo` se ce ne sono altre.
    pub fn cycle(&self, current: Option<&str>, step: isize) -> Option<String> {
        let mut names: Vec<&str> = self
            .ifaces
            .iter()
            .map(|i| i.name.as_str())
            .filter(|n| *n != "lo")
            .collect();
        if names.is_empty() {
            names = self.ifaces.iter().map(|i| i.name.as_str()).collect();
        }
        if names.is_empty() {
            return None;
        }
        let cur = current
            .and_then(|c| names.iter().position(|n| *n == c))
            .unwrap_or(0) as isize;
        let n = names.len() as isize;
        Some(names[(cur + step).rem_euclid(n) as usize].to_string())
    }
}

/// (nome, byte ricevuti, byte inviati) per ogni riga di /proc/net/dev.
pub fn parse_dev(text: &str) -> Vec<(String, u64, u64)> {
    text.lines()
        .filter_map(|l| {
            let (name, rest) = l.split_once(':')?;
            let f: Vec<u64> = rest
                .split_whitespace()
                .map(|x| x.parse().unwrap_or(0))
                .collect();
            Some((name.trim().to_string(), *f.first()?, *f.get(8)?))
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub iface: String,
    /// Valori grezzi come in /proc/net/route (ordine di rete letto come little-endian).
    pub dest: u32,
    pub mask: u32,
    pub metric: u32,
    /// Gateway, stesso formato grezzo di `dest`.
    pub gateway: u32,
}

pub fn parse_route(text: &str) -> Vec<Route> {
    text.lines()
        .skip(1)
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            Some(Route {
                iface: f.first()?.to_string(),
                dest: u32::from_str_radix(f.get(1)?, 16).ok()?,
                mask: u32::from_str_radix(f.get(7)?, 16).ok()?,
                metric: f.get(6)?.parse().ok()?,
                gateway: u32::from_str_radix(f.get(2)?, 16).ok()?,
            })
        })
        .collect()
}

/// Interfaccia della rotta di default con metrica minore.
pub fn default_iface(routes: &[Route]) -> Option<String> {
    routes
        .iter()
        .filter(|r| r.dest == 0 && r.mask == 0)
        .min_by_key(|r| r.metric)
        .map(|r| r.iface.clone())
}

/// Indirizzi locali IPv4 dalla tabella LOCAL di fib_trie.
pub fn parse_local_ips(text: &str) -> Vec<[u8; 4]> {
    let mut out = Vec::new();
    let mut last: Option<[u8; 4]> = None;
    for l in text.lines() {
        let t = l.trim();
        if let Some(ip) = t.strip_prefix("|-- ").or_else(|| t.strip_prefix("+-- ")) {
            last = parse_ip(ip.split_whitespace().next().unwrap_or(""));
        } else if t.starts_with("/32 host LOCAL")
            && let Some(ip) = last
            && ip[0] != 127
            && !out.contains(&ip)
        {
            out.push(ip);
        }
    }
    out
}

fn parse_ip(s: &str) -> Option<[u8; 4]> {
    let mut it = s.split('.');
    let mut ip = [0u8; 4];
    for b in &mut ip {
        *b = it.next()?.parse().ok()?;
    }
    it.next().is_none().then_some(ip)
}

/// IPv4 dell'interfaccia: l'indirizzo locale che sta nella rete di una sua rotta.
pub fn iface_ip(routes: &[Route], ips: &[[u8; 4]], iface: &str) -> Option<String> {
    routes
        .iter()
        .filter(|r| r.iface == iface && r.dest != 0)
        .find_map(|r| {
            ips.iter()
                .find(|ip| (u32::from_le_bytes(**ip) & r.mask) == r.dest)
        })
        .map(|ip| format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3]))
}

/// Gateway IPv4 della rotta di default con metrica minore.
pub fn default_gateway(routes: &[Route]) -> Option<String> {
    routes
        .iter()
        .filter(|r| r.dest == 0 && r.mask == 0 && r.gateway != 0)
        .min_by_key(|r| r.metric)
        .map(|r| std::net::Ipv4Addr::from(r.gateway.to_le_bytes()).to_string())
}

/// Server DNS (righe `nameserver`) di resolv.conf.
pub fn parse_resolv(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            (it.next()? == "nameserver").then(|| it.next().map(str::to_string))?
        })
        .collect()
}

/// (interfaccia, "indirizzo/prefisso", globale?) da /proc/net/if_inet6.
pub fn parse_if_inet6(text: &str) -> Vec<(String, String, bool)> {
    text.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            let addr = std::net::Ipv6Addr::from(u128::from_str_radix(f.first()?, 16).ok()?);
            let prefix = u32::from_str_radix(f.get(2)?, 16).ok()?;
            let scope = u32::from_str_radix(f.get(3)?, 16).ok()?;
            Some((
                f.get(5)?.to_string(),
                format!("{addr}/{prefix}"),
                scope == 0,
            ))
        })
        .collect()
}

fn read_link_info(src: &dyn Source, sys: &Path, iface: &str) -> (Option<u32>, Option<String>) {
    let base = sys.join("class/net").join(iface);
    let speed = src
        .read_text(&base.join("speed"))
        .and_then(|t| t.trim().parse::<i64>().ok())
        .filter(|v| *v > 0)
        .map(|v| v as u32);
    let duplex = src
        .read_text(&base.join("duplex"))
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty() && t != "unknown");
    (speed, duplex)
}

fn read_up(src: &dyn Source, sys: &Path, iface: &str) -> Option<bool> {
    let s = src.read_text(&sys.join("class/net").join(iface).join("operstate"))?;
    Some(matches!(s.trim(), "up" | "unknown"))
}

/// Aggiorna `net` con un nuovo campione; `dt` in secondi (<= 0 → velocità a zero).
pub fn refresh(net: &mut NetInfo, roots: &Roots, dt: f64) {
    let dev = roots
        .src
        .read_text(&roots.proc.join("net/dev"))
        .map(|t| parse_dev(&t))
        .unwrap_or_default();
    // Rotte e indirizzi cambiano di rado: riletti ogni 5 giri.
    if net.rounds.is_multiple_of(5) {
        net.routes = roots
            .src
            .read_text(&roots.proc.join("net/route"))
            .map(|t| parse_route(&t))
            .unwrap_or_default();
        net.ips = roots
            .src
            .read_text(&roots.proc.join("net/fib_trie"))
            .map(|t| parse_local_ips(&t))
            .unwrap_or_default();
        net.default = default_iface(&net.routes);
        net.gateway = default_gateway(&net.routes);
        net.dns = roots
            .src
            .read_text(&roots.etc.join("resolv.conf"))
            .map(|t| parse_resolv(&t))
            .unwrap_or_default();
        net.ips6 = roots
            .src
            .read_text(&roots.proc.join("net/if_inet6"))
            .map(|t| parse_if_inet6(&t))
            .unwrap_or_default();
    }
    let slow = net.rounds.is_multiple_of(5);
    net.rounds += 1;
    let (routes, ips, ips6) = (&net.routes, &net.ips, &net.ips6);
    let old = std::mem::take(&mut net.ifaces);
    net.ifaces = dev
        .into_iter()
        .map(|(name, rx, tx)| {
            let prev = old.iter().find(|o| o.name == name);
            let rate = |now: u64, before: Option<u64>| match before {
                Some(b) if dt > 0.0 => now.saturating_sub(b) as f64 / dt,
                _ => 0.0,
            };
            let rx_rate = rate(rx, prev.map(|p| p.rx_bytes));
            let tx_rate = rate(tx, prev.map(|p| p.tx_bytes));
            // Velocità, duplex e IPv6 cambiano di rado: riletti ogni 5 giri.
            let (speed_mbps, duplex) = match prev {
                Some(p) if !slow => (p.speed_mbps, p.duplex.clone()),
                _ => read_link_info(&*roots.src, &roots.sys, &name),
            };
            let ipv6 = ips6
                .iter()
                .filter(|(i, _, _)| *i == name)
                .max_by_key(|(_, _, global)| *global)
                .map(|(_, a, _)| a.clone());
            Iface {
                speed_mbps,
                duplex,
                ipv6,
                up: read_up(&*roots.src, &roots.sys, &name),
                ipv4: iface_ip(routes, ips, &name),
                rx_peak: prev.map_or(0.0, |p| p.rx_peak).max(rx_rate),
                tx_peak: prev.map_or(0.0, |p| p.tx_peak).max(tx_rate),
                name,
                rx_bytes: rx,
                tx_bytes: tx,
                rx_rate,
                tx_rate,
            }
        })
        .collect();
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROUTE: &str = "Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\t\tMTU\tWindow\tIRTT\n\
wlan0\t00000000\t0101A8C0\t0003\t0\t0\t600\t00000000\t0\t0\t0\n\
eth0\t00000000\t0101A8C0\t0003\t0\t0\t100\t00000000\t0\t0\t0\n\
eth0\t0001A8C0\t00000000\t0001\t0\t0\t100\t00FFFFFF\t0\t0\t0\n";
    const TRIE: &str = "Main:\n  +-- 0.0.0.0/0 3 0 5\n     |-- 192.168.1.0\n        /24 link UNICAST\n     |-- 192.168.1.10\n        /32 host LOCAL\nLocal:\n  +-- 127.0.0.0/8 2 0 2\n     |-- 127.0.0.1\n        /32 host LOCAL\n     |-- 192.168.1.10\n        /32 host LOCAL\n";

    #[test]
    fn dev_route_ip() {
        let dev = "Inter-|   Receive\n face |bytes\n    lo: 100 1 0 0 0 0 0 0 100 1 0 0 0 0 0 0\n  eth0: 5000 10 0 0 0 0 0 0 2000 5 0 0 0 0 0 0\n";
        let d = parse_dev(dev);
        assert_eq!(d, [("lo".into(), 100, 100), ("eth0".into(), 5000, 2000)]);
        let r = parse_route(ROUTE);
        assert_eq!(r.len(), 3);
        assert_eq!(default_iface(&r), Some("eth0".into()));
        let ips = parse_local_ips(TRIE);
        assert_eq!(ips, [[192, 168, 1, 10]]);
        assert_eq!(iface_ip(&r, &ips, "eth0"), Some("192.168.1.10".into()));
        assert_eq!(iface_ip(&r, &ips, "wlan0"), None);
        assert!(parse_dev("").is_empty());
        assert!(parse_route("x").is_empty());
        assert_eq!(parse_ip("1.2.3"), None);
        assert_eq!(default_gateway(&r), Some("192.168.1.1".into()));
    }

    #[test]
    fn resolv_e_ipv6() {
        let t = "# commento\nsearch lan\nnameserver 192.168.1.1\nnameserver 2001:4860:4860::8888\nnameserver\n";
        assert_eq!(parse_resolv(t), ["192.168.1.1", "2001:4860:4860::8888"]);
        assert!(parse_resolv("").is_empty());
        let t6 = "fe80000000000000026b6bfffe1c2d3a 02 40 20 80 eth0\n20010db8000000000000000000000005 02 40 00 80 eth0\n00000000000000000000000000000001 01 80 10 80 lo\nrotto\n";
        let v = parse_if_inet6(t6);
        assert_eq!(v.len(), 3);
        assert_eq!(
            v[0],
            ("eth0".into(), "fe80::26b:6bff:fe1c:2d3a/64".into(), false)
        );
        assert_eq!(v[1], ("eth0".into(), "2001:db8::5/64".into(), true));
        assert_eq!(v[2].1, "::1/128");
    }

    #[test]
    fn pick_e_cycle() {
        let mk = |n: &str| Iface {
            name: n.into(),
            ..Iface::default()
        };
        let n = NetInfo::new(
            vec![mk("lo"), mk("eth0"), mk("wlan0")],
            Some("wlan0".into()),
        );
        assert_eq!(n.pick(None).unwrap().name, "wlan0");
        assert_eq!(n.pick(Some("eth0")).unwrap().name, "eth0");
        assert_eq!(n.pick(Some("zzz")).unwrap().name, "wlan0");
        assert_eq!(n.cycle(Some("eth0"), 1), Some("wlan0".into()));
        assert_eq!(n.cycle(Some("wlan0"), 1), Some("eth0".into()));
        assert_eq!(n.cycle(None, -1), Some("wlan0".into()));
        let only_lo = NetInfo::new(vec![mk("lo")], None);
        assert_eq!(only_lo.pick(None).unwrap().name, "lo");
        assert!(NetInfo::default().pick(None).is_none());
        assert_eq!(NetInfo::default().cycle(None, 1), None);
    }
}
