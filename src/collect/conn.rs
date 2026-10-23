// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Connessioni di rete: /proc/net/{tcp,tcp6,udp,udp6} associate ai processi tramite gli
//! inode `socket:[N]` in /proc/*/fd. L'associazione è costosa: la cache inode→pid si aggiorna
//! in modo incrementale e la scansione completa parte solo per inode nuovi.

use std::collections::{HashMap, HashSet};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::path::Path;

use super::{Roots, read_text};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Proto {
    #[default]
    Tcp,
    Tcp6,
    Udp,
    Udp6,
}

impl Proto {
    pub fn label(self) -> &'static str {
        match self {
            Proto::Tcp => "TCP",
            Proto::Tcp6 => "TCP6",
            Proto::Udp => "UDP",
            Proto::Udp6 => "UDP6",
        }
    }

    pub fn is_tcp(self) -> bool {
        matches!(self, Proto::Tcp | Proto::Tcp6)
    }

    fn is_v6(self) -> bool {
        matches!(self, Proto::Tcp6 | Proto::Udp6)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Conn {
    pub proto: Proto,
    pub local: String,
    pub remote: String,
    /// "ESTAB", "LISTEN"… ("—" per UDP non connesso).
    pub state: &'static str,
    pub rx_queue: u64,
    pub tx_queue: u64,
    pub inode: u64,
    /// `None` se il processo non è determinabile (permesso negato, socket del kernel).
    pub pid: Option<u32>,
}

impl Conn {
    pub fn is_listen(&self) -> bool {
        self.state == "LISTEN"
    }
}

/// Indirizzo IPv4 da 8 cifre esadecimali (intero in ordine dei byte della macchina).
pub fn parse_addr4(hex: &str) -> Option<Ipv4Addr> {
    let n = u32::from_str_radix(hex, 16).ok()?;
    (hex.len() == 8).then(|| Ipv4Addr::from(n.to_ne_bytes()))
}

/// Indirizzo IPv6 da 32 cifre esadecimali: quattro parole da 32 bit in ordine della macchina.
pub fn parse_addr6(hex: &str) -> Option<Ipv6Addr> {
    if hex.len() != 32 || !hex.is_ascii() {
        return None;
    }
    let mut bytes = [0u8; 16];
    for i in 0..4 {
        let w = u32::from_str_radix(&hex[i * 8..i * 8 + 8], 16).ok()?;
        bytes[i * 4..i * 4 + 4].copy_from_slice(&w.to_ne_bytes());
    }
    Some(Ipv6Addr::from(bytes))
}

/// "indirizzo:porta" (esadecimali) in testo; IPv6 compresso e fra parentesi quadre.
/// `None` se il formato non è valido.
pub fn parse_endpoint(s: &str, v6: bool) -> Option<(String, u16, bool)> {
    let (addr, port) = s.split_once(':')?;
    let port = u16::from_str_radix(port, 16).ok()?;
    let (text, any) = if v6 {
        let a = parse_addr6(addr)?;
        (format!("[{a}]:{port}"), a.is_unspecified())
    } else {
        let a = parse_addr4(addr)?;
        (format!("{a}:{port}"), a.is_unspecified())
    };
    Some((text, port, any && port == 0))
}

/// Nome dello stato TCP dal codice esadecimale di /proc/net/tcp.
pub fn tcp_state(code: &str) -> &'static str {
    match code {
        "01" => "ESTAB",
        "02" => "SYN_SENT",
        "03" => "SYN_RECV",
        "04" => "FIN_WAIT1",
        "05" => "FIN_WAIT2",
        "06" => "TIME_WAIT",
        "07" => "CLOSE",
        "08" => "CLOSE_WAIT",
        "09" => "LAST_ACK",
        "0A" | "0a" => "LISTEN",
        "0B" | "0b" => "CLOSING",
        _ => "?",
    }
}

/// Righe di una tabella /proc/net/{tcp,udp}[6]; le righe malformate vengono saltate.
pub fn parse_table(text: &str, proto: Proto) -> Vec<Conn> {
    text.lines()
        .skip(1)
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            let (local, _, _) = parse_endpoint(f.get(1)?, proto.is_v6())?;
            let (remote, _, unset) = parse_endpoint(f.get(2)?, proto.is_v6())?;
            let state = if proto.is_tcp() {
                tcp_state(f.get(3)?)
            } else {
                // UDP: 07 = non connesso, 01 = connesso.
                match *f.get(3)? {
                    "01" => "ESTAB",
                    _ => "—",
                }
            };
            let (tx, rx) = f.get(4)?.split_once(':')?;
            Some(Conn {
                proto,
                local,
                remote: if unset { "*".to_string() } else { remote },
                state,
                rx_queue: u64::from_str_radix(rx, 16).ok()?,
                tx_queue: u64::from_str_radix(tx, 16).ok()?,
                inode: f.get(9)?.parse().ok()?,
                pid: None,
            })
        })
        .collect()
}

/// Inode da un collegamento "socket:[12345]".
pub fn socket_inode(link: &str) -> Option<u64> {
    link.strip_prefix("socket:[")?
        .strip_suffix(']')?
        .parse()
        .ok()
}

#[derive(Debug, Clone, Default)]
pub struct ConnInfo {
    pub conns: Vec<Conn>,
    /// Cache inode → pid, aggiornata in modo incrementale.
    cache: HashMap<u64, u32>,
    /// Inode che l'ultima scansione completa non ha saputo attribuire.
    unresolved: HashSet<u64>,
    /// Giri che mancano a una nuova scansione per gli inode non attribuiti.
    retry_in: u32,
    /// Processi con cartella fd illeggibile: pid → start_ticks.
    denied: HashMap<u32, u64>,
    /// Processi di cui è stata aperta la cartella fd (contatore cumulativo, per i test).
    pub visited: u64,
}

/// Ogni quanti giri si ritentano gli inode non attribuibili (processi altrui, socket del kernel).
const RETRY_EVERY: u32 = 5;

/// (start_ticks, ppid) da /proc/PID/stat.
fn read_start_ppid(proc: &Path, pid: u32) -> Option<(u64, u32)> {
    let t = read_text(&proc.join(pid.to_string()).join("stat"))?;
    let f: Vec<&str> = t[t.rfind(')')? + 1..].split_whitespace().collect();
    Some((f.get(19)?.parse().ok()?, f.get(1)?.parse().ok()?))
}

/// Cerca gli inode in `wanted` scorrendo /proc/*/fd; si ferma quando li ha trovati tutti.
/// Prima i processi già noti dalla cache; salta i thread del kernel e i processi la cui
/// cartella fd è già risultata illeggibile (stesso pid e stesso avvio).
fn scan_fds(
    info: &mut ConnInfo,
    proc: &Path,
    wanted: &mut HashSet<u64>,
    starts: Option<&HashMap<u32, u64>>,
) -> HashMap<u64, u32> {
    let mut found = HashMap::new();
    let mut order: Vec<u32> = info.cache.values().copied().collect();
    order.sort_unstable();
    order.dedup();
    let known: HashSet<u32> = order.iter().copied().collect();
    let mut complete = false;
    let rest = std::fs::read_dir(proc).into_iter().flatten().flatten();
    let rest = rest
        .filter_map(|e| e.file_name().to_str().and_then(|n| n.parse::<u32>().ok()))
        .filter(|p| !known.contains(p))
        .collect::<Vec<_>>();
    let mut seen: HashSet<u32> = HashSet::new();
    let total = order.len() + rest.len();
    for (n, pid) in order.into_iter().chain(rest).enumerate() {
        if wanted.is_empty() {
            break;
        }
        seen.insert(pid);
        complete = n + 1 == total;
        if pid == 2 {
            continue;
        }
        let start = |proc: &Path| match starts.and_then(|m| m.get(&pid)) {
            Some(s) => Some(*s),
            None => read_start_ppid(proc, pid).map(|x| x.0),
        };
        if let Some(d) = info.denied.get(&pid)
            && start(proc) == Some(*d)
        {
            continue;
        }
        info.visited += 1;
        let Ok(fds) = std::fs::read_dir(proc.join(pid.to_string()).join("fd")) else {
            // Permesso negato o processo sparito (thread del kernel compresi): ricordato.
            if let Some((st, _)) = read_start_ppid(proc, pid) {
                info.denied.insert(pid, st);
            }
            continue;
        };
        for fd in fds.flatten() {
            let Ok(target) = std::fs::read_link(fd.path()) else {
                continue;
            };
            if let Some(ino) = target.to_str().and_then(socket_inode)
                && wanted.remove(&ino)
            {
                found.insert(ino, pid);
            }
        }
    }
    if complete {
        info.denied.retain(|p, _| seen.contains(p));
    }
    found
}

/// Rilegge le tabelle e associa i processi; `info` conserva la cache tra un giro e l'altro.
pub fn refresh(info: &mut ConnInfo, roots: &Roots) {
    refresh_with(info, roots, None);
}

/// Come `refresh`; `starts` (pid → start_ticks, da `System::processes`) evita di rileggere
/// /proc/PID/stat per riconoscere i processi già risultati illeggibili.
pub fn refresh_with(info: &mut ConnInfo, roots: &Roots, starts: Option<&HashMap<u32, u64>>) {
    let mut conns = Vec::new();
    for (file, proto) in [
        ("net/tcp", Proto::Tcp),
        ("net/tcp6", Proto::Tcp6),
        ("net/udp", Proto::Udp),
        ("net/udp6", Proto::Udp6),
    ] {
        if let Some(t) = read_text(&roots.proc.join(file)) {
            conns.extend(parse_table(&t, proto));
        }
    }
    let live: HashSet<u64> = conns.iter().map(|c| c.inode).filter(|i| *i != 0).collect();
    info.cache.retain(|ino, _| live.contains(ino));
    info.unresolved.retain(|ino| live.contains(ino));
    info.retry_in = info.retry_in.saturating_sub(1);
    // Da cercare: inode mai visti; quelli già falliti solo ogni tanto.
    let retry = info.retry_in == 0;
    let mut wanted: HashSet<u64> = live
        .iter()
        .copied()
        .filter(|i| !info.cache.contains_key(i) && (retry || !info.unresolved.contains(i)))
        .collect();
    if !wanted.is_empty() {
        let found = scan_fds(info, &roots.proc, &mut wanted, starts);
        info.cache.extend(found);
        // Quel che resta in `wanted` non è attribuibile ora.
        info.unresolved.extend(wanted);
        if retry {
            info.retry_in = RETRY_EVERY;
        }
    }
    for c in &mut conns {
        c.pid = info.cache.get(&c.inode).copied();
    }
    info.conns = conns;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;

    fn le(addr: [u8; 4]) -> String {
        // Come il kernel: i byte in ordine di rete letti come intero della macchina.
        format!("{:08X}", u32::from_ne_bytes(addr))
    }

    #[test]
    fn indirizzi_v4() {
        assert_eq!(le([127, 0, 0, 1]), "0100007F");
        assert_eq!(parse_addr4("0100007F"), Some(Ipv4Addr::new(127, 0, 0, 1)));
        assert_eq!(
            parse_addr4(&le([192, 168, 1, 10])),
            Some(Ipv4Addr::new(192, 168, 1, 10))
        );
        assert_eq!(parse_addr4("zz"), None);
        assert_eq!(parse_addr4("0100"), None);
        let (t, p, any) = parse_endpoint(&format!("{}:01BB", le([1, 2, 3, 4])), false).unwrap();
        assert_eq!((t.as_str(), p, any), ("1.2.3.4:443", 443, false));
        let (t, _, any) = parse_endpoint("00000000:0000", false).unwrap();
        assert_eq!((t.as_str(), any), ("0.0.0.0:0", true));
        assert!(parse_endpoint("00000000", false).is_none());
    }

    fn v6hex(a: Ipv6Addr) -> String {
        let b = a.octets();
        (0..4)
            .map(|i| {
                let w = u32::from_ne_bytes([b[i * 4], b[i * 4 + 1], b[i * 4 + 2], b[i * 4 + 3]]);
                format!("{w:08X}")
            })
            .collect()
    }

    #[test]
    fn indirizzi_v6() {
        let loopback = v6hex(Ipv6Addr::LOCALHOST);
        assert_eq!(parse_addr6(&loopback), Some(Ipv6Addr::LOCALHOST));
        let mapped = v6hex(Ipv6Addr::new(0, 0, 0, 0, 0, 0xffff, 0xc0a8, 0x010a));
        assert_eq!(
            parse_addr6(&mapped).unwrap().to_string(),
            "::ffff:192.168.1.10"
        );
        let g = v6hex("2001:db8::1".parse().unwrap());
        let (t, p, any) = parse_endpoint(&format!("{g}:0050"), true).unwrap();
        assert_eq!((t.as_str(), p, any), ("[2001:db8::1]:80", 80, false));
        let (t, _, any) = parse_endpoint(&format!("{}:0000", "0".repeat(32)), true).unwrap();
        assert_eq!((t.as_str(), any), ("[::]:0", true));
        assert_eq!(parse_addr6("1234"), None);
        assert_eq!(parse_addr6(&"é".repeat(16)), None);
    }

    #[test]
    fn stati() {
        assert_eq!(tcp_state("01"), "ESTAB");
        assert_eq!(tcp_state("0A"), "LISTEN");
        assert_eq!(tcp_state("06"), "TIME_WAIT");
        assert_eq!(tcp_state("0B"), "CLOSING");
        assert_eq!(tcp_state("FF"), "?");
    }

    fn tcp_text() -> String {
        format!(
            "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n\
   0: {l}:0016 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 11111 1 0 100 0 0 10 0\n\
   1: {a}:C9A2 {b}:01BB 01 00000010:00000020 02:000A7C 00000000  1000        0 22222 2 0 20 4 28 10 -1\n\
   2: garbage line\n\
   3: {a}:0050 {b}:D000 06 00000000:00000000 03:000 00000000     0        0 0 3 0\n",
            l = "00000000",
            a = le([192, 168, 1, 10]),
            b = le([151, 101, 1, 140]),
        )
    }

    #[test]
    fn tabella_tcp() {
        let v = parse_table(&tcp_text(), Proto::Tcp);
        assert_eq!(v.len(), 3);
        assert_eq!(v[0].local, "0.0.0.0:22");
        assert_eq!(v[0].remote, "*");
        assert_eq!(v[0].state, "LISTEN");
        assert!(v[0].is_listen());
        assert_eq!(v[0].inode, 11111);
        assert_eq!(v[1].local, "192.168.1.10:51618");
        assert_eq!(v[1].remote, "151.101.1.140:443");
        assert_eq!(v[1].state, "ESTAB");
        assert_eq!((v[1].tx_queue, v[1].rx_queue), (0x10, 0x20));
        assert_eq!(v[2].state, "TIME_WAIT");
        assert_eq!(v[2].inode, 0);
    }

    #[test]
    fn tabella_udp_e_v6_mappato() {
        let udp = format!(
            "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode ref pointer drops\n\
   0: {a}:1F90 00000000:0000 07 00000000:00000000 00:00000000 00000000  1000        0 33333 2 0 0\n",
            a = le([0, 0, 0, 0])
        );
        let u = parse_table(&udp, Proto::Udp);
        assert_eq!(u[0].proto.label(), "UDP");
        assert_eq!(u[0].state, "—");
        assert_eq!(u[0].local, "0.0.0.0:8080");
        let mapped = v6hex(Ipv6Addr::new(0, 0, 0, 0, 0, 0xffff, 0xc0a8, 0x010a));
        let remote = v6hex("2001:db8::1".parse().unwrap());
        let tcp6 = format!(
            "  sl  local_address rem_address st tx_queue rx_queue tr tm->when retrnsmt uid timeout inode\n\
   0: {mapped}:C350 {remote}:01BB 01 00000000:00000000 00:00000000 00000000  1000        0 44444 1 0 0\n"
        );
        let t = parse_table(&tcp6, Proto::Tcp6);
        assert_eq!(t[0].local, "[::ffff:192.168.1.10]:50000");
        assert_eq!(t[0].remote, "[2001:db8::1]:443");
        assert_eq!(t[0].proto, Proto::Tcp6);
        assert!(t[0].proto.is_tcp());
    }

    #[test]
    fn inode_dai_collegamenti() {
        assert_eq!(socket_inode("socket:[12345]"), Some(12345));
        assert_eq!(socket_inode("pipe:[12345]"), None);
        assert_eq!(socket_inode("socket:[x]"), None);
        assert_eq!(socket_inode("/dev/null"), None);
    }

    fn fixture_proc(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ngtop-conn-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("net")).unwrap();
        std::fs::write(d.join("net/tcp"), tcp_text()).unwrap();
        // pid 100 possiede il socket 11111, pid 200 il 22222; pid 300 ha solo un file.
        for (pid, ino) in [(100, 11111u64), (200, 22222)] {
            let fd = d.join(format!("{pid}/fd"));
            std::fs::create_dir_all(&fd).unwrap();
            symlink(format!("socket:[{ino}]"), fd.join("3")).unwrap();
            symlink("/dev/null", fd.join("0")).unwrap();
        }
        std::fs::create_dir_all(d.join("300/fd")).unwrap();
        symlink("/dev/null", d.join("300/fd/0")).unwrap();
        // Directory non numerica ignorata.
        std::fs::create_dir_all(d.join("self/fd")).unwrap();
        d
    }

    fn roots(proc: PathBuf) -> Roots {
        Roots {
            proc,
            ..Roots::default()
        }
    }

    #[test]
    fn associa_inode_a_pid() {
        let proc = fixture_proc("assoc");
        let r = roots(proc.clone());
        let mut info = ConnInfo::default();
        refresh(&mut info, &r);
        assert_eq!(info.conns.len(), 3);
        assert_eq!(info.conns[0].pid, Some(100));
        assert_eq!(info.conns[1].pid, Some(200));
        // inode 0 (TIME_WAIT): nessun processo.
        assert_eq!(info.conns[2].pid, None);

        // Incrementale: se i fd spariscono la cache regge finché la connessione esiste.
        std::fs::remove_dir_all(proc.join("100")).unwrap();
        refresh(&mut info, &r);
        assert_eq!(info.conns[0].pid, Some(100));
        // Quando la connessione sparisce, l'inode esce dalla cache.
        std::fs::write(proc.join("net/tcp"), "header\n").unwrap();
        refresh(&mut info, &r);
        assert!(info.conns.is_empty());
        assert!(info.cache.is_empty());
        let _ = std::fs::remove_dir_all(&proc);
    }

    #[test]
    fn inode_non_attribuibile_resta_senza_pid_e_non_si_riscandisce_subito() {
        let proc = fixture_proc("noperm");
        // Il socket 22222 non è più in nessun fd (permesso negato equivalente).
        std::fs::remove_dir_all(proc.join("200")).unwrap();
        let r = roots(proc.clone());
        let mut info = ConnInfo::default();
        refresh(&mut info, &r);
        assert_eq!(info.conns[1].pid, None);
        assert!(info.unresolved.contains(&22222));
        // Compare dopo: finché non scade il ritardo non viene cercato di nuovo...
        let fd = proc.join("200/fd");
        std::fs::create_dir_all(&fd).unwrap();
        symlink("socket:[22222]", fd.join("3")).unwrap();
        refresh(&mut info, &r);
        assert_eq!(info.conns[1].pid, None);
        // ...ma alla scadenza viene trovato.
        for _ in 0..RETRY_EVERY {
            refresh(&mut info, &r);
        }
        assert_eq!(info.conns[1].pid, Some(200));
        let _ = std::fs::remove_dir_all(&proc);
    }

    #[test]
    fn socket_non_attribuibile_non_riscandisce_ogni_giro() {
        let proc = fixture_proc("costo");
        // 22222 non attribuibile; 300 ha fd illeggibile (cartella senza permessi).
        std::fs::remove_dir_all(proc.join("200")).unwrap();
        std::fs::write(
            proc.join("300/stat"),
            "300 (x) S 1 1 1 0 -1 0 0 0 0 0 0 0 0 0 20 0 1 0 777 0 0\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(proc.join("300/fd"), std::fs::Permissions::from_mode(0o000))
            .unwrap();
        let r = roots(proc.clone());
        let mut info = ConnInfo::default();
        refresh(&mut info, &r);
        let first = info.visited;
        assert!(first >= 2);
        // I giri successivi, finché non scade la riprova, non aprono nessuna cartella fd.
        for _ in 0..(RETRY_EVERY - 1) {
            refresh(&mut info, &r);
        }
        assert_eq!(info.visited, first);
        // Alla riprova: parte la scansione, ma i pid noti vengono prima e 300 (se illeggibile
        // anche per chi esegue il test) è saltato in base alla cache.
        refresh(&mut info, &r);
        let second = info.visited - first;
        assert!(second >= 1);
        refresh(&mut info, &r);
        let _ =
            std::fs::set_permissions(proc.join("300/fd"), std::fs::Permissions::from_mode(0o755));
        let _ = std::fs::remove_dir_all(&proc);
    }

    #[test]
    fn processi_noti_per_primi_e_fermata_anticipata() {
        let proc = fixture_proc("ordine");
        let r = roots(proc.clone());
        let mut info = ConnInfo::default();
        refresh(&mut info, &r);
        let base = info.visited;
        // Nuova connessione dal pid 200 (già in cache).
        let t = std::fs::read_to_string(proc.join("net/tcp")).unwrap();
        let extra = t.lines().nth(2).unwrap().replace("22222", "88888");
        std::fs::write(proc.join("net/tcp"), format!("{t}{extra}\n")).unwrap();
        symlink("socket:[88888]", proc.join("200/fd/9")).unwrap();
        refresh(&mut info, &r);
        // Solo i due processi in cache (100 e 200), non tutta /proc (c'è anche il 300).
        assert_eq!(info.visited - base, 2);
        assert_eq!(info.conns.last().unwrap().pid, Some(200));
        let _ = std::fs::remove_dir_all(&proc);
    }

    #[test]
    fn proc_mancante_senza_panic() {
        let mut info = ConnInfo::default();
        refresh(&mut info, &roots(PathBuf::from("/nonexistent-ngtop-conn")));
        assert!(info.conns.is_empty());
    }

    /// Misura su /proc reale: `cargo test --release bench_refresh_reale -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn bench_refresh_reale() {
        let r = Roots::default();
        let mut info = ConnInfo::default();
        let t = std::time::Instant::now();
        refresh(&mut info, &r);
        let cold = t.elapsed();
        let n = 200;
        let t = std::time::Instant::now();
        for _ in 0..n {
            refresh(&mut info, &r);
        }
        println!(
            "BENCH conn: {} conn, {} without pid, cold {:?}, steady {:?}/round",
            info.conns.len(),
            info.conns.iter().filter(|c| c.pid.is_none()).count(),
            cold,
            t.elapsed() / n
        );
    }
}
