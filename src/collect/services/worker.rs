// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Thread di lavoro: esegue le richieste (D-Bus, sottoprocessi, cgroup) fuori dal thread
//! dell'interfaccia, che comunica solo con due canali `mpsc` e non si blocca mai.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::Instant;

use crate::source::{LocalSource, Source};

use super::cgroup::CgroupSampler;
use super::{
    Action, ActionError, Backend, BootTimestamps, JournalLine, Scope, SessionInfo, UnitDetail,
    UnitInfo, UnitRow,
};

#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    Units(Scope),
    Detail(Scope, String),
    Journal(Scope, String),
    Action(Scope, String, Action),
    Boot,
    Sessions,
    Blame,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Response {
    Units(Scope, Result<Vec<UnitRow>, String>),
    Detail(Scope, String, Result<Box<UnitDetail>, String>),
    Journal(Scope, String, Result<Vec<JournalLine>, String>),
    Action(Scope, String, Action, Result<(), ActionError>),
    Boot(Result<BootTimestamps, String>),
    Sessions(Result<Vec<SessionInfo>, String>),
    Blame(Result<Vec<(String, f64)>, String>),
}

/// Lato interfaccia: invia richieste e raccoglie (senza attendere) le risposte.
pub struct Client {
    tx: Sender<Request>,
    rx: Receiver<Response>,
}

impl Client {
    /// Avvia il thread di lavoro. Il thread termina da solo quando il `Client` viene scartato.
    pub fn spawn(backend: Arc<dyn Backend>, cgroup_root: PathBuf) -> Client {
        let ncores = thread::available_parallelism().map_or(1, |n| n.get());
        Self::spawn_with(backend, cgroup_root, Arc::new(LocalSource), ncores)
    }

    /// Come `spawn`, ma memoria/CPU dal cgroup e `/proc/PID/comm` si leggono da `src`
    /// (per una macchina remota: la `Source` remota e `cgroup_root` = `/sys/fs/cgroup`).
    /// `ncores` = core logici della macchina osservata, per la CPU% (remoto: `sys.cpu.logical`).
    pub fn spawn_with(
        backend: Arc<dyn Backend>,
        cgroup_root: PathBuf,
        src: Arc<dyn Source>,
        ncores: usize,
    ) -> Client {
        let (tx, req_rx) = channel();
        let (resp_tx, rx) = channel();
        let sampler = CgroupSampler::with_source(Arc::clone(&src), cgroup_root, ncores);
        let spawned = thread::Builder::new()
            .name("ngtop-services".into())
            .spawn(move || run(backend, sampler, src, req_rx, resp_tx));
        // Se il thread non parte, le richieste restano senza risposta: l'interfaccia resta viva.
        let _ = spawned;
        Client { tx, rx }
    }

    pub fn send(&self, r: Request) {
        let _ = self.tx.send(r);
    }

    pub fn try_recv(&self) -> Option<Response> {
        self.rx.try_recv().ok()
    }
}

/// Unisce le unità con memoria e CPU% dal cgroup; scorda i cgroup non più nell'elenco.
pub(crate) fn sample_rows(
    sampler: &mut CgroupSampler,
    units: Vec<UnitInfo>,
    now: f64,
) -> Vec<UnitRow> {
    let live: std::collections::HashSet<String> =
        units.iter().filter_map(|u| u.cgroup.clone()).collect();
    let rows = units
        .into_iter()
        .map(|info| {
            let (mem, cpu) = match info.cgroup.as_deref() {
                Some(cg) if !cg.is_empty() && cg != "/" => sampler.sample(cg, now),
                _ => (None, None),
            };
            UnitRow { info, mem, cpu }
        })
        .collect();
    sampler.retain(|k| live.contains(k));
    rows
}

fn run(
    backend: Arc<dyn Backend>,
    mut sampler: CgroupSampler,
    src: Arc<dyn Source>,
    rx: Receiver<Request>,
    tx: Sender<Response>,
) {
    let t0 = Instant::now();
    let action_busy = Arc::new(AtomicBool::new(false));
    while let Ok(req) = rx.recv() {
        let resp = match req {
            Request::Units(scope) => {
                let r = backend
                    .list_units(scope)
                    .map(|units| sample_rows(&mut sampler, units, t0.elapsed().as_secs_f64()));
                Response::Units(scope, r)
            }
            Request::Detail(scope, name) => {
                let r = backend.unit_detail(scope, &name).map(|mut d| {
                    if let Some(cg) = d.cgroup.clone() {
                        d.mem_current = d.mem_current.or_else(|| sampler.mem_current(&cg));
                        d.mem_peak = d.mem_peak.or_else(|| sampler.mem_peak(&cg));
                    }
                    if let Some(pid) = d.main_pid.filter(|&p| p > 0) {
                        d.comm = src
                            .read_text(std::path::Path::new(&format!("/proc/{pid}/comm")))
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty());
                    }
                    Box::new(d)
                });
                Response::Detail(scope, name, r)
            }
            // Sottoprocessi: in un thread a parte, così non fermano le altre richieste.
            Request::Journal(scope, name) => {
                let (b, tx) = (Arc::clone(&backend), tx.clone());
                let _ = thread::Builder::new()
                    .name("ngtop-journal".into())
                    .spawn(move || {
                        let r = b.journal(scope, &name, 50);
                        let _ = tx.send(Response::Journal(scope, name, r));
                    });
                continue;
            }
            Request::Blame => {
                let (b, tx) = (Arc::clone(&backend), tx.clone());
                let _ = thread::Builder::new()
                    .name("ngtop-blame".into())
                    .spawn(move || {
                        let _ = tx.send(Response::Blame(b.blame()));
                    });
                continue;
            }
            // Può attendere la password (polkit) fino a 120 s: thread a parte, una sola alla volta.
            Request::Action(scope, name, action) => {
                if action_busy.swap(true, Ordering::AcqRel) {
                    let e = ActionError::Failed("another action is in progress".to_string());
                    Response::Action(scope, name, action, Err(e))
                } else {
                    let (b, tx, busy) =
                        (Arc::clone(&backend), tx.clone(), Arc::clone(&action_busy));
                    let name_err = name.clone();
                    let spawned =
                        thread::Builder::new()
                            .name("ngtop-action".into())
                            .spawn(move || {
                                let r = b.action(scope, &name, action);
                                busy.store(false, Ordering::Release);
                                let _ = tx.send(Response::Action(scope, name, action, r));
                            });
                    if spawned.is_err() {
                        action_busy.store(false, Ordering::Release);
                        let e = ActionError::Failed("thread not started".to_string());
                        Response::Action(scope, name_err, action, Err(e))
                    } else {
                        continue;
                    }
                }
            }
            Request::Boot => Response::Boot(backend.boot_times()),
            Request::Sessions => Response::Sessions(backend.sessions()),
        };
        if tx.send(resp).is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn il_campionatore_non_cresce_senza_limite() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cgroup/a");
        let mut sampler = CgroupSampler::new(root, 4);
        let unit = |cg: &str| UnitInfo {
            name: format!("{cg}.service"),
            cgroup: Some(cg.to_string()),
            ..Default::default()
        };
        let rows = sample_rows(
            &mut sampler,
            vec![unit("/system.slice/demo.service"), unit("/finto/uno")],
            1.0,
        );
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].mem, Some(104_857_600));
        assert_eq!(sampler.tracked(), 1);
        // Il cgroup sparisce dall'elenco: la sua lettura precedente viene dimenticata.
        sample_rows(&mut sampler, vec![unit("/altro")], 2.0);
        assert_eq!(sampler.tracked(), 0);
    }
}

#[cfg(test)]
mod tests_azioni {
    use super::*;
    use crate::collect::services::fake::FakeBackend;
    use std::time::Duration;

    struct Lenta(FakeBackend);

    impl Backend for Lenta {
        fn list_units(&self, s: Scope) -> Result<Vec<UnitInfo>, String> {
            self.0.list_units(s)
        }
        fn unit_detail(&self, s: Scope, n: &str) -> Result<UnitDetail, String> {
            self.0.unit_detail(s, n)
        }
        fn action(&self, _: Scope, _: &str, _: Action) -> Result<(), ActionError> {
            thread::sleep(Duration::from_millis(400));
            Ok(())
        }
        fn boot_times(&self) -> Result<BootTimestamps, String> {
            self.0.boot_times()
        }
        fn sessions(&self) -> Result<Vec<SessionInfo>, String> {
            self.0.sessions()
        }
        fn journal(&self, s: Scope, n: &str, k: usize) -> Result<Vec<JournalLine>, String> {
            self.0.journal(s, n, k)
        }
        fn blame(&self) -> Result<Vec<(String, f64)>, String> {
            self.0.blame()
        }
    }

    #[test]
    fn un_azione_lenta_non_blocca_la_coda_e_ce_ne_e_una_sola() {
        let c = Client::spawn(
            Arc::new(Lenta(FakeBackend::demo())),
            PathBuf::from("/nonexistent"),
        );
        c.send(Request::Action(
            Scope::System,
            "a.service".into(),
            Action::Start,
        ));
        c.send(Request::Action(
            Scope::System,
            "b.service".into(),
            Action::Stop,
        ));
        c.send(Request::Sessions);
        let t = Instant::now();
        let mut got = Vec::new();
        while got.len() < 3 && t.elapsed() < Duration::from_secs(3) {
            if let Some(r) = c.try_recv() {
                got.push((t.elapsed(), r));
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(got.len(), 3);
        // Le Sessions e il rifiuto della seconda azione arrivano subito, prima dell'esito.
        assert!(matches!(
            got[0].1,
            Response::Action(_, _, _, Err(ActionError::Failed(_)))
        ));
        assert!(matches!(got[1].1, Response::Sessions(_)));
        assert!(got[1].0 < Duration::from_millis(300));
        assert!(matches!(got[2].1, Response::Action(_, _, _, Ok(()))));
    }
}
