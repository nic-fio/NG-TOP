// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Sorgente delle letture: la macchina locale oppure una remota via ssh.
//!
//! Tutti i raccoglitori leggono /proc, /sys ed /etc SOLO attraverso [`Source`]; i comandi esterni
//! (systemctl, journalctl, df…) passano da [`CommandRunner`]. Così lo stesso codice misura la macchina
//! locale ([`LocalSource`], [`LocalRunner`]) o una remota (implementazioni in `crate::remote`), e i test
//! usano alberi finti. I percorsi sono sempre assoluti nel sistema osservato (es. `/proc/1/stat`), già
//! comprensivi della radice di `Roots` quando i test usano fixture.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// Spazio di un filesystem, in byte e in inode (come statvfs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FsStat {
    pub total: u64,
    /// Spazio disponibile per un utente normale (f_bavail, la colonna "Available" di df).
    pub free: u64,
    /// Spazio occupato (f_blocks - f_bfree, la colonna "Used" di df): può essere minore di
    /// `total - free` per via dei blocchi riservati a root.
    pub used: u64,
    pub files: u64,
    pub ffree: u64,
}

/// Letture del sistema osservato. Le implementazioni non devono mai andare in panic:
/// tutto ciò che manca o non è permesso è un `Err`.
pub trait Source: Send + Sync {
    /// Contenuto intero di un file.
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;
    /// Nomi delle voci di una cartella (senza "." e "..").
    fn read_dir(&self, path: &Path) -> io::Result<Vec<OsString>>;
    /// Destinazione di un collegamento simbolico.
    fn read_link(&self, path: &Path) -> io::Result<PathBuf>;
    /// Uid del proprietario di un file o di una cartella.
    fn owner_uid(&self, path: &Path) -> io::Result<u32>;
    /// Spazio del filesystem montato in `mount`.
    fn statvfs(&self, mount: &Path) -> io::Result<FsStat>;
    /// `true` se il percorso esiste.
    fn exists(&self, path: &Path) -> bool {
        self.read_link(path).is_ok() || self.owner_uid(path).is_ok()
    }
    /// Contenuto di un file come testo (UTF-8 con sostituzione dei byte non validi).
    fn read_text(&self, path: &Path) -> Option<String> {
        self.read(path)
            .ok()
            .map(|b| String::from_utf8_lossy(&b).into_owned())
    }
    /// Chiamato una volta all'inizio di ogni aggiornamento: le sorgenti remote raccolgono qui, in
    /// un'unica andata e ritorno, i file che serviranno. Quella locale non fa nulla.
    fn begin_refresh(&self) {}
    /// Stato della connessione, solo per le sorgenti remote.
    fn remote_status(&self) -> Option<RemoteStatus> {
        None
    }
}

/// La macchina su cui gira ngtop, letta con le chiamate di sistema.
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalSource;

impl Source for LocalSource {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(path)
    }
    fn read_dir(&self, path: &Path) -> io::Result<Vec<OsString>> {
        std::fs::read_dir(path)?
            .map(|e| e.map(|e| e.file_name()))
            .collect()
    }
    fn read_link(&self, path: &Path) -> io::Result<PathBuf> {
        std::fs::read_link(path)
    }
    fn owner_uid(&self, path: &Path) -> io::Result<u32> {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(path).map(|m| m.uid())
    }
    fn statvfs(&self, mount: &Path) -> io::Result<FsStat> {
        let s = rustix::fs::statvfs(mount).map_err(io::Error::from)?;
        let unit = s.f_frsize.max(1);
        Ok(FsStat {
            total: s.f_blocks.saturating_mul(unit),
            free: s.f_bavail.saturating_mul(unit),
            used: s.f_blocks.saturating_sub(s.f_bfree).saturating_mul(unit),
            files: s.f_files,
            ffree: s.f_ffree,
        })
    }
    fn exists(&self, path: &Path) -> bool {
        std::fs::symlink_metadata(path).is_ok()
    }
}

/// Esito di un comando esterno.
#[derive(Debug, Clone, Default)]
pub struct CommandOutput {
    pub status: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Esecuzione di comandi sul sistema osservato (systemctl, journalctl, loginctl, df…).
pub trait CommandRunner: Send + Sync {
    /// Esegue `argv` (niente shell: gli argomenti passano così come sono) con un tempo massimo.
    fn run(&self, argv: &[&str], timeout: Duration) -> io::Result<CommandOutput>;
}

/// Comandi sulla macchina locale.
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalRunner;

impl CommandRunner for LocalRunner {
    fn run(&self, argv: &[&str], timeout: Duration) -> io::Result<CommandOutput> {
        let (cmd, args) = argv
            .split_first()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "empty command"))?;
        let mut child = Command::new(cmd)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;
        // stdout e stderr si svuotano in thread mentre il comando gira: con una pipe piena
        // (oltre 64 KB) il comando resterebbe fermo fino al tempo massimo
        let drain = |r: Option<Box<dyn io::Read + Send>>| {
            std::thread::spawn(move || {
                let mut buf = Vec::new();
                if let Some(mut r) = r {
                    let _ = r.read_to_end(&mut buf);
                }
                buf
            })
        };
        let out_t = drain(
            child
                .stdout
                .take()
                .map(|s| Box::new(s) as Box<dyn io::Read + Send>),
        );
        let err_t = drain(
            child
                .stderr
                .take()
                .map(|s| Box::new(s) as Box<dyn io::Read + Send>),
        );
        let start = Instant::now();
        let status = loop {
            if let Some(st) = child.try_wait()? {
                break st;
            }
            if start.elapsed() >= timeout {
                let _ = child.kill();
                let _ = child.wait();
                return Err(io::Error::new(io::ErrorKind::TimedOut, "command timed out"));
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        Ok(CommandOutput {
            status: status.code(),
            stdout: out_t.join().unwrap_or_default(),
            stderr: err_t.join().unwrap_or_default(),
        })
    }
}

/// Stato della connessione con una macchina remota, mostrato nella barra in alto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteStatus {
    /// Destinazione come l'ha scritta l'utente (es. `alex@server`).
    pub target: String,
    /// Nome della macchina remota letto da /proc/sys/kernel/hostname, se già noto.
    pub hostname: Option<String>,
    /// Tempo dell'ultima andata e ritorno, in millisecondi.
    pub latency_ms: Option<u32>,
    /// Byte ricevuti nell'ultimo aggiornamento (dopo la compressione di ssh non è misurabile: sono i byte del protocollo).
    pub last_bytes: u64,
    pub state: RemoteState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteState {
    Connected,
    /// Connessione persa, nuovo tentativo in corso; messaggio dell'ultimo errore.
    Reconnecting(String),
    /// Impossibile collegarsi (errore definitivo, es. autenticazione).
    Failed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comando_con_molto_output_non_si_blocca() {
        // 200 KB su stdout: oltre la capienza della pipe
        let t = Instant::now();
        let out = LocalRunner
            .run(
                &["sh", "-c", "head -c 200000 /dev/zero"],
                Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(out.stdout.len(), 200_000);
        assert_eq!(out.status, Some(0));
        assert!(t.elapsed() < Duration::from_secs(4));
    }

    #[test]
    fn tempo_massimo_rispettato() {
        let t = Instant::now();
        let r = LocalRunner.run(&["sleep", "5"], Duration::from_millis(200));
        assert!(r.is_err());
        assert!(t.elapsed() < Duration::from_secs(3));
    }
}
