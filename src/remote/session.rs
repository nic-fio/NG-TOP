// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Sessione persistente (`ssh ... sh` o `sh` locale) e protocollo a record con delimitatore segreto.
//!
//! Scelta della codifica: i dati viaggiano GREZZI (zero overhead, corretti per qualsiasi byte, NUL e
//! assenza di newline finale compresi). Il corpo di un record finisce al primo `\n<NG> `, dove `NG`
//! è un nonce casuale di 128 bit scelto dal client a ogni sessione e mai scritto sulla riga di comando
//! remota (resta solo nello stdin dello `sh`), quindi nessun file o processo remoto può produrlo per
//! caso o per cattiveria. Nessuna lunghezza da conoscere in anticipo (i file di /proc dicono 0) e nessun
//! file temporaneo sul server. Vedi `agent.sh` per il formato dei record.

use std::io::{self, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::Target;

/// Tetti di memoria sui dati ricevuti da un server non fidato.
const MAX_BEFORE_READY: usize = 1 << 20;
const MAX_RESPONSE: usize = 128 << 20;

pub(crate) const AGENT: &str = include_str!("agent.sh");

/// Come avviare la sessione.
#[derive(Debug, Clone)]
pub(crate) enum Transport {
    /// `sh` locale (test, `NGTOP_SSH=local-sh`).
    LocalSh,
    /// ssh (o il comando di `NGTOP_SSH`) verso una destinazione.
    Ssh {
        cmd: Vec<String>,
        target: Target,
        compress: bool,
    },
}

impl Transport {
    fn command(&self, interactive: bool) -> Command {
        match self {
            Transport::LocalSh => {
                let mut c = Command::new("sh");
                c.env_remove("ENV").env("TERM", "dumb");
                c
            }
            Transport::Ssh {
                cmd,
                target,
                compress,
            } => {
                let mut c = Command::new(&cmd[0]);
                c.args(&cmd[1..]);
                c.arg("-T");
                c.args([
                    "-o",
                    "ServerAliveInterval=15",
                    "-o",
                    "ServerAliveCountMax=3",
                ]);
                if !interactive {
                    // riconnessioni durante la TUI: niente richieste sul terminale
                    c.args(["-o", "BatchMode=yes"]);
                }
                if *compress {
                    c.arg("-C");
                }
                if let Some(p) = target.port() {
                    c.arg("-p").arg(p.to_string());
                }
                // `env TERM=dumb`: i profili remoti che chiamano tput & co. non si lamentano (l'output spurio prima di
                // READY è comunque ignorato)
                c.arg(target.ssh_dest()).args(["env", "TERM=dumb", "sh"]);
                c
            }
        }
    }
}

/// Un record della risposta.
#[derive(Debug, Clone)]
pub(crate) struct Rec {
    pub kind: u8,
    pub arg: Vec<u8>,
    pub body: Vec<u8>,
    pub rc: i32,
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    let first = needle[0];
    let mut i = from;
    while i + needle.len() <= hay.len() {
        i += hay[i..hay.len() - needle.len() + 1]
            .iter()
            .position(|&b| b == first)?;
        if &hay[i..i + needle.len()] == needle {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Estrae il record che inizia in `from`. `Ok(None)` = dati incompleti.
fn parse_record(
    buf: &[u8],
    from: usize,
    ng: &str,
    hint: &mut usize,
) -> io::Result<Option<(Rec, usize)>> {
    let mark = format!("{ng} ");
    let m = mark.as_bytes();
    if buf.len() < from + m.len() {
        return Ok(None);
    }
    if &buf[from..from + m.len()] != m {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "remote protocol out of sync",
        ));
    }
    let Some(nl) = buf[from..].iter().position(|&b| b == b'\n') else {
        return Ok(None);
    };
    let head = &buf[from + m.len()..from + nl];
    let after = from + nl + 1;
    let kind = *head.first().unwrap_or(&b'?');
    let arg = head.get(2..).unwrap_or(&[]).to_vec();
    if matches!(kind, b'P' | b'Z' | b'R') {
        // R = READY, P = ping, Z = fine risposta: solo intestazione
        return Ok(Some((
            Rec {
                kind,
                arg,
                body: Vec::new(),
                rc: 0,
            },
            after,
        )));
    }
    let mut endmark = Vec::with_capacity(m.len() + 1);
    endmark.push(b'\n');
    endmark.extend_from_slice(m);
    // riparte da dove si era arrivati (corpi grandi in arrivo a pezzi: niente scansioni quadratiche)
    let Some(e) = find(buf, &endmark, after.max(*hint)) else {
        *hint = buf.len().saturating_sub(endmark.len());
        return Ok(None);
    };
    // "! rc\n"
    let tail = &buf[e + endmark.len()..];
    let Some(tnl) = tail.iter().position(|&b| b == b'\n') else {
        return Ok(None);
    };
    let rc = std::str::from_utf8(&tail[..tnl])
        .ok()
        .and_then(|t| t.strip_prefix("! "))
        .and_then(|t| t.trim().parse::<i32>().ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "bad record trailer"))?;
    let body = buf[after..e].to_vec();
    Ok(Some((
        Rec {
            kind,
            arg,
            body,
            rc,
        },
        e + endmark.len() + tnl + 1,
    )))
}

/// Quota un argomento per `sh` (apici singoli; il newline diventa `"$NL"`). `None` se contiene NUL.
pub(crate) fn sh_quote(s: &[u8], out: &mut Vec<u8>) -> bool {
    if s.contains(&0) {
        return false;
    }
    out.push(b'\'');
    for &b in s {
        match b {
            b'\'' => out.extend_from_slice(b"'\\''"),
            b'\n' => out.extend_from_slice(b"'\"$NL\"'"),
            _ => out.push(b),
        }
    }
    out.push(b'\'');
    true
}

/// Errno -> errore Rust.
pub(crate) fn rc_error(rc: i32) -> io::Error {
    io::Error::from_raw_os_error(rc)
}

pub(crate) struct Session {
    child: Child,
    tx: Option<Sender<Vec<u8>>>,
    rx: Receiver<Vec<u8>>,
    buf: Vec<u8>,
    pos: usize,
    ng: String,
    seq: u64,
    hint: usize,
    /// Dimensione della pagina e tick al secondo del server (`getconf`, ripiego 4096 / 100).
    pub page_size: u64,
    pub clock_ticks: u64,
    pub stderr_tail: Arc<Mutex<Vec<u8>>>,
    pub bytes_in: Arc<AtomicU64>,
}

impl Drop for Session {
    fn drop(&mut self) {
        self.tx = None;
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn nonce() -> String {
    let mut b = [0u8; 16];
    let ok = std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut b))
        .is_ok();
    if !ok {
        use std::hash::{BuildHasher, Hasher};
        for (i, c) in b.chunks_mut(8).enumerate() {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u128(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos())
                    + i as u128,
            );
            c.copy_from_slice(&h.finish().to_le_bytes());
        }
    }
    let mut s = String::from("ngtop-");
    for x in b {
        s.push_str(&format!("{x:02x}"));
    }
    s
}

impl Session {
    /// Avvia il processo, invia l'agente e attende `READY`. Con `interactive` lo stderr del processo
    /// (richieste di password, host key…) è inoltrato al terminale fino a `READY`.
    pub fn start(tr: &Transport, interactive: bool) -> io::Result<Session> {
        let mut cmd = tr.command(interactive);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn()?;
        let mut stdin = child.stdin.take().unwrap();
        let mut stdout = child.stdout.take().unwrap();
        let mut stderr = child.stderr.take().unwrap();

        let passthrough = Arc::new(AtomicBool::new(interactive));
        let stderr_tail = Arc::new(Mutex::new(Vec::new()));
        {
            let (pt, tail) = (passthrough.clone(), stderr_tail.clone());
            std::thread::spawn(move || {
                let mut b = [0u8; 1024];
                while let Ok(n) = stderr.read(&mut b) {
                    if n == 0 {
                        break;
                    }
                    if pt.load(Ordering::Relaxed) {
                        let _ = io::stderr().write_all(&b[..n]);
                    }
                    let mut t = tail.lock().unwrap_or_else(|e| e.into_inner());
                    t.extend_from_slice(&b[..n]);
                    if t.len() > 4096 {
                        let cut = t.len() - 4096;
                        t.drain(..cut);
                    }
                }
            });
        }
        let bytes_in = Arc::new(AtomicU64::new(0));
        let (otx, rx) = mpsc::channel::<Vec<u8>>();
        {
            let bi = bytes_in.clone();
            std::thread::spawn(move || {
                let mut b = vec![0u8; 64 * 1024];
                while let Ok(n) = stdout.read(&mut b) {
                    if n == 0 {
                        break;
                    }
                    bi.fetch_add(n as u64, Ordering::Relaxed);
                    if otx.send(b[..n].to_vec()).is_err() {
                        break;
                    }
                }
            });
        }
        let (tx, wrx) = mpsc::channel::<Vec<u8>>();
        std::thread::spawn(move || {
            while let Ok(m) = wrx.recv() {
                if stdin.write_all(&m).and_then(|_| stdin.flush()).is_err() {
                    break;
                }
            }
        });

        let ng = nonce();
        let mut s = Session {
            child,
            tx: Some(tx),
            rx,
            buf: Vec::new(),
            pos: 0,
            ng: ng.clone(),
            seq: 0,
            hint: 0,
            page_size: 4096,
            clock_ticks: 100,
            stderr_tail,
            bytes_in,
        };
        let mut init = format!("NG='{ng}'\n").into_bytes();
        init.extend_from_slice(AGENT.as_bytes());
        init.push(b'\n');
        s.send(init)?;
        // attesa di READY (anche minuti: ssh può chiedere la password)
        let marker = format!("{ng} READY").into_bytes();
        let deadline = Instant::now() + Duration::from_secs(300);
        loop {
            if let Some(i) = find(&s.buf, &marker, 0) {
                let nl = s.buf[i..].iter().position(|&b| b == b'\n');
                if let Some(nl) = nl {
                    // "<NG> READY <flag> <pagesize> <clk_tck>"
                    let line =
                        String::from_utf8_lossy(&s.buf[i + marker.len()..i + nl]).into_owned();
                    let mut f = line.split_whitespace().skip(1);
                    s.page_size = f
                        .next()
                        .and_then(|x| x.parse().ok())
                        .filter(|&n| n > 0)
                        .unwrap_or(4096);
                    s.clock_ticks = f
                        .next()
                        .and_then(|x| x.parse().ok())
                        .filter(|&n| n > 0)
                        .unwrap_or(100);
                    s.pos = i + nl + 1;
                    break;
                }
            }
            if s.buf.len() > MAX_BEFORE_READY {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "too much output before the remote agent started",
                ));
            }
            let left = deadline.saturating_duration_since(Instant::now());
            match s.rx.recv_timeout(left) {
                Ok(c) => s.buf.extend_from_slice(&c),
                Err(RecvTimeoutError::Timeout) => {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "connection timed out",
                    ));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    let _ = s.child.wait();
                    return Err(io::Error::new(
                        io::ErrorKind::ConnectionAborted,
                        s.failure_text(),
                    ));
                }
            }
        }
        passthrough.store(false, Ordering::Relaxed);
        Ok(s)
    }

    fn failure_text(&self) -> String {
        let t = self.stderr_tail.lock().unwrap_or_else(|e| e.into_inner());
        let t = String::from_utf8_lossy(&t);
        let t = t.trim();
        if t.is_empty() {
            "connection closed".to_string()
        } else {
            t.lines().last().unwrap_or(t).to_string()
        }
    }

    fn send(&self, data: Vec<u8>) -> io::Result<()> {
        self.tx
            .as_ref()
            .and_then(|t| t.send(data).ok())
            .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "session closed"))
    }

    /// Invia le righe di richiesta e raccoglie i record fino a `Z`. Restituisce i record e il tempo
    /// fino al record `P` (andata e ritorno a vuoto). Ogni errore lascia la sessione inutilizzabile.
    pub fn exchange(
        &mut self,
        lines: &[u8],
        timeout: Duration,
    ) -> io::Result<(Vec<Rec>, Duration)> {
        self.seq += 1;
        let seq = self.seq;
        let mut req = b"p\n".to_vec();
        req.extend_from_slice(lines);
        req.extend_from_slice(format!("z {seq}\n").as_bytes());
        let t0 = Instant::now();
        self.send(req)?;
        let deadline = t0 + timeout;
        let mut recs = Vec::new();
        let mut rtt = Duration::ZERO;
        loop {
            while let Some((r, next)) = parse_record(&self.buf, self.pos, &self.ng, &mut self.hint)?
            {
                self.pos = next;
                self.hint = 0;
                match r.kind {
                    b'P' => rtt = t0.elapsed(),
                    b'Z' => {
                        if r.arg == seq.to_string().as_bytes() {
                            self.buf.drain(..self.pos);
                            self.pos = 0;
                            self.hint = 0;
                            return Ok((recs, rtt));
                        }
                    }
                    _ => recs.push(r),
                }
            }
            if self.pos > 1 << 20 {
                self.buf.drain(..self.pos);
                self.hint = self.hint.saturating_sub(self.pos);
                self.pos = 0;
            }
            if self.buf.len() > MAX_RESPONSE {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "remote response too large",
                ));
            }
            let left = deadline.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(left) {
                Ok(c) => self.buf.extend_from_slice(&c),
                Err(RecvTimeoutError::Timeout) => {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "remote request timed out",
                    ));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(io::Error::new(
                        io::ErrorKind::ConnectionAborted,
                        self.failure_text(),
                    ));
                }
            }
        }
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Termina il processo (usato dai test per simulare la caduta della connessione).
    pub fn kill(&mut self) {
        let _ = self.child.kill();
    }
}
