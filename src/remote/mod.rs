// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Monitoraggio di una macchina remota via ssh, senza agenti installati: un solo processo
//! `ssh host sh` persistente che esegue uno script POSIX (incluso nel binario) e risponde a richieste
//! a blocchi. Vedi `session.rs` (protocollo), `source.rs` (cache e manifesto) e `agent.sh`.

mod actions;
mod session;
mod source;
mod sudo;
mod target;

use std::io;
use std::sync::Arc;

pub use source::{RemoteRunner, RemoteSource};
pub use sudo::{Password, ask_password, read_password};
pub use target::Target;

use session::Transport;
use sudo::SudoMode;

#[derive(Debug, Clone, Copy)]
pub struct ConnectOptions {
    /// Compressione ssh (`-C`).
    pub compress: bool,
    /// Script sul server eseguito con sudo (`--sudo`).
    pub sudo: bool,
}

impl Default for ConnectOptions {
    fn default() -> Self {
        ConnectOptions {
            compress: true,
            sudo: false,
        }
    }
}

fn ssh_command() -> Vec<String> {
    match std::env::var("NGTOP_SSH") {
        Ok(s) if !s.trim().is_empty() => s.split_whitespace().map(str::to_string).collect(),
        _ => vec!["ssh".to_string()],
    }
}

/// Apre la sessione. Da chiamare PRIMA della TUI: ssh può chiedere password o conferma della host key
/// sul terminale. `NGTOP_SSH=local-sh` usa invece un `sh` locale (prove senza rete).
///
/// Con `o.sudo`: prima `sudo -n`; se serve la password la chiede su /dev/tty (senza eco, prompt
/// `[sudo] password for utente@host: `), la manda una sola volta e la tiene in memoria per le
/// riconnessioni (azzerata alla chiusura). Errori: `sudo: wrong password`, `sudo: a terminal is required
/// (requiretty)`, `sudo: not allowed for this user`.
pub fn connect(
    t: &Target,
    o: &ConnectOptions,
) -> io::Result<(Arc<RemoteSource>, Arc<RemoteRunner>)> {
    let cmd = ssh_command();
    if cmd.len() == 1 && cmd[0] == "local-sh" {
        return source::open(Transport::LocalSh, t.display());
    }
    let ssh = |sudo: SudoMode| Transport::Ssh {
        cmd: cmd.clone(),
        target: t.clone(),
        compress: o.compress,
        sudo,
    };
    if !o.sudo {
        return source::open(ssh(SudoMode::Off), t.display());
    }
    match source::open(ssh(SudoMode::NoPasswd), t.display()) {
        Err(e) if sudo::needs_password(&e) => {
            let pw = ask_password(t)?;
            source::open(ssh(SudoMode::Password(pw)), t.display())
        }
        r => r,
    }
}

/// Azioni sui processi della macchina raggiunta da `src`: segnali e nice con la verifica dell'avvio fatta
/// sul server nella stessa chiamata dell'azione.
pub fn actions_for(src: &Arc<RemoteSource>) -> Arc<dyn crate::source::ProcActions> {
    Arc::new(actions::RemoteActions::new(src.shared()))
}

/// Come [`connect`] ma con `sh` locale: per i test (anche di altri moduli); non fa parte dell'API.
#[doc(hidden)]
pub fn connect_local_sh() -> io::Result<(Arc<RemoteSource>, Arc<RemoteRunner>)> {
    source::open(Transport::LocalSh, "local-sh".to_string())
}

#[cfg(test)]
mod tests;
