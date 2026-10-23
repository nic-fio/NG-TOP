// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Segnali e nice sui processi del server (`ProcActions` remoto).
//!
//! Una chiamata = una richiesta all'agente. Segnale e nice rileggono l'avvio del processo (campo 22 di
//! stat) e agiscono solo se coincide con quello atteso: controllo e `kill`/`renice` stanno nella stessa
//! chiamata sul server (resta la finestra di pochi millisecondi tra i due: niente pidfd da remoto).
//! Gli interi sono controllati qui e di nuovo nello script; nella riga di richiesta entrano solo cifre
//! formattate da questo codice, mai testo.

use std::sync::Arc;
use std::time::Duration;

use super::session::Rec;
use super::source::Shared;
use crate::source::ProcActions;
use crate::source::actions::{ERR_ACCESS, ERR_GONE, ERR_PERM, ERR_REUSED};

const TIMEOUT: Duration = Duration::from_secs(10);
/// Pid massimo ammesso: pid_max di Linux è al più 2^22 = 4194304 (7 cifre), come nello script.
const PID_MAX: u32 = 4_194_304;
/// Timeout o caduta durante un'azione: forse è stata eseguita.
const LOST: &str = "connection lost: outcome unknown";
const BAD_REPLY: &str = "unexpected reply from the server";

pub(super) struct RemoteActions {
    sh: Arc<Shared>,
}

impl RemoteActions {
    pub(super) fn new(sh: Arc<Shared>) -> RemoteActions {
        RemoteActions { sh }
    }

    /// Richiesta all'agente; i record di tipo `kind` o l'errore (testo breve, senza caratteri di controllo).
    fn ask(&self, line: String, kind: u8) -> Result<Rec, String> {
        let mut req = line.into_bytes();
        req.push(b'\n');
        let (recs, _) = self
            .sh
            .request(&req, TIMEOUT, None)
            .map_err(|e| match e.kind() {
                // mai partita: esito certo
                std::io::ErrorKind::NotConnected | std::io::ErrorKind::WouldBlock => {
                    short(&e.to_string())
                }
                // la richiesta può essere arrivata (e il segnale partito): niente nuovi tentativi
                _ => LOST.to_string(),
            })?;
        recs.into_iter()
            .find(|r| r.kind == kind && r.rc == 0)
            .ok_or_else(|| BAD_REPLY.to_string())
    }
}

/// Testo breve e stampabile (può contenere stderr del server).
fn short(s: &str) -> String {
    let t: String = s
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(80)
        .collect();
    t.trim().to_string()
}

fn valid_pid(p: u32) -> bool {
    (1..=PID_MAX).contains(&p)
}

/// Righe del corpo di un record.
fn lines(r: &Rec) -> Vec<String> {
    let t = String::from_utf8_lossy(&r.body);
    t.lines().map(|l| l.trim().to_string()).collect()
}

/// Esito testuale dell'agente -> risultato.
fn outcome(w: &str) -> Result<(), String> {
    match w {
        "ok" => Ok(()),
        "gone" => Err(ERR_GONE.to_string()),
        "reused" => Err(ERR_REUSED.to_string()),
        "perm" => Err(ERR_PERM.to_string()),
        "acc" => Err(ERR_ACCESS.to_string()),
        "bad" => Err("invalid request".to_string()),
        _ => Err("the command failed on the server".to_string()),
    }
}

impl ProcActions for RemoteActions {
    fn live_starts(&self, pids: &[u32]) -> Vec<Option<u64>> {
        let mut out = vec![None; pids.len()];
        // pid non validi non partono: restano None
        let ok: Vec<(usize, u32)> = pids
            .iter()
            .copied()
            .enumerate()
            .filter(|&(_, p)| valid_pid(p))
            .collect();
        if ok.is_empty() {
            return out;
        }
        let mut line = String::from("j");
        for (_, p) in &ok {
            line.push_str(&format!(" {p}"));
        }
        let Ok(rec) = self.ask(line, b'J') else {
            return out;
        };
        let l = lines(&rec);
        if l.len() != ok.len() {
            return out;
        }
        for ((i, _), v) in ok.iter().zip(l) {
            out[*i] = v.parse::<u64>().ok();
        }
        out
    }

    fn signal(&self, targets: &[(u32, u64)], sig: i32) -> Vec<Result<(), String>> {
        let all = |m: &str| vec![Err(m.to_string()); targets.len()];
        if !(1..=64).contains(&sig) {
            return all("invalid signal");
        }
        if targets.is_empty() {
            return Vec::new();
        }
        if targets.iter().any(|&(p, _)| !valid_pid(p)) {
            return all("invalid pid");
        }
        let mut line = format!("a {sig}");
        for (p, s) in targets {
            line.push_str(&format!(" {p} {s}"));
        }
        match self.ask(line, b'O') {
            Err(e) => all(&e),
            Ok(rec) => {
                let l = lines(&rec);
                if l.len() != targets.len() {
                    return all(BAD_REPLY);
                }
                l.iter().map(|w| outcome(w)).collect()
            }
        }
    }

    fn set_nice(&self, pid: u32, expect: u64, nice: i32) -> Result<(), String> {
        if !valid_pid(pid) {
            return Err("invalid pid".to_string());
        }
        if !(-20..=19).contains(&nice) {
            return Err("invalid nice value".to_string());
        }
        let rec = self.ask(format!("r {nice} {pid} {expect}"), b'O')?;
        match lines(&rec).as_slice() {
            [w] => outcome(w),
            _ => Err(BAD_REPLY.to_string()),
        }
    }

    fn protected(&self, pid: u32) -> Option<&'static str> {
        if pid == 1 {
            Some("PID 1 (init)")
        } else if self.sh.self_pids().contains(&pid) {
            Some("the ngtop session on the server")
        } else {
            None
        }
    }
}
