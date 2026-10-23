// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Monitoraggio di una macchina remota via ssh, senza agenti installati: un solo processo
//! `ssh host sh` persistente che esegue uno script POSIX (incluso nel binario) e risponde a richieste
//! a blocchi. Vedi `session.rs` (protocollo), `source.rs` (cache e manifesto) e `agent.sh`.

mod session;
mod source;
mod target;

use std::io;
use std::sync::Arc;

pub use source::{RemoteRunner, RemoteSource};
pub use target::Target;

use session::Transport;

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
pub fn connect(
    t: &Target,
    o: &ConnectOptions,
) -> io::Result<(Arc<RemoteSource>, Arc<RemoteRunner>)> {
    let cmd = ssh_command();
    let tr = if cmd.len() == 1 && cmd[0] == "local-sh" {
        Transport::LocalSh
    } else {
        Transport::Ssh {
            cmd,
            target: t.clone(),
            compress: o.compress,
        }
    };
    source::open(tr, t.display())
}

/// Azioni sui processi della macchina raggiunta da `src` (segnali e nice con verifica dell'avvio sul
/// server). Provvisorio: rifiuta tutto finché il trasporto non le implementa.
pub fn actions_for(src: &Arc<RemoteSource>) -> Arc<dyn crate::source::ProcActions> {
    let _ = src;
    Arc::new(crate::source::NoActions)
}

/// Come [`connect`] ma con `sh` locale: per i test (anche di altri moduli); non fa parte dell'API.
#[doc(hidden)]
pub fn connect_local_sh() -> io::Result<(Arc<RemoteSource>, Arc<RemoteRunner>)> {
    source::open(Transport::LocalSh, "local-sh".to_string())
}

#[cfg(test)]
mod tests;
