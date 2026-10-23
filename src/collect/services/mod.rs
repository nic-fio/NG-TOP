// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Servizi systemd: modello dei dati, "backend" (D-Bus reale o finto nei test), parsing di
//! journalctl / systemd-analyze, fasi di avvio e lettura di memoria/CPU dal cgroup.
//! Tutto ciò che è lento (D-Bus, sottoprocessi) gira nel thread di lavoro (`worker`).

pub mod cgroup;
pub mod dbus;
pub mod fake;
pub mod remote;
pub mod worker;

use std::path::Path;
use std::sync::Arc;

use crate::source::{CommandRunner, Source};

/// Bus a cui si parla: di sistema oppure di sessione (unità dell'utente).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    System,
    User,
}

/// Una unità dell'elenco (da `ListUnits` + `UnitFileState`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UnitInfo {
    pub name: String,
    pub description: String,
    pub load_state: String,
    pub active_state: String,
    pub sub_state: String,
    /// enabled, disabled, static, masked…; vuoto se sconosciuto.
    pub unit_file_state: String,
    pub main_pid: Option<u32>,
    /// Percorso del cgroup (es. `/system.slice/docker.service`).
    pub cgroup: Option<String>,
}

/// Una riga della tabella: unità + memoria e CPU% letti dal cgroup.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UnitRow {
    pub info: UnitInfo,
    pub mem: Option<u64>,
    pub cpu: Option<f64>,
}

/// Dettaglio dell'unità selezionata.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UnitDetail {
    pub name: String,
    pub description: String,
    pub active_state: String,
    pub sub_state: String,
    /// Istante (µs Unix) in cui l'unità è entrata nello stato attuale.
    pub since_usec: Option<u64>,
    pub main_pid: Option<u32>,
    /// Nome del processo principale (da /proc/PID/comm).
    pub comm: Option<String>,
    pub tasks: Option<u64>,
    pub tasks_max: Option<u64>,
    pub mem_current: Option<u64>,
    pub mem_peak: Option<u64>,
    pub cpu_nsec: Option<u64>,
    pub requires: Vec<String>,
    pub wants: Vec<String>,
    pub n_restarts: Option<u32>,
    pub restart: String,
    pub fragment_path: String,
    pub cgroup: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct JournalLine {
    /// µs Unix.
    pub time_usec: u64,
    /// 0..7 (syslog); 6 se manca.
    pub priority: u8,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SessionInfo {
    pub id: String,
    pub user: String,
    pub uid: u32,
    pub seat: String,
    pub tty: String,
    /// tty, x11, wayland, mir, unspecified.
    pub kind: String,
    pub remote_host: String,
    pub desktop: String,
    /// µs Unix.
    pub since_usec: u64,
}

/// Proprietà *Timestamp*Monotonic del Manager (µs), come le legge `systemd-analyze time`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BootTimestamps {
    pub firmware: u64,
    pub loader: u64,
    pub initrd: u64,
    pub userspace: u64,
    pub finish: u64,
    /// KernelTimestamp (µs Unix): quando è partito il kernel.
    pub kernel_realtime: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Firmware,
    Loader,
    Kernel,
    Initrd,
    Servizi,
}

impl Phase {
    pub fn label(self) -> &'static str {
        match self {
            Phase::Firmware => "firmware",
            Phase::Loader => "loader",
            Phase::Kernel => "kernel",
            Phase::Initrd => "initrd",
            Phase::Servizi => "services",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Start,
    Stop,
    Restart,
    Enable,
    Disable,
}

impl Action {
    pub fn verb(self) -> &'static str {
        match self {
            Action::Start => "Start",
            Action::Stop => "Stop",
            Action::Restart => "Restart",
            Action::Enable => "Enable",
            Action::Disable => "Disable",
        }
    }

    /// Participio per la riga di stato ("Start requested …").
    pub fn done(self) -> &'static str {
        match self {
            Action::Start => "Start requested",
            Action::Stop => "Stop requested",
            Action::Restart => "Restart requested",
            Action::Enable => "Enabled",
            Action::Disable => "Disabled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionError {
    /// Permessi insufficienti (polkit non ha autorizzato).
    Denied,
    Failed(String),
}

pub const DENIED_TEXT: &str = "Permission denied: a polkit agent is needed, or run ngtop with sudo";

/// Classifica il testo di un errore D-Bus: i rifiuti di autorizzazione diventano `Denied`.
pub fn classify_error(text: &str) -> ActionError {
    let l = text.to_lowercase();
    if l.contains("accessdenied")
        || l.contains("access denied")
        || l.contains("interactiveauthorizationrequired")
        || l.contains("interactive authentication required")
        || l.contains("not authorized")
        || l.contains("permission denied")
    {
        ActionError::Denied
    } else {
        ActionError::Failed(text.to_string())
    }
}

/// Accesso a systemd. Le implementazioni possono bloccare: si usano solo dal thread di lavoro.
pub trait Backend: Send + Sync {
    fn list_units(&self, scope: Scope) -> Result<Vec<UnitInfo>, String>;
    fn unit_detail(&self, scope: Scope, name: &str) -> Result<UnitDetail, String>;
    fn action(&self, scope: Scope, name: &str, action: Action) -> Result<(), ActionError>;
    fn boot_times(&self) -> Result<BootTimestamps, String>;
    fn sessions(&self) -> Result<Vec<SessionInfo>, String>;
    /// Ultime `n` righe del registro dell'unità.
    fn journal(&self, scope: Scope, name: &str, n: usize) -> Result<Vec<JournalLine>, String>;
    /// Servizi più lenti all'avvio: (unità, secondi), dal più lento.
    fn blame(&self) -> Result<Vec<(String, f64)>, String>;
}

/// `true` se il sistema è avviato con systemd.
pub fn systemd_present(run_dir: &Path) -> bool {
    run_dir.join("systemd/system").is_dir()
}

/// Come [`systemd_present`], ma letto attraverso la `Source` (anche remota).
pub fn systemd_present_in(src: &dyn Source, run_dir: &Path) -> bool {
    src.exists(&run_dir.join("systemd/system"))
}

/// Sceglie il backend: D-Bus sulla macchina locale, `RemoteBackend` (solo comandi) se la
/// `Source` è remota (`remote_status()` è `Some`).
pub fn backend_for(src: &Arc<dyn Source>, run: &Arc<dyn CommandRunner>) -> Arc<dyn Backend> {
    if src.remote_status().is_some() {
        Arc::new(remote::RemoteBackend::new(Arc::clone(src), Arc::clone(run)))
    } else {
        Arc::new(dbus::DbusBackend::new())
    }
}

/// Fasi di avvio con la loro durata in secondi (come `systemd-analyze time`).
/// `None` se mancano i dati di userspace (container, avvio non terminato).
pub fn boot_phases(t: &BootTimestamps) -> Option<Vec<(Phase, f64)>> {
    if t.userspace == 0 {
        return None;
    }
    let us = |v: u64| v as f64 / 1e6;
    let mut out = Vec::new();
    // Firmware e loader sono "µs prima dell'avvio del kernel".
    if t.firmware > 0 && t.loader > 0 && t.firmware >= t.loader {
        out.push((Phase::Firmware, us(t.firmware - t.loader)));
        out.push((Phase::Loader, us(t.loader)));
    } else if t.loader > 0 {
        out.push((Phase::Loader, us(t.loader)));
    }
    let kernel_end = if t.initrd > 0 { t.initrd } else { t.userspace };
    out.push((Phase::Kernel, us(kernel_end)));
    if t.initrd > 0 && t.userspace > t.initrd {
        out.push((Phase::Initrd, us(t.userspace - t.initrd)));
    }
    if t.finish > t.userspace {
        out.push((Phase::Servizi, us(t.finish - t.userspace)));
    }
    Some(out)
}

/// Righe di `journalctl -o json` (una per riga) → righe del registro, dalla più vecchia.
/// Le righe non valide vengono ignorate; MESSAGE binario (array di byte) è decodificato.
pub fn parse_journal_json(text: &str) -> Vec<JournalLine> {
    let mut out: Vec<JournalLine> = text
        .lines()
        .filter_map(|l| {
            let v: serde_json::Value = serde_json::from_str(l.trim()).ok()?;
            let time_usec = v
                .get("__REALTIME_TIMESTAMP")
                .and_then(|t| t.as_str().and_then(|s| s.parse().ok()).or(t.as_u64()))
                .unwrap_or(0);
            let priority = v
                .get("PRIORITY")
                .and_then(|p| {
                    p.as_str()
                        .and_then(|s| s.parse::<u8>().ok())
                        .or(p.as_u64().map(|n| n as u8))
                })
                .unwrap_or(6)
                .min(7);
            let message = match v.get("MESSAGE") {
                Some(serde_json::Value::String(s)) => s.clone(),
                Some(serde_json::Value::Array(a)) => {
                    let bytes: Vec<u8> = a
                        .iter()
                        .filter_map(|x| x.as_u64().map(|n| n as u8))
                        .collect();
                    String::from_utf8_lossy(&bytes).into_owned()
                }
                _ => String::new(),
            };
            Some(JournalLine {
                time_usec,
                priority,
                message: message.replace(['\n', '\r', '\t'], " "),
            })
        })
        .collect();
    out.sort_by_key(|l| l.time_usec);
    out
}

/// Durata di `systemd-analyze` ("1min 2.345s", "696ms", "1h 3min") in secondi.
fn parse_span_token(tok: &str) -> Option<f64> {
    let pos = tok.find(|c: char| c.is_ascii_alphabetic())?;
    let (num, unit) = tok.split_at(pos);
    let n: f64 = num.parse().ok()?;
    let mult = match unit {
        "us" | "μs" => 1e-6,
        "ms" => 1e-3,
        "s" => 1.0,
        "min" => 60.0,
        "h" => 3600.0,
        "d" => 86400.0,
        _ => return None,
    };
    Some(n * mult)
}

/// Output di `systemd-analyze blame` → (unità, secondi), nell'ordine del programma.
pub fn parse_blame(text: &str) -> Vec<(String, f64)> {
    text.lines()
        .filter_map(|l| {
            let toks: Vec<&str> = l.split_whitespace().collect();
            let (name, spans) = toks.split_last()?;
            if spans.is_empty() {
                return None;
            }
            let mut total = 0.0;
            for t in spans {
                total += parse_span_token(t)?;
            }
            Some((name.to_string(), total))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fasi_con_firmware_e_loader() {
        // Valori reali di questo PC: niente initrd.
        let t = BootTimestamps {
            firmware: 13_058_188,
            loader: 6_053_949,
            initrd: 0,
            userspace: 3_589_477,
            finish: 6_359_038,
            kernel_realtime: 0,
        };
        let p = boot_phases(&t).unwrap();
        let labels: Vec<_> = p.iter().map(|(f, _)| f.label()).collect();
        assert_eq!(labels, ["firmware", "loader", "kernel", "services"]);
        assert!((p[0].1 - 7.004).abs() < 0.001);
        assert!((p[1].1 - 6.054).abs() < 0.001);
        assert!((p[2].1 - 3.589).abs() < 0.001);
        assert!((p[3].1 - 2.769).abs() < 0.001);
    }

    #[test]
    fn fasi_con_initrd_e_senza_dati() {
        let t = BootTimestamps {
            initrd: 2_300_000,
            userspace: 4_200_000,
            finish: 10_600_000,
            ..Default::default()
        };
        let p = boot_phases(&t).unwrap();
        assert_eq!(p.len(), 3);
        assert_eq!(p[0].0, Phase::Kernel);
        assert!((p[0].1 - 2.3).abs() < 1e-9);
        assert_eq!(p[1].0, Phase::Initrd);
        assert!((p[1].1 - 1.9).abs() < 1e-9);
        assert!((p[2].1 - 6.4).abs() < 1e-9);
        assert!(boot_phases(&BootTimestamps::default()).is_none());
        // Avvio non ancora finito: manca la fase servizi.
        let t = BootTimestamps {
            userspace: 1_000_000,
            ..Default::default()
        };
        assert_eq!(boot_phases(&t).unwrap().len(), 1);
    }

    #[test]
    fn journal_json() {
        let txt = concat!(
            "{\"__REALTIME_TIMESTAMP\":\"1700000002000000\",\"PRIORITY\":\"3\",\"MESSAGE\":\"errore\"}\n",
            "{\"__REALTIME_TIMESTAMP\":\"1700000001000000\",\"MESSAGE\":[104,105]}\n",
            "non è json\n",
            "{\"__REALTIME_TIMESTAMP\":\"1700000003000000\",\"PRIORITY\":\"4\",\"MESSAGE\":\"a\\nb\"}\n",
        );
        let l = parse_journal_json(txt);
        assert_eq!(l.len(), 3);
        assert_eq!(l[0].message, "hi");
        assert_eq!(l[0].priority, 6);
        assert_eq!(l[0].time_usec, 1_700_000_001_000_000);
        assert_eq!(l[1].priority, 3);
        assert_eq!(l[2].message, "a b");
        assert!(parse_journal_json("").is_empty());
    }

    #[test]
    fn blame() {
        let txt = "4.100s NetworkManager-wait-online.service\n1min 2.5s slow.service\n696ms apt.service\nrumore\n";
        let b = parse_blame(txt);
        assert_eq!(b.len(), 3);
        assert_eq!(b[0].0, "NetworkManager-wait-online.service");
        assert!((b[0].1 - 4.1).abs() < 1e-9);
        assert!((b[1].1 - 62.5).abs() < 1e-9);
        assert!((b[2].1 - 0.696).abs() < 1e-9);
    }

    #[test]
    fn errori_permessi() {
        assert_eq!(
            classify_error("org.freedesktop.DBus.Error.AccessDenied: Access denied"),
            ActionError::Denied
        );
        assert_eq!(
            classify_error("Interactive authentication required."),
            ActionError::Denied
        );
        assert!(matches!(
            classify_error("Unit foo.service not found"),
            ActionError::Failed(_)
        ));
    }

    #[test]
    fn senza_systemd() {
        let d = std::env::temp_dir().join("ngtop-nosystemd-test");
        let _ = std::fs::create_dir_all(&d);
        assert!(!systemd_present(&d));
        let _ = std::fs::create_dir_all(d.join("systemd/system"));
        assert!(systemd_present(&d));
        let _ = std::fs::remove_dir_all(&d);
    }
}
