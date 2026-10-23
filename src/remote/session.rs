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
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::Target;
use super::sudo::{self, SudoMode, Verdict, wipe};

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
        sudo: SudoMode,
    },
}

impl Transport {
    fn sudo(&self) -> Option<&SudoMode> {
        match self {
            Transport::Ssh { sudo, .. } if sudo.is_on() => Some(sudo),
            _ => None,
        }
    }

    /// `tag` e `prompt`: vedi `SudoMode::remote_args`.
    fn command(&self, interactive: bool, tag: &str, prompt: &str) -> Command {
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
                sudo,
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
                c.arg(target.ssh_dest()).args(sudo.remote_args(tag, prompt));
                c
            }
        }
    }
}

/// Dati dell'handshake sudo.
struct Handshake<'a> {
    tag: &'a str,
    slot: &'a sudo::PwSlot,
    target: &'a Target,
    prompts: &'a AtomicUsize,
    scrub: &'a AtomicBool,
    interactive: bool,
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
    /// uid dello script sul server (0 = root) e catena dei suoi antenati fino a PID 1 escluso.
    pub uid: Option<u32>,
    pub self_pids: Vec<u32>,
    /// sudo attivo su questa sessione.
    sudo: bool,
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

/// Unico punto di pulizia dei testi che vengono dal server (hostname, stderr, messaggi di stato): i
/// caratteri di controllo (ESC, BEL, ...) diventano `?`, così non raggiungono mai il terminale.
pub(crate) fn clean(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

/// Aggiunge una riga di stderr a `tail` (ultimi 4 KiB). Dopo l'invio della password (`scrub`) non si
/// conserva testo del server: se la riga è un messaggio noto di sudo ne resta solo il testo canonico, il
/// resto (ciò che una shell potrebbe stampare eseguendo la riga della password) è scartato.
fn keep(tail: &Mutex<Vec<u8>>, line: &[u8], scrub: bool) {
    let canon;
    let text: &[u8] = if scrub {
        match sudo::classify(&String::from_utf8_lossy(line)) {
            Some(v) => {
                canon = v.text();
                canon.as_bytes()
            }
            None => return,
        }
    } else {
        line
    };
    if text.is_empty() {
        return;
    }
    let mut t = tail.lock().unwrap_or_else(|e| e.into_inner());
    t.extend_from_slice(text);
    t.push(b'\n');
    if t.len() > 4096 {
        let cut = t.len() - 4096;
        t.drain(..cut);
    }
}

impl Session {
    /// Avvia il processo, invia l'agente e attende `READY`. Con `interactive` lo stderr del processo
    /// (richieste di password, host key…) è inoltrato al terminale fino a `READY`.
    pub fn start(tr: &Transport, interactive: bool) -> io::Result<Session> {
        let sudo_mode = tr.sudo().cloned();
        // prompt e marcatore di sudo: casuali, diversi dal nonce del protocollo (che non compare mai sulla
        // riga di comando). Il prompt non contiene `%`.
        let tag = nonce().replace("ngtop-", "");
        let prompt = format!("ngsudo-P-{}:", nonce().replace("ngtop-", ""));
        let mut cmd = tr.command(interactive, &tag, &prompt);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn()?;
        let mut stdin = child.stdin.take().unwrap();
        let mut stdout = child.stdout.take().unwrap();
        let mut stderr = child.stderr.take().unwrap();

        // con sudo lo stderr non va al terminale: i suoi messaggi diventano l'errore di connect
        let passthrough = Arc::new(AtomicBool::new(interactive && sudo_mode.is_none()));
        let stderr_tail = Arc::new(Mutex::new(Vec::new()));
        // quante volte sudo ha mostrato il prompt (a inizio riga, completo) e se la password è già partita
        let prompts = Arc::new(AtomicUsize::new(0));
        let scrub = Arc::new(AtomicBool::new(false));
        {
            let (pt, tail) = (passthrough.clone(), stderr_tail.clone());
            let (prompts, scrub) = (prompts.clone(), scrub.clone());
            let want = sudo_mode.as_ref().map(|_| prompt.clone().into_bytes());
            std::thread::spawn(move || {
                let mut b = [0u8; 1024];
                // riga corrente non ancora terminata
                let mut line: Vec<u8> = Vec::new();
                while let Ok(n) = stderr.read(&mut b) {
                    if n == 0 {
                        break;
                    }
                    if pt.load(Ordering::Relaxed) {
                        let _ = io::stderr().write_all(&b[..n]);
                    }
                    for &c in &b[..n] {
                        if c == b'\n' {
                            keep(&tail, &line, scrub.load(Ordering::SeqCst));
                            line.clear();
                        } else {
                            line.push(c);
                            if line.len() > 4096 {
                                line.drain(..2048);
                            }
                        }
                        // il prompt vale solo se è l'intera riga corrente
                        if want.as_deref() == Some(line.as_slice()) {
                            prompts.fetch_add(1, Ordering::SeqCst);
                            line.clear();
                        }
                    }
                }
                keep(&tail, &line, scrub.load(Ordering::SeqCst));
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
            while let Ok(mut m) = wrx.recv() {
                let r = stdin.write_all(&m).and_then(|_| stdin.flush());
                // può contenere la password: azzerato appena scritto
                wipe(&mut m);
                if r.is_err() {
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
            uid: None,
            self_pids: Vec::new(),
            sudo: sudo_mode.is_some(),
            stderr_tail,
            bytes_in,
        };
        if let Some(SudoMode::On(slot)) = &sudo_mode {
            let Transport::Ssh { target, .. } = tr else {
                unreachable!("sudo solo con ssh")
            };
            let h = Handshake {
                tag: &tag,
                slot,
                target,
                prompts: &prompts,
                scrub: &scrub,
                interactive,
            };
            s.sudo_handshake(&h)?;
        }
        let mut init = format!("NG='{ng}'\n").into_bytes();
        init.extend_from_slice(AGENT.as_bytes());
        init.push(b'\n');
        s.send(init)?;
        // attesa di READY (anche minuti: ssh può chiedere la password)
        let ready = format!("{ng} READY").into_bytes();
        let deadline = Instant::now() + Duration::from_secs(300);
        loop {
            if let Some(i) = find(&s.buf, &ready, 0) {
                let nl = s.buf[i..].iter().position(|&b| b == b'\n');
                if let Some(nl) = nl {
                    // "<NG> READY <flag> <pagesize> <clk_tck> <uid> <antenati>"
                    let line =
                        String::from_utf8_lossy(&s.buf[i + ready.len()..i + nl]).into_owned();
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
                    s.uid = f.next().and_then(|x| x.parse().ok());
                    s.self_pids = f
                        .next()
                        .map(|c| c.split(',').filter_map(|x| x.parse().ok()).collect())
                        .unwrap_or_default();
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
                    return Err(s.connect_failure());
                }
            }
        }
        passthrough.store(false, Ordering::Relaxed);
        Ok(s)
    }

    /// Errore per un processo morto prima di READY: con sudo, il verdetto letto dallo stderr.
    fn connect_failure(&self) -> io::Error {
        // lo stderr arriva da un altro thread: gli si lascia un attimo per svuotarsi
        let t0 = Instant::now();
        while self.tail_text().trim().is_empty() && t0.elapsed() < Duration::from_millis(300) {
            std::thread::sleep(Duration::from_millis(20));
        }
        if self.sudo
            && let Some(v) = sudo::classify(&self.tail_text())
        {
            return io::Error::other(v.text());
        }
        io::Error::new(io::ErrorKind::ConnectionAborted, self.failure_text())
    }

    fn tail_text(&self) -> String {
        let t = self.stderr_tail.lock().unwrap_or_else(|e| e.into_inner());
        String::from_utf8_lossy(&t).into_owned()
    }

    /// Handshake con sudo, su UNA connessione. Lo script non parte finché non arriva il marcatore.
    /// - marcatore subito: sudo non vuole la password (NOPASSWD), non ne viene mandata nessuna;
    /// - prompt (quello casuale, a inizio riga): la password parte SOLO ora, una volta, poi si aspetta il
    ///   marcatore; un secondo prompt = password sbagliata;
    /// - la password non si manda mai "al buio": se sudo non la chiede, nessuno la legge.
    ///
    /// Errore o scadenza: il chiamante scarta la sessione (stdin chiuso, ssh ucciso).
    fn sudo_handshake(&mut self, h: &Handshake) -> io::Result<()> {
        let mut want = format!("ngsudo-{}", h.tag).into_bytes();
        want.push(b'\n');
        let mut sent: Option<sudo::Password> = None;
        // prima del prompt può esserci l'autenticazione di ssh (utente che digita); dopo, pochi secondi
        let mut deadline =
            Instant::now() + Duration::from_secs(if h.interactive { 300 } else { 30 });
        loop {
            if let Some(i) = find(&self.buf, &want, 0) {
                self.buf.drain(..i + want.len());
                if let Some(p) = sent {
                    h.slot.set(p);
                }
                return Ok(());
            }
            if self.buf.len() > MAX_BEFORE_READY {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "too much output before the remote agent started",
                ));
            }
            let n = h.prompts.load(Ordering::SeqCst);
            if n >= 2 {
                return Err(io::Error::other(Verdict::Wrong.text()));
            }
            if n == 1 && sent.is_none() {
                let pw = match h.slot.get() {
                    Some(p) => p,
                    None if h.interactive => sudo::ask_password(h.target)?,
                    None => return Err(io::Error::other(sudo::NEED_PASSWORD)),
                };
                // da qui in poi niente testo del server nello stderr_tail
                h.scrub.store(true, Ordering::SeqCst);
                self.send(pw.line())?;
                sent = Some(pw);
                deadline = Instant::now() + Duration::from_secs(20);
                continue;
            }
            if let Some(v) = sudo::classify(&self.tail_text()) {
                return Err(io::Error::other(v.text()));
            }
            if Instant::now() >= deadline {
                return Err(if sent.is_some() {
                    io::Error::other(Verdict::Wrong.text())
                } else {
                    io::Error::new(io::ErrorKind::TimedOut, "connection timed out")
                });
            }
            match self.rx.recv_timeout(Duration::from_millis(50)) {
                Ok(c) => self.buf.extend_from_slice(&c),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    let _ = self.child.wait();
                    return Err(self.connect_failure());
                }
            }
        }
    }

    fn failure_text(&self) -> String {
        let t = self.stderr_tail.lock().unwrap_or_else(|e| e.into_inner());
        let t = String::from_utf8_lossy(&t);
        let t = t.trim();
        if t.is_empty() {
            "connection closed".to_string()
        } else {
            clean(t.lines().last().unwrap_or(t))
        }
    }

    fn send(&self, mut data: Vec<u8>) -> io::Result<()> {
        let closed = || io::Error::new(io::ErrorKind::BrokenPipe, "session closed");
        let Some(tx) = self.tx.as_ref() else {
            wipe(&mut data);
            return Err(closed());
        };
        // se `data` era la password e l'invio fallisce, non resta in giro
        tx.send(data).map_err(|mut e| {
            wipe(&mut e.0);
            closed()
        })
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

#[cfg(test)]
pub(crate) fn keep_for_test(tail: &Mutex<Vec<u8>>, line: &[u8], scrub: bool) {
    keep(tail, line, scrub);
}
