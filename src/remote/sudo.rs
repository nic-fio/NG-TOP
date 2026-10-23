// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! `--sudo`: comando remoto, password in memoria e riconoscimento degli errori di sudo.
//!
//! La password sta solo qui: mai in argv, nei log o nei testi d'errore; passa dallo stdin del processo
//! ssh una sola volta, e il buffer viene azzerato quando il writer ha finito e alla distruzione.

use std::fmt;
use std::io::{self, Read, Write};
use std::sync::{Arc, Mutex};

use rustix::termios::{LocalModes, OptionalActions, SpecialCodeIndex, tcgetattr, tcsetattr};

use super::Target;

/// Sudo chiede la password ma non c'è modo di chiederla (riconnessione, niente terminale).
pub(crate) const NEED_PASSWORD: &str = "sudo: a password is required";
pub(crate) const ERR_WRONG: &str = "sudo: wrong password";
pub(crate) const ERR_TTY: &str = "sudo: a terminal is required (requiretty)";
pub(crate) const ERR_NOT_ALLOWED: &str = "sudo: not allowed for this user";
pub(crate) const ERR_MISSING: &str = "sudo: not installed on the server";

/// Lunghezza massima accettata per la password.
const MAX_PASSWORD: usize = 1000;

/// Azzera un buffer (anche la capacità non usata) in modo che il compilatore non lo elimini.
pub(crate) fn wipe(v: &mut Vec<u8>) {
    let cap = v.capacity();
    v.clear();
    v.resize(cap, 0);
    v.fill(0);
    std::hint::black_box(&v);
    v.clear();
}

struct Secret {
    bytes: Vec<u8>,
}

impl Drop for Secret {
    fn drop(&mut self) {
        wipe(&mut self.bytes);
    }
}

/// Password sudo, solo in memoria. Nessuna `Display`; `Debug` non mostra niente.
#[derive(Clone)]
pub struct Password(Arc<Secret>);

impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Password(..)")
    }
}

impl Password {
    /// La riga da scrivere sullo stdin di `sudo -S` (password + newline), in un buffer a capacità esatta
    /// (nessuna riallocazione: nessuna copia dimenticata). Chi la riceve deve azzerarla con [`wipe`].
    pub(crate) fn line(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(self.0.bytes.len() + 1);
        v.extend_from_slice(&self.0.bytes);
        v.push(b'\n');
        v
    }
}

/// Password tenuta per le riconnessioni: vuota finché sudo non ne ha chiesta una.
#[derive(Default)]
pub(crate) struct PwSlot(Mutex<Option<Password>>);

impl fmt::Debug for PwSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PwSlot(..)")
    }
}

impl PwSlot {
    pub fn get(&self) -> Option<Password> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    pub fn set(&self, p: Password) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(p);
    }
}

/// Come ottenere i privilegi sul server.
#[derive(Debug, Clone)]
pub(crate) enum SudoMode {
    /// Nessun sudo.
    Off,
    /// Sempre `sudo -S -k` con un prompt casuale: la password parte solo dopo aver visto QUEL prompt.
    On(Arc<PwSlot>),
}

impl SudoMode {
    pub fn new() -> SudoMode {
        SudoMode::On(Arc::new(PwSlot::default()))
    }

    pub fn is_on(&self) -> bool {
        !matches!(self, SudoMode::Off)
    }

    /// Argomenti del comando remoto (ssh li unisce con spazi per la shell del server). `tag`: esadecimale
    /// casuale; `prompt`: il prompt di sudo (casuale, senza `%`).
    ///
    /// Il marcatore stampato dalla shell avviata da sudo è costruito a run time (`printf '%s-%s'`), quindi
    /// non compare mai uguale nella riga di comando (un profilo che la stampasse non potrebbe falsificarlo).
    /// Se arriva senza che sudo abbia chiesto la password, la password non viene mai mandata.
    pub fn remote_args(&self, tag: &str, prompt: &str) -> Vec<String> {
        match self {
            SudoMode::Off => ["env", "TERM=dumb", "sh"].map(String::from).to_vec(),
            // LC_ALL=C: messaggi di sudo in inglese. -k: ignora l'autenticazione in cache, così se la
            // password serve sudo la chiede sempre (e la consuma lui, non la shell).
            SudoMode::On(_) => vec![format!(
                "env LC_ALL=C sudo -S -k -p '{prompt}' env TERM=dumb sh -c 'printf \"%s-%s\\n\" ngsudo {tag}; exec sh'"
            )],
        }
    }
}

/// Cosa dice lo stderr (inglese, grazie a LC_ALL=C). I testi canonici sono quelli che noi stessi mettiamo
/// nello stderr_tail dopo l'invio della password.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    Wrong,
    Tty,
    NotAllowed,
    Missing,
}

impl Verdict {
    pub fn text(self) -> &'static str {
        match self {
            Verdict::Wrong => ERR_WRONG,
            Verdict::Tty => ERR_TTY,
            Verdict::NotAllowed => ERR_NOT_ALLOWED,
            Verdict::Missing => ERR_MISSING,
        }
    }
}

/// Riconosce l'esito di sudo dallo stderr.
pub(crate) fn classify(stderr: &str) -> Option<Verdict> {
    let t = stderr.to_ascii_lowercase();
    let has = |p: &[&str]| p.iter().any(|x| t.contains(x));
    if has(&[
        "you must have a tty",
        "no tty present",
        "a terminal is required",
        "(requiretty)",
    ]) {
        Some(Verdict::Tty)
    } else if has(&[
        "is not in the sudoers",
        "may not run sudo",
        "is not allowed to run sudo",
        "not allowed for this user",
    ]) {
        Some(Verdict::NotAllowed)
    } else if has(&[
        "sudo: not found",
        "sudo: command not found",
        "not installed on the server",
    ]) {
        Some(Verdict::Missing)
    } else if has(&[
        "try again",
        "incorrect password",
        "no password was provided",
        "authentication failure",
        "sudo: wrong password",
    ]) {
        Some(Verdict::Wrong)
    } else {
        None
    }
}

/// Chiede la password sul terminale (`/dev/tty`), senza eco: `[sudo] password for utente@host: `
/// (solo `host` se la destinazione non ha un utente: quello del server non è ancora noto).
pub fn ask_password(t: &Target) -> io::Result<Password> {
    let prompt = match t.user() {
        Some(u) => format!("[sudo] password for {u}@{}: ", t.host()),
        None => format!("[sudo] password for {}: ", t.host()),
    };
    read_password(&prompt)
}

struct Restore<'a> {
    tty: &'a std::fs::File,
    old: rustix::termios::Termios,
}

impl Drop for Restore<'_> {
    fn drop(&mut self) {
        let _ = tcsetattr(self.tty, OptionalActions::Flush, &self.old);
    }
}

/// Elimina l'ultimo carattere (UTF-8) dal buffer della password.
fn erase_char(buf: &mut Vec<u8>) {
    while let Some(b) = buf.pop() {
        if b & 0xC0 != 0x80 {
            break;
        }
    }
}

/// Legge una riga dal terminale senza eco. Il terminale è in modo non canonico: Invio termina, Backspace
/// e Ctrl-U li gestiamo noi, Ctrl-C e Ctrl-D annullano subito, gli altri caratteri di controllo (Ctrl-\
/// compreso) sono scartati.
pub fn read_password(prompt: &str) -> io::Result<Password> {
    let tty = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .map_err(|_| io::Error::other("sudo: a password is required (no terminal to ask it)"))?;
    let old = tcgetattr(&tty).map_err(io::Error::from)?;
    // la guardia esiste PRIMA di cambiare il terminale: qualunque uscita lo ripristina
    let _restore = Restore {
        tty: &tty,
        old: old.clone(),
    };
    let mut raw = old;
    raw.local_modes
        .remove(LocalModes::ECHO | LocalModes::ISIG | LocalModes::ICANON);
    raw.special_codes[SpecialCodeIndex::VMIN] = 1;
    raw.special_codes[SpecialCodeIndex::VTIME] = 0;
    tcsetattr(&tty, OptionalActions::Flush, &raw).map_err(io::Error::from)?;
    harden();
    let mut out: &std::fs::File = &tty;
    out.write_all(prompt.as_bytes())?;
    out.flush()?;
    let mut buf = Vec::with_capacity(MAX_PASSWORD + 1);
    let mut b = [0u8; 1];
    let res = loop {
        match (&tty).read(&mut b) {
            Ok(0) => break Err(io::Error::other("cancelled")),
            Ok(_) => match b[0] {
                b'\n' | b'\r' => break Ok(()),
                3 | 4 => break Err(io::Error::other("cancelled")),
                0x7f | 0x08 => erase_char(&mut buf),
                0x15 => {
                    let mut t = std::mem::take(&mut buf);
                    wipe(&mut t);
                    buf = Vec::with_capacity(MAX_PASSWORD + 1);
                }
                c if c < 0x20 => {}
                c => {
                    if buf.len() >= MAX_PASSWORD {
                        break Err(io::Error::other("password too long"));
                    }
                    buf.push(c);
                }
            },
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => break Err(e),
        }
    };
    b[0] = 0;
    std::hint::black_box(&b);
    let _ = out.write_all(b"\n");
    match res {
        Ok(()) => Ok(Password(Arc::new(Secret { bytes: buf }))),
        Err(e) => {
            wipe(&mut buf);
            Err(e)
        }
    }
}

/// Con una password in memoria il processo non è più "dumpable": niente core dump né ptrace/lettura di
/// /proc/PID/mem da parte di altri processi dello stesso utente. (Nessun mlock: richiede `unsafe`.)
fn harden() {
    let _ = rustix::process::set_dumpable_behavior(rustix::process::DumpableBehavior::NotDumpable);
}

#[cfg(test)]
impl Password {
    pub(crate) fn from_str_for_test(s: &str) -> Password {
        Password(Arc::new(Secret {
            bytes: s.as_bytes().to_vec(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn riconosce_gli_errori_di_sudo() {
        let c = classify;
        assert_eq!(c("Sorry, try again.\n"), Some(Verdict::Wrong));
        assert_eq!(
            c("sudo: 1 incorrect password attempt\n"),
            Some(Verdict::Wrong)
        );
        assert_eq!(
            c("sudo: sorry, you must have a tty to run sudo\n"),
            Some(Verdict::Tty)
        );
        assert_eq!(
            c("bob is not in the sudoers file.\n"),
            Some(Verdict::NotAllowed)
        );
        assert_eq!(
            c("Sorry, user bob may not run sudo on host.\n"),
            Some(Verdict::NotAllowed)
        );
        assert_eq!(c("sh: 1: sudo: not found\n"), Some(Verdict::Missing));
        assert_eq!(c("Connection refused\n"), None);
        // i testi canonici si riconoscono (servono dopo l'invio della password)
        for v in [
            Verdict::Wrong,
            Verdict::Tty,
            Verdict::NotAllowed,
            Verdict::Missing,
        ] {
            assert_eq!(c(v.text()), Some(v));
        }
    }

    #[test]
    fn cancellazione_di_un_carattere() {
        let mut v = "aé".as_bytes().to_vec();
        erase_char(&mut v);
        assert_eq!(v, b"a");
        erase_char(&mut v);
        erase_char(&mut v);
        assert!(v.is_empty());
    }

    #[test]
    fn password_non_si_vede_e_si_azzera() {
        let p = Password::from_str_for_test("segreto");
        assert!(!format!("{p:?}").contains("segreto"));
        let mut l = p.line();
        assert_eq!(l, b"segreto\n");
        wipe(&mut l);
        assert!(l.is_empty());
    }
}
