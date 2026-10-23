// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Backend finto con dati fissi, per i test e per provare l'interfaccia senza systemd.

use std::sync::Mutex;

use super::{
    Action, ActionError, Backend, BootTimestamps, JournalLine, Scope, SessionInfo, UnitDetail,
    UnitInfo,
};

#[derive(Default)]
pub struct FakeBackend {
    pub units: Vec<UnitInfo>,
    pub sessions: Vec<SessionInfo>,
    pub boot: BootTimestamps,
    pub blame: Vec<(String, f64)>,
    pub journal: Vec<JournalLine>,
    /// Se `Some`, le azioni falliscono con questo errore.
    pub action_error: Option<ActionError>,
    /// Azioni ricevute (nome, azione).
    pub actions: Mutex<Vec<(String, Action)>>,
}

fn unit(name: &str, desc: &str, active: &str, sub: &str, file: &str, pid: Option<u32>) -> UnitInfo {
    UnitInfo {
        name: name.into(),
        description: desc.into(),
        load_state: "loaded".into(),
        active_state: active.into(),
        sub_state: sub.into(),
        unit_file_state: file.into(),
        main_pid: pid,
        cgroup: None,
    }
}

impl FakeBackend {
    /// Insieme di dati simile al mockup.
    pub fn demo() -> Self {
        FakeBackend {
            units: vec![
                unit(
                    "bluetooth.service",
                    "Bluetooth service",
                    "active",
                    "running",
                    "enabled",
                    Some(688),
                ),
                unit(
                    "docker.service",
                    "Docker Application Container Engine",
                    "active",
                    "running",
                    "enabled",
                    Some(1022),
                ),
                unit(
                    "docker.socket",
                    "Docker Socket",
                    "active",
                    "running",
                    "enabled",
                    None,
                ),
                unit(
                    "fwupd.service",
                    "Firmware update daemon",
                    "inactive",
                    "dead",
                    "static",
                    None,
                ),
                unit(
                    "llama-swap.service",
                    "LLM proxy (llama-swap)",
                    "failed",
                    "failed",
                    "enabled",
                    None,
                ),
                unit(
                    "snapper-timeline.timer",
                    "Timeline of Snapper Snapshots",
                    "active",
                    "waiting",
                    "enabled",
                    None,
                ),
                unit(
                    "sshd.service",
                    "OpenSSH Daemon",
                    "active",
                    "running",
                    "disabled",
                    Some(901),
                ),
                unit(
                    "update-notifier.service",
                    "Check for available updates",
                    "failed",
                    "failed",
                    "static",
                    None,
                ),
            ],
            sessions: vec![SessionInfo {
                id: "2".into(),
                user: "alex".into(),
                uid: 1000,
                seat: "seat0".into(),
                tty: "tty2".into(),
                kind: "wayland".into(),
                remote_host: String::new(),
                desktop: "KDE Plasma".into(),
                since_usec: 1_700_000_000_000_000,
            }],
            boot: BootTimestamps {
                firmware: 13_058_188,
                loader: 6_053_949,
                initrd: 0,
                userspace: 3_589_477,
                finish: 6_359_038,
                kernel_realtime: 1_700_000_000_000_000,
            },
            blame: vec![
                ("NetworkManager-wait-online.service".into(), 4.1),
                ("docker.service".into(), 1.88),
                ("snapd.seeded.service".into(), 1.27),
                ("plymouth-quit-wait.service".into(), 0.91),
            ],
            journal: (0..5)
                .map(|i| JournalLine {
                    time_usec: 1_700_000_000_000_000 + i * 1_000_000,
                    priority: if i == 3 {
                        3
                    } else if i == 4 {
                        4
                    } else {
                        6
                    },
                    message: format!("journal line {i}"),
                })
                .collect(),
            ..Default::default()
        }
    }
}

impl Backend for FakeBackend {
    fn list_units(&self, _scope: Scope) -> Result<Vec<UnitInfo>, String> {
        Ok(self.units.clone())
    }

    fn unit_detail(&self, _scope: Scope, name: &str) -> Result<UnitDetail, String> {
        let u = self
            .units
            .iter()
            .find(|u| u.name == name)
            .ok_or_else(|| format!("unit {name} not found"))?;
        Ok(UnitDetail {
            name: u.name.clone(),
            description: u.description.clone(),
            active_state: u.active_state.clone(),
            sub_state: u.sub_state.clone(),
            since_usec: Some(1_700_000_000_000_000),
            main_pid: u.main_pid,
            comm: u.main_pid.map(|_| "demo".to_string()),
            tasks: Some(22),
            tasks_max: Some(4915),
            mem_current: Some(96 << 20),
            mem_peak: Some(212 << 20),
            cpu_nsec: Some(102_000_000_000),
            requires: vec!["system.slice".into()],
            wants: vec!["network-online.target".into(), "docker.socket".into()],
            n_restarts: Some(0),
            restart: "always".into(),
            fragment_path: format!("/usr/lib/systemd/system/{name}"),
            cgroup: None,
        })
    }

    fn action(&self, _scope: Scope, name: &str, action: Action) -> Result<(), ActionError> {
        if let Ok(mut a) = self.actions.lock() {
            a.push((name.to_string(), action));
        }
        match &self.action_error {
            Some(e) => Err(e.clone()),
            None => Ok(()),
        }
    }

    fn boot_times(&self) -> Result<BootTimestamps, String> {
        Ok(self.boot)
    }

    fn sessions(&self) -> Result<Vec<SessionInfo>, String> {
        Ok(self.sessions.clone())
    }

    fn journal(&self, _scope: Scope, _name: &str, _n: usize) -> Result<Vec<JournalLine>, String> {
        Ok(self.journal.clone())
    }

    fn blame(&self) -> Result<Vec<(String, f64)>, String> {
        Ok(self.blame.clone())
    }
}
