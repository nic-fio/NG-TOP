// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! NVIDIA: thread che lancia `nvidia-smi` (query, `pmon`, `--query-compute-apps`) tramite `Roots.run`,
//! quindi in locale e in remoto. Il raccoglitore non aspetta mai: prende l'ultimo risultato pronto.
//! Il thread non lancia nulla finché il raccoglitore non dice che c'è una scheda sveglia, e ogni
//! comando passa da una guardia che rilegge `runtime_status` sulla macchina osservata subito prima
//! di lanciarlo (non si sveglia mai la GPU). Termina da solo quando il `NvClient` viene scartato.

use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::{Duration, Instant};

use super::gpu::is_suspended;
use super::nvidia::{
    self, FIELDS_BASE, FIELDS_FULL, MAX_OUTPUT, NvGpu, NvProc, compute_apps_args, pmon_args,
    query_args,
};
use crate::source::{CommandOutput, CommandRunner, Source};

/// Pausa tra due giri riusciti.
const POLL: Duration = Duration::from_secs(2);
/// Tempo massimo di ogni comando.
const CMD_TIMEOUT: Duration = Duration::from_secs(3);
/// Ricontrollo di `nvidia-smi` mancante.
const NOTOOL_RECHECK: Duration = Duration::from_secs(60);
/// Attesa dopo un errore: parte da `POLL`, raddoppia fino a qui.
const BACKOFF_MAX: Duration = Duration::from_secs(30);
/// Risultati più vecchi di così non si usano.
pub const MAX_AGE: Duration = Duration::from_secs(10);
/// Passo con cui il thread controlla se deve uscire o riprendere.
const SLICE: Duration = Duration::from_millis(50);
/// Ogni quanti giri si riprova `pmon` dopo che non era supportato.
const PMON_RETRY_EVERY: u32 = 30;
/// Uscita della guardia quando una scheda dorme (fuori dai codici di nvidia-smi e di timeout(1)).
const GUARD_ASLEEP: i32 = 97;
/// Guardia eseguita sulla macchina osservata (locale o remota) subito prima di `nvidia-smi`:
/// argomenti `N`, N file `runtime_status`, poi il comando. Se una scheda dorme o sta per dormire,
/// o il suo stato non si legge, esce con `GUARD_ASLEEP` senza lanciare nulla.
const GUARD: &str = "n=$1; shift; while [ \"$n\" -gt 0 ]; do s=; \
read -r s < \"$1\" || [ -n \"$s\" ] || exit 97; \
case $s in suspended|suspending) exit 97 ;; esac; shift; n=$((n - 1)); done; exec \"$@\"";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvStatus {
    Ok,
    /// `nvidia-smi` non esiste.
    NoTool,
    /// Timeout, errore o uscita inutilizzabile.
    NotResponding,
}

/// Esito di un giro del thread.
#[derive(Debug, Clone)]
pub struct NvResult {
    pub status: NvStatus,
    pub gpus: Vec<NvGpu>,
    pub procs: Vec<NvProc>,
    /// I processi vengono da `--query-compute-apps` (pmon non supportato).
    pub compute_only: bool,
    pub at: Instant,
}

/// Scheda nvidia che il thread può interrogare: indirizzo PCI (forma di sysfs) e cartella `device` in sysfs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvTarget {
    pub bus_id: String,
    pub dev: PathBuf,
}

/// Elenco condiviso delle schede sveglie (vuoto = il thread non lancia nulla). Il numero di riferimenti
/// serve anche a sapere se il raccoglitore è ancora vivo.
type Targets = Arc<Mutex<Vec<NvTarget>>>;

/// Lato raccoglitore del thread nvidia.
pub struct NvClient {
    rx: Receiver<NvResult>,
    targets: Targets,
    last: Option<NvResult>,
}

impl NvClient {
    /// Avvia il thread (in attesa finché `set_targets` non gli dà almeno una scheda sveglia).
    /// `src` serve a rileggere lo stato di alimentazione subito prima di ogni lancio.
    pub fn spawn(run: Arc<dyn CommandRunner>, src: Arc<dyn Source>) -> NvClient {
        let (tx, rx) = channel();
        let targets: Targets = Arc::new(Mutex::new(Vec::new()));
        let w = targets.clone();
        // se il thread non parte, non arriveranno mai risultati: stato "in attesa" per sempre
        let _ = thread::Builder::new()
            .name("ngtop-nvidia".into())
            .spawn(move || worker(run, src, tx, w));
        NvClient {
            rx,
            targets,
            last: None,
        }
    }

    /// Schede nvidia sveglie: il thread interroga solo queste (`-i`) e solo se restano sveglie.
    pub fn set_targets(&mut self, t: Vec<NvTarget>) {
        let none = t.is_empty();
        *self.targets.lock().unwrap_or_else(|e| e.into_inner()) = t;
        if none {
            while self.rx.try_recv().is_ok() {}
            self.last = None;
        }
    }

    /// Ultimo risultato pronto (senza attendere). `None` = ancora nessuna risposta.
    pub fn latest(&mut self) -> Option<&NvResult> {
        while let Ok(r) = self.rx.try_recv() {
            self.last = Some(r);
        }
        self.last.as_ref()
    }
}

/// Dorme `d` a pezzi; `false` se il raccoglitore è stato scartato.
fn nap(d: Duration, wanted: &Targets) -> bool {
    let end = Instant::now() + d;
    loop {
        if Arc::strong_count(wanted) == 1 {
            return false;
        }
        let left = end.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return true;
        }
        thread::sleep(left.min(SLICE));
    }
}

/// Esito di un comando: testo di stdout (limitato) e stderr uniti, oppure l'errore.
enum Run {
    Done {
        code: Option<i32>,
        text: String,
    },
    NotFound,
    Failed,
    /// La guardia ha trovato una scheda addormentata: nulla è stato lanciato.
    Asleep,
}

fn lossy(b: &[u8]) -> String {
    String::from_utf8_lossy(&b[..b.len().min(MAX_OUTPUT)]).into_owned()
}

/// Contesto di un giro: da dove si rilegge lo stato delle schede e quali schede sono richieste.
struct Ctx<'a> {
    run: &'a dyn CommandRunner,
    src: &'a dyn Source,
    targets: &'a Targets,
}

impl Ctx<'_> {
    /// Schede richieste che risultano sveglie da `src` (in remoto: dalla cache dell'ultimo
    /// aggiornamento). Nel dubbio (stato illeggibile) la scheda non conta come sveglia.
    fn awake(&self) -> Vec<NvTarget> {
        let t = self
            .targets
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        t.into_iter()
            .filter(|x| {
                self.src
                    .read_text(&x.dev.join("power/runtime_status"))
                    .is_some()
                    && !is_suspended(self.src, &x.dev)
            })
            .collect()
    }

    /// Lancia `nvidia-smi <args> -i <schede sveglie>` dietro la guardia; `None` se in questo
    /// momento nessuna è sveglia. La guardia rilegge lo stato sulla macchina osservata nella stessa
    /// shell che poi lancia il comando: la finestra residua è il tempo tra `read` ed `exec`.
    fn exec(&self, args: &[String]) -> Option<Run> {
        let awake = self.awake();
        if awake.is_empty() {
            return None;
        }
        let ids: Vec<String> = awake.iter().map(|x| smi_bus_id(&x.bus_id)).collect();
        let paths: Vec<String> = awake
            .iter()
            .map(|x| {
                x.dev
                    .join("power/runtime_status")
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        let mut cmd = vec!["nvidia-smi".to_string()];
        cmd.extend(args.iter().cloned());
        cmd.push("-i".into());
        cmd.push(ids.join(","));
        match exec(self.run, &guarded(&paths, &cmd)) {
            Run::Asleep => None,
            r => Some(r),
        }
    }
}

/// Bus id di sysfs → forma di `nvidia-smi` (dominio a 8 cifre): "0000:01:00.0" → "00000000:01:00.0".
fn smi_bus_id(bus_id: &str) -> String {
    match bus_id.split_once(':') {
        Some((dom, rest)) => format!("{dom:0>8}:{rest}"),
        None => bus_id.to_string(),
    }
}

/// `sh -c GUARD sh N <file di stato...> <comando...>`.
fn guarded(status_files: &[String], cmd: &[String]) -> Vec<String> {
    let mut a = vec![
        "sh".to_string(),
        "-c".to_string(),
        GUARD.to_string(),
        "sh".to_string(),
        status_files.len().to_string(),
    ];
    a.extend(status_files.iter().cloned());
    a.extend(cmd.iter().cloned());
    a
}

fn exec(run: &dyn CommandRunner, argv: &[String]) -> Run {
    let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
    match run.run(&argv, CMD_TIMEOUT) {
        Ok(CommandOutput {
            status,
            stdout,
            stderr,
        }) => {
            // la shell della guardia dà rc 127 se nvidia-smi non esiste (in locale e in remoto)
            if status == Some(127) {
                return Run::NotFound;
            }
            if status == Some(GUARD_ASLEEP) {
                return Run::Asleep;
            }
            let mut text = lossy(&stdout);
            // stderr serve solo a riconoscere gli errori di campo (su alcuni driver finiscono lì)
            let err = lossy(&stderr);
            if !err.is_empty() {
                text.push('\n');
                text.push_str(&err);
            }
            Run::Done { code: status, text }
        }
        Err(e) if e.kind() == ErrorKind::NotFound => Run::NotFound,
        Err(_) => Run::Failed,
    }
}

struct State {
    fields: &'static [&'static str],
    /// `pmon` rifiutato: si usa `--query-compute-apps`, riprovando `pmon` ogni tanto.
    pmon_off: bool,
    pmon_skipped: u32,
}

fn fail(status: NvStatus) -> NvResult {
    NvResult {
        status,
        gpus: Vec::new(),
        procs: Vec::new(),
        compute_only: false,
        at: Instant::now(),
    }
}

/// Un giro completo: query delle schede, poi dei processi.
/// `None` se le schede si sono sospese nel frattempo: niente da riportare.
fn poll(cx: &Ctx, st: &mut State) -> Option<NvResult> {
    let mut gpus = Vec::new();
    // al massimo due tentativi: campi completi, poi campi di base
    for _ in 0..2 {
        match cx.exec(&query_args(st.fields))? {
            Run::NotFound => return Some(fail(NvStatus::NoTool)),
            Run::Failed => return Some(fail(NvStatus::NotResponding)),
            Run::Asleep => return None,
            Run::Done { code, text } => {
                if nvidia::is_unknown_field_error(&text) {
                    if st.fields.len() == FIELDS_BASE.len() {
                        return Some(fail(NvStatus::NotResponding));
                    }
                    st.fields = FIELDS_BASE;
                    continue;
                }
                if code != Some(0) {
                    return Some(fail(NvStatus::NotResponding));
                }
                gpus = nvidia::parse_query(&text, st.fields);
                break;
            }
        }
    }
    if gpus.is_empty() {
        return Some(fail(NvStatus::NotResponding));
    }
    let mut procs = None;
    if st.pmon_off {
        st.pmon_skipped += 1;
        if st.pmon_skipped >= PMON_RETRY_EVERY {
            st.pmon_off = false;
        }
    }
    if !st.pmon_off {
        if let Run::Done {
            code: Some(0),
            text,
        } = cx.exec(&pmon_args())?
        {
            procs = nvidia::parse_pmon(&text);
        }
        if procs.is_none() {
            st.pmon_off = true;
            st.pmon_skipped = 0;
        }
    }
    let compute_only = procs.is_none();
    let procs = match procs {
        Some(p) => p,
        None => match cx.exec(&compute_apps_args())? {
            Run::Done {
                code: Some(0),
                text,
            } => nvidia::parse_compute_apps(&text),
            _ => Vec::new(),
        },
    };
    Some(NvResult {
        status: NvStatus::Ok,
        gpus,
        procs,
        compute_only,
        at: Instant::now(),
    })
}

fn worker(
    run: Arc<dyn CommandRunner>,
    src: Arc<dyn Source>,
    tx: Sender<NvResult>,
    wanted: Targets,
) {
    let mut st = State {
        fields: FIELDS_FULL,
        pmon_off: false,
        pmon_skipped: 0,
    };
    let mut backoff = POLL;
    loop {
        if !nap(Duration::ZERO, &wanted) {
            return;
        }
        let cx = Ctx {
            run: &*run,
            src: &*src,
            targets: &wanted,
        };
        // nessuna scheda sveglia: si aspetta a passi brevi senza lanciare nulla
        if cx.awake().is_empty() {
            if !nap(SLICE, &wanted) {
                return;
            }
            continue;
        }
        // la guardia l'ha trovata addormentata: un giro intero di pausa, non una raffica di richieste
        let Some(r) = poll(&cx, &mut st) else {
            if !nap(POLL, &wanted) {
                return;
            }
            continue;
        };
        let wait = match r.status {
            NvStatus::Ok => {
                backoff = POLL;
                POLL
            }
            NvStatus::NoTool => NOTOOL_RECHECK,
            NvStatus::NotResponding => {
                let w = backoff;
                backoff = (backoff * 2).min(BACKOFF_MAX);
                w
            }
        };
        if tx.send(r).is_err() || !nap(wait, &wanted) {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::LocalRunner;

    fn guardia(stati: &[&str]) -> CommandOutput {
        let dir = std::env::temp_dir().join(format!(
            "ngtop-nvguard-{}-{}",
            std::process::id(),
            stati.join("-")
        ));
        let _ = std::fs::create_dir_all(&dir);
        let mut files = Vec::new();
        for (i, st) in stati.iter().enumerate() {
            let f = dir.join(format!("s{i}"));
            // "" = file assente
            if !st.is_empty() {
                std::fs::write(&f, format!("{st}\n")).unwrap();
            }
            files.push(f.to_string_lossy().into_owned());
        }
        let cmd = vec!["echo".to_string(), "lanciato".to_string()];
        let argv = guarded(&files, &cmd);
        let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
        let out = LocalRunner.run(&argv, CMD_TIMEOUT).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        out
    }

    #[test]
    fn guardia_lancia_solo_con_schede_sveglie() {
        let ok = guardia(&["active", "unsupported"]);
        assert_eq!(ok.status, Some(0));
        assert_eq!(String::from_utf8_lossy(&ok.stdout).trim(), "lanciato");
        for stati in [
            &["suspended"][..],
            &["active", "suspending"][..],
            &["active", ""][..],
        ] {
            let o = guardia(stati);
            assert_eq!(o.status, Some(GUARD_ASLEEP), "{stati:?}");
            assert!(o.stdout.is_empty(), "{stati:?}");
        }
    }

    #[test]
    fn bus_id_per_nvidia_smi() {
        assert_eq!(smi_bus_id("0000:01:00.0"), "00000000:01:00.0");
        assert_eq!(smi_bus_id("10000:e1:00.0"), "00010000:e1:00.0");
    }

    #[test]
    fn lossy_taglia_a_max_output() {
        let big = vec![b'x'; MAX_OUTPUT * 2];
        assert_eq!(lossy(&big).len(), MAX_OUTPUT);
    }
}
