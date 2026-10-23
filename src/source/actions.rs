// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Azioni sui processi del sistema osservato: segnali e nice.
//!
//! Ogni azione porta con sé l'avvio atteso del processo (starttime di stat, in tick): si agisce solo
//! se il processo vivo con quel pid ha ancora lo stesso avvio, così un pid riusato non viene mai
//! colpito. [`LocalActions`] agisce su questa macchina; quella remota è in `crate::remote`.

use std::path::{Path, PathBuf};

use rustix::io::Errno;
use rustix::process::{
    Pid, PidfdFlags, Signal, kill_process, pidfd_open, pidfd_send_signal, setpriority_process,
};

/// Testi d'errore comuni: le implementazioni remote usano gli stessi, l'interfaccia li mostra così.
pub const ERR_GONE: &str = "the process no longer exists (ESRCH)";
pub const ERR_REUSED: &str = "the pid now belongs to another process (reused)";
pub const ERR_PERM: &str = "permission denied (EPERM)";
pub const ERR_ACCESS: &str = "access denied (EACCES)";

/// Segnali e nice sul sistema osservato. Mai panic: ogni problema è un `Err` con un testo breve in
/// inglese (preferibilmente una delle costanti `ERR_*`).
pub trait ProcActions: Send + Sync {
    /// Avvio (tick) dei processi vivi, nello stesso ordine di `pids`; `None` = non esiste più.
    fn live_starts(&self, pids: &[u32]) -> Vec<Option<u64>>;
    /// Invia il segnale `sig` (numero, es. 15) a ogni `(pid, avvio atteso)`, solo se l'avvio coincide
    /// ancora ([`ERR_REUSED`] altrimenti). Un esito per bersaglio, nello stesso ordine.
    fn signal(&self, targets: &[(u32, u64)], sig: i32) -> Vec<Result<(), String>>;
    /// Imposta il nice del thread principale di `pid` (come htop), solo se l'avvio è ancora `expect`.
    fn set_nice(&self, pid: u32, expect: u64, nice: i32) -> Result<(), String>;
    /// Perché `pid` non va colpito alla leggera (PID 1, ngtop stesso o la sua sessione sul server).
    fn protected(&self, pid: u32) -> Option<&'static str>;
}

/// Azioni sulla macchina locale; `proc` è la radice di /proc (fixture nei test).
#[derive(Debug, Clone)]
pub struct LocalActions {
    pub proc: PathBuf,
}

impl LocalActions {
    pub fn new(proc: PathBuf) -> LocalActions {
        LocalActions { proc }
    }
}

impl ProcActions for LocalActions {
    fn live_starts(&self, pids: &[u32]) -> Vec<Option<u64>> {
        pids.iter().map(|&p| live_start(&self.proc, p)).collect()
    }
    fn signal(&self, targets: &[(u32, u64)], sig: i32) -> Vec<Result<(), String>> {
        targets
            .iter()
            .map(|&(pid, e)| send_signal(&self.proc, pid, e, sig))
            .collect()
    }
    fn set_nice(&self, pid: u32, expect: u64, nice: i32) -> Result<(), String> {
        if live_start(&self.proc, pid) != Some(expect) {
            return Err(ERR_REUSED.to_string());
        }
        set_nice(pid, nice)
    }
    fn protected(&self, pid: u32) -> Option<&'static str> {
        protected(pid)
    }
}

pub fn errno_text(e: Errno) -> String {
    match e {
        Errno::PERM => ERR_PERM.to_string(),
        Errno::ACCESS => ERR_ACCESS.to_string(),
        Errno::SRCH => ERR_GONE.to_string(),
        other => format!("error {}", other.raw_os_error()),
    }
}

pub fn to_pid(pid: u32) -> Option<Pid> {
    i32::try_from(pid).ok().and_then(Pid::from_raw)
}

/// Invio reale con rustix (nessuna libc C). Ordine senza finestre: pidfd_open → si rilegge
/// l'avvio da stat e lo si confronta con quello atteso → pidfd_send_signal (il pidfd resta
/// legato a quel processo anche se il pid viene riusato). Senza pidfd (kernel < 5.3 o permessi):
/// verifica + kill.
pub fn send_signal(proc_root: &Path, pid: u32, expect: u64, sig: i32) -> Result<(), String> {
    let p = to_pid(pid).ok_or("invalid pid")?;
    let s = Signal::from_named_raw(sig).ok_or("invalid signal")?;
    let changed = || Err(ERR_REUSED.to_string());
    match pidfd_open(p, PidfdFlags::empty()) {
        Ok(fd) => {
            if live_start(proc_root, pid) != Some(expect) {
                return changed();
            }
            match pidfd_send_signal(&fd, s) {
                Ok(()) => Ok(()),
                Err(Errno::SRCH) => Err(errno_text(Errno::SRCH)),
                Err(_) => kill_process(p, s).map_err(errno_text),
            }
        }
        Err(Errno::SRCH) => Err(errno_text(Errno::SRCH)),
        Err(_) => {
            if live_start(proc_root, pid) != Some(expect) {
                return changed();
            }
            kill_process(p, s).map_err(errno_text)
        }
    }
}

/// Avvio (starttime di stat, in tick) del processo vivo: serve a riconoscere un pid riusato.
pub fn live_start(proc_root: &Path, pid: u32) -> Option<u64> {
    std::fs::read_to_string(proc_root.join(pid.to_string()).join("stat"))
        .ok()
        .and_then(|t| crate::collect::process::parse_stat(&t))
        .map(|st| st.starttime)
}

/// Imposta il nice di un processo (senza verifica dell'avvio: la fa `ProcActions::set_nice`).
pub fn set_nice(pid: u32, nice: i32) -> Result<(), String> {
    let p = to_pid(pid).ok_or("invalid pid")?;
    setpriority_process(Some(p), nice).map_err(errno_text)
}

/// Motivo per cui un pid locale non va colpito alla leggera (init, ngtop stesso).
pub fn protected(pid: u32) -> Option<&'static str> {
    if pid == 1 {
        Some("PID 1 (init)")
    } else if pid == std::process::id() {
        Some("ngtop itself")
    } else {
        None
    }
}

/// Usata finché una sorgente remota non fornisce le sue azioni: rifiuta tutto, non tocca mai la
/// macchina locale.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoActions;

/// Testo di [`NoActions`].
pub const NO_ACTIONS_MSG: &str = "Not available on this machine";

impl ProcActions for NoActions {
    fn live_starts(&self, pids: &[u32]) -> Vec<Option<u64>> {
        vec![None; pids.len()]
    }
    fn signal(&self, targets: &[(u32, u64)], _: i32) -> Vec<Result<(), String>> {
        vec![Err(NO_ACTIONS_MSG.to_string()); targets.len()]
    }
    fn set_nice(&self, _: u32, _: u64, _: i32) -> Result<(), String> {
        Err(NO_ACTIONS_MSG.to_string())
    }
    fn protected(&self, pid: u32) -> Option<&'static str> {
        (pid == 1).then_some("PID 1 (init)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pid_riusato_non_colpito() {
        // processo vero, avvio atteso sbagliato: nessun segnale
        let mut c = std::process::Command::new("sleep")
            .arg("5")
            .spawn()
            .unwrap();
        let a = LocalActions::new("/proc".into());
        let pid = c.id();
        let real = a.live_starts(&[pid])[0].unwrap();
        assert_eq!(
            a.signal(&[(pid, real + 1)], 15),
            vec![Err(ERR_REUSED.to_string())]
        );
        assert_eq!(a.set_nice(pid, real + 1, 5), Err(ERR_REUSED.to_string()));
        assert!(c.try_wait().unwrap().is_none());
        assert_eq!(a.set_nice(pid, real, 5), Ok(()));
        assert_eq!(a.signal(&[(pid, real)], 15), vec![Ok(())]);
        let _ = c.wait();
    }

    #[test]
    fn nessuna_azione_senza_backend() {
        assert!(NoActions.signal(&[(1234, 1)], 15)[0].is_err());
        assert!(NoActions.set_nice(1234, 1, 5).is_err());
        assert_eq!(NoActions.live_starts(&[1, 2]), vec![None, None]);
    }
}
