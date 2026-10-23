// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! `--sudo`: comando remoto, password in memoria e riconoscimento degli errori di sudo.
//!
//! La password sta solo qui: mai in argv, nei log o nei testi d'errore; passa dallo stdin del processo
//! ssh una sola volta, e il buffer viene azzerato quando il writer ha finito e alla distruzione.

use std::fmt;
use std::io::{self, Read, Write};
use std::sync::Arc;

use rustix::termios::{LocalModes, OptionalActions, tcgetattr, tcsetattr};

use super::Target;

/// Errore del primo tentativo (`sudo -n`) che dice "serve la password": `connect` lo riconosce e chiede.
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

/// Come ottenere i privilegi sul server.
#[derive(Debug, Clone)]
pub(crate) enum SudoMode {
    /// Nessun sudo.
    Off,
    /// `sudo -n` (NOPASSWD).
    NoPasswd,
    /// `sudo -S` con la password inviata una sola volta, poi attesa del marcatore.
    Password(Password),
}

impl SudoMode {
    pub fn is_on(&self) -> bool {
        !matches!(self, SudoMode::Off)
    }

    /// Argomenti del comando remoto (ssh li unisce con spazi per la shell del server). Con la password
    /// la shell avviata da sudo stampa `marker` prima di diventare l'agente: finché non arriva, lo
    /// script non viene mandato (altrimenti sudo lo leggerebbe come password).
    pub fn remote_args(&self, marker: &str) -> Vec<String> {
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect();
        match self {
            SudoMode::Off => v(&["env", "TERM=dumb", "sh"]),
            // LC_ALL=C: i messaggi di sudo restano in inglese (li riconosciamo dal testo)
            SudoMode::NoPasswd => v(&["env", "LC_ALL=C", "sudo", "-n", "env", "TERM=dumb", "sh"]),
            // -k: ignora un'eventuale autenticazione già in cache, così la riga di password viene sempre
            // consumata da sudo e non resta in stdin per la shell
            SudoMode::Password(_) => vec![format!(
                "env LC_ALL=C sudo -S -k -p '' env TERM=dumb sh -c 'echo {marker}; exec sh'"
            )],
        }
    }
}

/// Il primo tentativo (`sudo -n`) è fallito perché serve la password.
pub(crate) fn needs_password(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::PermissionDenied && e.to_string() == NEED_PASSWORD
}

/// Cosa dice lo stderr raccolto (inglese, grazie a LC_ALL=C).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    NeedPassword,
    Wrong,
    Tty,
    NotAllowed,
    Missing,
}

impl Verdict {
    pub fn text(self) -> &'static str {
        match self {
            Verdict::NeedPassword => NEED_PASSWORD,
            Verdict::Wrong => ERR_WRONG,
            Verdict::Tty => ERR_TTY,
            Verdict::NotAllowed => ERR_NOT_ALLOWED,
            Verdict::Missing => ERR_MISSING,
        }
    }
}

/// Riconosce l'esito di sudo dallo stderr. `password`: siamo nel tentativo con `-S`.
pub(crate) fn classify(stderr: &str, password: bool) -> Option<Verdict> {
    let t = stderr.to_ascii_lowercase();
    let has = |p: &[&str]| p.iter().any(|x| t.contains(x));
    if has(&[
        "you must have a tty",
        "no tty present",
        "a terminal is required",
    ]) {
        return Some(Verdict::Tty);
    }
    if has(&[
        "is not in the sudoers",
        "may not run sudo",
        "is not allowed to run sudo",
    ]) {
        return Some(Verdict::NotAllowed);
    }
    if has(&["sudo: not found", "sudo: command not found"]) {
        return Some(Verdict::Missing);
    }
    if password {
        if has(&[
            "try again",
            "incorrect password",
            "no password was provided",
            "authentication failure",
        ]) {
            return Some(Verdict::Wrong);
        }
    } else if has(&["a password is required"]) {
        return Some(Verdict::NeedPassword);
    }
    None
}

/// Chiede la password sul terminale (`/dev/tty`), senza eco: `[sudo] password for utente@host: `.
/// Ctrl-C annulla (ISIG è spento durante la lettura, così il terminale non resta senza eco).
pub fn ask_password(t: &Target) -> io::Result<Password> {
    let who = match t.user() {
        Some(u) => u.to_string(),
        None => std::env::var("USER").unwrap_or_default(),
    };
    let prompt = if who.is_empty() {
        format!("[sudo] password for {}: ", t.host())
    } else {
        format!("[sudo] password for {who}@{}: ", t.host())
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

/// Legge una riga dal terminale senza eco.
pub fn read_password(prompt: &str) -> io::Result<Password> {
    let mut tty = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .map_err(|_| io::Error::other("sudo: a password is required (no terminal to ask it)"))?;
    let old = tcgetattr(&tty).map_err(io::Error::from)?;
    let mut raw = old.clone();
    raw.local_modes.remove(LocalModes::ECHO | LocalModes::ISIG);
    tcsetattr(&tty, OptionalActions::Flush, &raw).map_err(io::Error::from)?;
    let _restore = Restore {
        tty: &tty.try_clone()?,
        old,
    };
    tty.write_all(prompt.as_bytes())?;
    tty.flush()?;
    let mut buf = Vec::with_capacity(MAX_PASSWORD + 1);
    let mut b = [0u8; 1];
    let res = loop {
        match tty.read(&mut b) {
            Ok(0) => break Err(io::Error::other("cancelled")),
            Ok(_) => match b[0] {
                b'\n' | b'\r' => break Ok(()),
                // Ctrl-C, Ctrl-D
                3 | 4 => break Err(io::Error::other("cancelled")),
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
    let _ = tty.write_all(b"\n");
    match res {
        Ok(()) => Ok(Password(Arc::new(Secret { bytes: buf }))),
        Err(e) => {
            wipe(&mut buf);
            Err(e)
        }
    }
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
        let c = |s, p| classify(s, p);
        assert_eq!(
            c("sudo: a password is required\n", false),
            Some(Verdict::NeedPassword)
        );
        assert_eq!(c("Sorry, try again.\n", true), Some(Verdict::Wrong));
        assert_eq!(
            c("sudo: 1 incorrect password attempt\n", true),
            Some(Verdict::Wrong)
        );
        assert_eq!(
            c("sudo: sorry, you must have a tty to run sudo\n", false),
            Some(Verdict::Tty)
        );
        assert_eq!(
            c("sudo: sorry, you must have a tty to run sudo\n", true),
            Some(Verdict::Tty)
        );
        assert_eq!(
            c("bob is not in the sudoers file.\n", true),
            Some(Verdict::NotAllowed)
        );
        assert_eq!(
            c("Sorry, user bob may not run sudo on host.\n", false),
            Some(Verdict::NotAllowed)
        );
        assert_eq!(c("sh: 1: sudo: not found\n", false), Some(Verdict::Missing));
        assert_eq!(c("Connection refused\n", true), None);
        // una password richiesta non è un errore nel tentativo con -S
        assert_eq!(c("sudo: a password is required\n", true), None);
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
