// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Backend per una macchina remota (ssh): solo lettura, senza D-Bus.
//! Parla con systemd esclusivamente attraverso `CommandRunner` (systemctl, journalctl, loginctl,
//! systemd-analyze) e `Source` (/proc/stat per l'ora di avvio). Per ogni comando c'è un ripiego per
//! i systemd vecchi che non conoscono l'output JSON. Le azioni non sono disponibili.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::{
    Action, ActionError, Backend, BootTimestamps, JournalLine, Scope, SessionInfo, UnitDetail,
    UnitInfo, classify_error, parse_blame, parse_journal_json, parse_span_token,
};
use crate::source::{CommandOutput, CommandRunner, Source};

/// Testo di errore delle azioni (avvia/ferma/riavvia/abilita/disabilita).
pub const REMOTE_ACTION_MSG: &str = "Not available on remote machines in this version";
/// Testo per le unità dell'utente (il bus di sessione non è raggiungibile via ssh).
pub const REMOTE_USER_MSG: &str = "User units are not available on remote machines";

/// Come per il D-Bus: interrogazioni brevi, registro più paziente.
const QUERY_TIMEOUT: Duration = Duration::from_secs(10);
const ACTION_TIMEOUT: Duration = Duration::from_secs(30);
const JOURNAL_TIMEOUT: Duration = Duration::from_secs(15);
const UNIT_FILES_TTL: Duration = Duration::from_secs(60);
const PID_TTL: Duration = Duration::from_secs(10);
/// Unità per ogni `systemctl show` (la riga di comando resta corta).
const SHOW_CHUNK: usize = 40;

const DETAIL_PROPS: &str = "Id,Description,MainPID,ControlGroup,MemoryCurrent,MemoryPeak,CPUUsageNSec,TasksCurrent,TasksMax,StateChangeTimestampMonotonic,ActiveEnterTimestampMonotonic,NRestarts,Restart,FragmentPath,Requires,Wants,SubState,ActiveState";

#[derive(Clone)]
struct SvcEntry {
    cgroup: Option<String>,
    pid: Option<u32>,
    sub_state: String,
    pid_at: Instant,
}

pub struct RemoteBackend {
    src: Arc<dyn Source>,
    run: Arc<dyn CommandRunner>,
    unit_files: Mutex<Option<(Instant, HashMap<String, String>)>>,
    svc_cache: Mutex<HashMap<String, SvcEntry>>,
}

impl RemoteBackend {
    pub fn new(src: Arc<dyn Source>, run: Arc<dyn CommandRunner>) -> Self {
        RemoteBackend {
            src,
            run,
            unit_files: Mutex::new(None),
            svc_cache: Mutex::new(HashMap::new()),
        }
    }

    /// Esegue il comando; `Err` solo se non si è potuto eseguire (ssh caduto, tempo scaduto).
    fn exec(&self, argv: &[&str], timeout: Duration) -> Result<CommandOutput, String> {
        self.run.run(argv, timeout).map_err(|e| match e.kind() {
            std::io::ErrorKind::TimedOut => "remote command timed out".to_string(),
            std::io::ErrorKind::NotFound => format!("{} not found", argv.first().unwrap_or(&"")),
            _ => e.to_string(),
        })
    }

    /// Testo dello stdout se il comando è riuscito (codice 0).
    fn exec_ok(&self, argv: &[&str], timeout: Duration) -> Option<String> {
        let out = self.exec(argv, timeout).ok()?;
        (out.status == Some(0)).then(|| String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// Come `exec_ok`, ma con un messaggio d'errore leggibile.
    fn exec_text(&self, argv: &[&str], timeout: Duration) -> Result<String, String> {
        let out = self.exec(argv, timeout)?;
        if out.status == Some(0) {
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        } else {
            Err(first_line(&out.stderr, &format!("{} failed", argv[0])))
        }
    }

    /// Stati di avvio (enabled, disabled…), con cache; se il comando fallisce l'elenco resta senza.
    fn unit_file_states(&self) -> HashMap<String, String> {
        if let Ok(g) = self.unit_files.lock()
            && let Some((at, m)) = g.as_ref()
            && at.elapsed() < UNIT_FILES_TTL
        {
            return m.clone();
        }
        let json = self
            .exec_ok(
                &[
                    "systemctl",
                    "list-unit-files",
                    "--output=json",
                    "--no-pager",
                ],
                QUERY_TIMEOUT,
            )
            .and_then(|t| parse_unit_files_json(&t));
        let map = json
            .or_else(|| {
                self.exec_ok(
                    &[
                        "systemctl",
                        "list-unit-files",
                        "--plain",
                        "--no-legend",
                        "--no-pager",
                    ],
                    QUERY_TIMEOUT,
                )
                .map(|t| parse_unit_files_plain(&t))
            })
            .unwrap_or_default();
        if let Ok(mut g) = self.unit_files.lock() {
            *g = Some((Instant::now(), map.clone()));
        }
        map
    }

    /// `systemctl show` raggruppato: un blocco di proprietà per unità.
    fn show(&self, names: &[&str], props: &str) -> Vec<HashMap<String, String>> {
        let mut out = Vec::new();
        for chunk in names.chunks(SHOW_CHUNK) {
            let mut argv = vec!["systemctl", "show", "-p", props, "--no-pager", "--"];
            argv.extend(chunk.iter().copied());
            if let Some(t) = self.exec_ok(&argv, QUERY_TIMEOUT) {
                out.extend(parse_show(&t));
            }
        }
        out
    }

    /// Ora di avvio (µs Unix) da /proc/stat della macchina osservata.
    fn boot_realtime_usec(&self) -> Option<u64> {
        let t = self.src.read_text(std::path::Path::new("/proc/stat"))?;
        parse_btime(&t).map(|s| s * 1_000_000)
    }
}

impl Backend for RemoteBackend {
    fn list_units(&self, scope: Scope) -> Result<Vec<UnitInfo>, String> {
        if scope == Scope::User {
            return Err(REMOTE_USER_MSG.to_string());
        }
        let json = self
            .exec_ok(
                &[
                    "systemctl",
                    "list-units",
                    "--all",
                    "--output=json",
                    "--no-pager",
                ],
                QUERY_TIMEOUT,
            )
            .and_then(|t| parse_units_json(&t));
        let mut units = match json {
            Some(u) => u,
            None => {
                let t = self.exec_text(
                    &[
                        "systemctl",
                        "list-units",
                        "--all",
                        "--plain",
                        "--no-legend",
                        "--no-pager",
                    ],
                    QUERY_TIMEOUT,
                )?;
                parse_units_plain(&t)
            }
        };
        let states = self.unit_file_states();
        for u in &mut units {
            // Istanze di template (`foo@bar.service`) hanno lo stato in `foo@.service`.
            let template = u
                .name
                .split_once('@')
                .and_then(|(a, b)| b.rsplit_once('.').map(|(_, ext)| format!("{a}@.{ext}")));
            u.unit_file_state = states
                .get(&u.name)
                .or_else(|| template.as_ref().and_then(|t| states.get(t)))
                .cloned()
                .unwrap_or_default();
        }
        // PID principale e cgroup: solo servizi attivi; si rileggono se cambia lo stato o dopo PID_TTL.
        let mut cache = self.svc_cache.lock().map(|g| g.clone()).unwrap_or_default();
        let mut to_ask: Vec<String> = Vec::new();
        for u in &units {
            if is_live_service(u) {
                let fresh = cache
                    .get(&u.name)
                    .is_some_and(|e| e.sub_state == u.sub_state && e.pid_at.elapsed() < PID_TTL);
                if !fresh && valid_unit_name(&u.name) {
                    to_ask.push(u.name.clone());
                }
            }
        }
        let names: Vec<&str> = to_ask.iter().map(String::as_str).collect();
        for block in self.show(&names, "Id,MainPID,ControlGroup") {
            let Some(id) = block.get("Id") else { continue };
            let Some(u) = units.iter().find(|u| &u.name == id) else {
                continue;
            };
            cache.insert(
                id.clone(),
                SvcEntry {
                    cgroup: block.get("ControlGroup").filter(|c| !c.is_empty()).cloned(),
                    pid: block
                        .get("MainPID")
                        .and_then(|p| p.parse::<u32>().ok())
                        .filter(|&p| p > 0),
                    sub_state: u.sub_state.clone(),
                    pid_at: Instant::now(),
                },
            );
        }
        // Unità senza risposta: si ritenta dopo PID_TTL, non a ogni aggiornamento.
        for u in units.iter().filter(|u| to_ask.contains(&u.name)) {
            cache
                .entry(u.name.clone())
                .and_modify(|e| e.pid_at = Instant::now())
                .or_insert_with(|| SvcEntry {
                    cgroup: None,
                    pid: None,
                    sub_state: u.sub_state.clone(),
                    pid_at: Instant::now(),
                });
        }
        for u in &mut units {
            if is_live_service(u)
                && let Some(e) = cache.get(&u.name)
            {
                u.main_pid = e.pid;
                u.cgroup = e.cgroup.clone();
            }
        }
        units.sort_by_cached_key(|u| u.name.to_lowercase());
        cache.retain(|k, _| units.iter().any(|u| &u.name == k));
        if let Ok(mut g) = self.svc_cache.lock() {
            *g = cache;
        }
        Ok(units)
    }

    fn unit_detail(&self, scope: Scope, name: &str) -> Result<UnitDetail, String> {
        if scope == Scope::User {
            return Err(REMOTE_USER_MSG.to_string());
        }
        if !valid_unit_name(name) {
            return Err("invalid unit name".to_string());
        }
        let text = self.exec_text(
            &[
                "systemctl",
                "show",
                "-p",
                DETAIL_PROPS,
                "--no-pager",
                "--",
                name,
            ],
            QUERY_TIMEOUT,
        )?;
        let blocks = parse_show(&text);
        let p = blocks.first().ok_or("no data for this unit")?;
        let boot = self.boot_realtime_usec();
        Ok(detail_from_props(name, p, boot))
    }

    fn action(&self, scope: Scope, name: &str, action: Action) -> Result<(), ActionError> {
        if scope == Scope::User {
            return Err(ActionError::Failed(REMOTE_USER_MSG.to_string()));
        }
        if !valid_unit_name(name) {
            return Err(ActionError::Failed("invalid unit name".to_string()));
        }
        // Come in locale: start/stop/restart in coda senza attendere il job (`--no-block`,
        // equivale a StartUnit con "replace"); enable/disable senza `--now`.
        let verb = match action {
            Action::Start => "start",
            Action::Stop => "stop",
            Action::Restart => "restart",
            Action::Enable => "enable",
            Action::Disable => "disable",
        };
        let mut argv = vec!["systemctl", "--no-ask-password", "--no-pager"];
        if matches!(action, Action::Start | Action::Stop | Action::Restart) {
            argv.push("--no-block");
        }
        argv.extend([verb, "--", name]);
        if matches!(action, Action::Enable | Action::Disable)
            && let Ok(mut g) = self.unit_files.lock()
        {
            // Cache scaduta: il prossimo elenco rilegge gli stati.
            *g = None;
        }
        let out = self
            .exec(&argv, ACTION_TIMEOUT)
            .map_err(ActionError::Failed)?;
        if out.status == Some(0) {
            return Ok(());
        }
        let root = self.src.remote_status().is_some_and(|s| s.root);
        Err(action_error(&out.stderr, root))
    }

    fn boot_times(&self) -> Result<BootTimestamps, String> {
        // LC_ALL=C: il testo di systemd-analyze è tradotto nella lingua della macchina remota.
        let text = self.exec_text(
            &["env", "LC_ALL=C", "systemd-analyze", "time", "--no-pager"],
            QUERY_TIMEOUT,
        )?;
        let mut t = parse_analyze_time(&text).ok_or("boot timing unavailable")?;
        t.kernel_realtime = self.boot_realtime_usec().unwrap_or(0);
        Ok(t)
    }

    fn sessions(&self) -> Result<Vec<SessionInfo>, String> {
        let json = self
            .exec_ok(
                &["loginctl", "list-sessions", "--json=short", "--no-pager"],
                QUERY_TIMEOUT,
            )
            .and_then(|t| parse_sessions_json(&t));
        let mut list = match json {
            Some(l) => l,
            None => {
                let t = self.exec_text(
                    &["loginctl", "list-sessions", "--no-legend", "--no-pager"],
                    QUERY_TIMEOUT,
                )?;
                parse_sessions_plain(&t)
            }
        };
        list.retain(|s| valid_session_id(&s.id));
        if list.is_empty() {
            return Ok(list);
        }
        // Tipo, tty, host remoto, desktop e ora di inizio: un solo `show-session` per tutte.
        let mut argv: Vec<&str> = vec!["loginctl", "show-session"];
        argv.extend([
            "-p",
            "Id",
            "-p",
            "Type",
            "-p",
            "TTY",
            "-p",
            "TimestampMonotonic",
            "-p",
            "RemoteHost",
            "-p",
            "Desktop",
            "--no-pager",
            "--",
        ]);
        argv.extend(list.iter().map(|s| s.id.as_str()));
        let boot = self.boot_realtime_usec();
        if let Some(t) = self.exec_ok(&argv, QUERY_TIMEOUT) {
            let props: HashMap<String, HashMap<String, String>> = parse_show(&t)
                .into_iter()
                .filter_map(|b| b.get("Id").cloned().map(|id| (id, b)))
                .collect();
            for s in &mut list {
                let Some(p) = props.get(&s.id) else { continue };
                let g = |k: &str| p.get(k).cloned().unwrap_or_default();
                s.kind = g("Type");
                if s.tty.is_empty() {
                    s.tty = g("TTY");
                }
                s.remote_host = g("RemoteHost");
                s.desktop = g("Desktop");
                if s.since_usec == 0
                    && let (Some(b), Some(m)) = (boot, p.get("TimestampMonotonic"))
                    && let Ok(m) = m.parse::<u64>()
                    && m > 0
                {
                    s.since_usec = b + m;
                }
            }
        }
        Ok(list)
    }

    fn journal(&self, scope: Scope, name: &str, n: usize) -> Result<Vec<JournalLine>, String> {
        if scope == Scope::User {
            return Err(REMOTE_USER_MSG.to_string());
        }
        if !valid_unit_name(name) {
            return Err("invalid unit name".to_string());
        }
        let n = n.to_string();
        let unit_arg = format!("--unit={name}");
        let out = self.exec(
            &[
                "journalctl",
                &unit_arg,
                "-n",
                &n,
                "-o",
                "json",
                "--no-pager",
                "-q",
            ],
            JOURNAL_TIMEOUT,
        )?;
        if out.status != Some(0) {
            return Err(super::journal_error(&out.stderr));
        }
        Ok(parse_journal_json(&String::from_utf8_lossy(&out.stdout)))
    }

    fn blame(&self) -> Result<Vec<(String, f64)>, String> {
        let t = self
            .exec_text(&["systemd-analyze", "blame", "--no-pager"], JOURNAL_TIMEOUT)
            .map_err(|_| "systemd-analyze blame unavailable".to_string())?;
        Ok(parse_blame(&t))
    }
}

fn is_live_service(u: &UnitInfo) -> bool {
    u.name.ends_with(".service")
        && matches!(
            u.active_state.as_str(),
            "active" | "activating" | "reloading"
        )
}

/// Nomi sicuri da passare a un comando remoto: niente caratteri speciali. Possono iniziare con `-`
/// (`-.mount`, `-.slice`): i comandi li ricevono dopo `--` o nella forma `--unit=NOME`.
fn valid_unit_name(n: &str) -> bool {
    !n.is_empty()
        && n.len() < 256
        && n.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '.' | '\\' | '@' | '-'))
}

fn valid_session_id(n: &str) -> bool {
    !n.is_empty() && n.len() < 32 && n.chars().all(|c| c.is_ascii_alphanumeric())
}

fn first_line(stderr: &[u8], default: &str) -> String {
    String::from_utf8_lossy(stderr)
        .lines()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim().to_string())
        .unwrap_or_else(|| default.to_string())
}

/// Testo dell'ultima riga significativa di stderr, senza caratteri di controllo.
fn last_line(stderr: &[u8]) -> String {
    let t = String::from_utf8_lossy(stderr);
    let l = t
        .lines()
        .rev()
        .map(|l| l.chars().filter(|c| !c.is_control()).collect::<String>())
        .map(|l| l.trim().to_string())
        .find(|l| !l.is_empty());
    l.unwrap_or_else(|| "systemctl failed".to_string())
}

/// Traduce l'errore di `systemctl` nello stesso modo del backend locale.
fn action_error(stderr: &[u8], root: bool) -> ActionError {
    let text = String::from_utf8_lossy(stderr);
    let l = text.to_lowercase();
    if l.contains("not found") || l.contains("not loaded") || l.contains("no such file") {
        return ActionError::Failed("unit not found".to_string());
    }
    if matches!(classify_error(&text), ActionError::Denied) && !root {
        return ActionError::Failed("needs root: start ngtop with --sudo".to_string());
    }
    ActionError::Failed(last_line(stderr))
}

fn str_of(v: &serde_json::Value, k: &str) -> String {
    v.get(k)
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string()
}

/// `list-units --output=json`. `None` se non è JSON valido (systemd vecchio).
pub fn parse_units_json(text: &str) -> Option<Vec<UnitInfo>> {
    let v: serde_json::Value = serde_json::from_str(text.trim()).ok()?;
    let a = v.as_array()?;
    Some(
        a.iter()
            .filter_map(|u| {
                let name = str_of(u, "unit");
                (!name.is_empty()).then(|| UnitInfo {
                    name,
                    description: str_of(u, "description"),
                    load_state: str_of(u, "load"),
                    active_state: str_of(u, "active"),
                    sub_state: str_of(u, "sub"),
                    ..Default::default()
                })
            })
            .collect(),
    )
}

/// Ripiego: `list-units --plain --no-legend` → UNIT LOAD ACTIVE SUB DESCRIPTION (descrizione con spazi).
pub fn parse_units_plain(text: &str) -> Vec<UnitInfo> {
    text.lines()
        .filter_map(|l| {
            let mut rest = l.trim();
            // Marcatore di stato ("●", "×", "*") che alcune versioni stampano anche con --plain.
            if let Some((first, tail)) = rest.split_once(char::is_whitespace)
                && first.chars().count() == 1
                && !first.chars().all(|c| c.is_ascii_alphanumeric())
            {
                rest = tail.trim_start();
            }
            let mut f = [""; 4];
            for slot in &mut f {
                let (a, b) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
                *slot = a;
                rest = b.trim_start();
            }
            (!f[0].is_empty() && f[0].contains('.') && !f[3].is_empty()).then(|| UnitInfo {
                name: f[0].to_string(),
                load_state: f[1].to_string(),
                active_state: f[2].to_string(),
                sub_state: f[3].to_string(),
                description: rest.trim().to_string(),
                ..Default::default()
            })
        })
        .collect()
}

fn base_name(s: &str) -> String {
    s.rsplit('/').next().unwrap_or(s).to_string()
}

/// `list-unit-files --output=json` → nome → stato.
pub fn parse_unit_files_json(text: &str) -> Option<HashMap<String, String>> {
    let v: serde_json::Value = serde_json::from_str(text.trim()).ok()?;
    Some(
        v.as_array()?
            .iter()
            .filter_map(|u| {
                let n = str_of(u, "unit_file");
                (!n.is_empty()).then(|| (base_name(&n), str_of(u, "state")))
            })
            .collect(),
    )
}

/// Ripiego: `list-unit-files --plain --no-legend` → UNIT STATE [PRESET].
pub fn parse_unit_files_plain(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let n = it.next()?;
            let s = it.next()?;
            n.contains('.').then(|| (base_name(n), s.to_string()))
        })
        .collect()
}

/// Output di `systemctl show` / `loginctl show-session`: blocchi `chiave=valore` separati da righe vuote.
pub fn parse_show(text: &str) -> Vec<HashMap<String, String>> {
    let mut out = Vec::new();
    let mut cur: HashMap<String, String> = HashMap::new();
    for l in text.lines() {
        if l.trim().is_empty() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        } else if let Some((k, v)) = l.split_once('=') {
            cur.insert(k.to_string(), v.to_string());
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Numero senza i valori "non impostato" (`[not set]`, u64::MAX, infinity).
fn limited(p: &HashMap<String, String>, k: &str) -> Option<u64> {
    p.get(k)?
        .trim()
        .parse::<u64>()
        .ok()
        .filter(|&v| v != u64::MAX)
}

fn words(p: &HashMap<String, String>, k: &str) -> Vec<String> {
    p.get(k)
        .map(|v| v.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default()
}

fn detail_from_props(
    name: &str,
    p: &HashMap<String, String>,
    boot_usec: Option<u64>,
) -> UnitDetail {
    let s = |k: &str| p.get(k).cloned().unwrap_or_default();
    let mono = limited(p, "StateChangeTimestampMonotonic")
        .filter(|&t| t > 0)
        .or_else(|| limited(p, "ActiveEnterTimestampMonotonic").filter(|&t| t > 0));
    UnitDetail {
        name: name.to_string(),
        description: s("Description"),
        active_state: s("ActiveState"),
        sub_state: s("SubState"),
        since_usec: mono.and_then(|m| boot_usec.map(|b| b + m)),
        main_pid: limited(p, "MainPID")
            .filter(|&x| x > 0)
            .and_then(|x| u32::try_from(x).ok()),
        comm: None,
        tasks: limited(p, "TasksCurrent"),
        tasks_max: limited(p, "TasksMax"),
        mem_current: limited(p, "MemoryCurrent"),
        mem_peak: limited(p, "MemoryPeak"),
        cpu_nsec: limited(p, "CPUUsageNSec"),
        requires: words(p, "Requires"),
        wants: words(p, "Wants"),
        n_restarts: limited(p, "NRestarts").and_then(|x| u32::try_from(x).ok()),
        restart: s("Restart"),
        fragment_path: s("FragmentPath"),
        cgroup: p.get("ControlGroup").filter(|c| !c.is_empty()).cloned(),
    }
}

/// Riga `btime` di /proc/stat (secondi Unix dell'avvio).
fn parse_btime(stat: &str) -> Option<u64> {
    stat.lines()
        .find_map(|l| l.strip_prefix("btime "))
        .and_then(|v| v.trim().parse().ok())
}

/// `systemd-analyze time`: "Startup finished in 8.1s (firmware) + 1.2s (loader) + 3.5s (kernel) +
/// 2s (initrd) + 2.7s (userspace) = 17.5s" → timestamp come quelli del Manager (vedi `boot_phases`).
pub fn parse_analyze_time(text: &str) -> Option<BootTimestamps> {
    let line = text
        .lines()
        .find(|l| l.contains("(kernel)") || l.contains("(userspace)"))?;
    let body = line.split('=').next()?;
    let body = body.split_once(" in ").map_or(body, |(_, b)| b);
    let (mut fw, mut loader, mut kernel, mut initrd, mut user) = (0.0, 0.0, 0.0, 0.0, 0.0);
    let mut found = false;
    for piece in body.split('+') {
        let toks: Vec<&str> = piece.split_whitespace().collect();
        let Some((label, spans)) = toks.split_last() else {
            continue;
        };
        let Some(label) = label.strip_prefix('(').and_then(|l| l.strip_suffix(')')) else {
            continue;
        };
        let mut secs = 0.0;
        for t in spans {
            secs += parse_span_token(t)?;
        }
        found = true;
        match label {
            "firmware" => fw = secs,
            "loader" => loader = secs,
            "kernel" => kernel = secs,
            "initrd" => initrd = secs,
            "userspace" => user = secs,
            _ => {}
        }
    }
    if !found || user <= 0.0 {
        return None;
    }
    let us = |s: f64| (s * 1e6).round() as u64;
    let userspace = us(kernel + initrd);
    Some(BootTimestamps {
        firmware: if fw > 0.0 && loader > 0.0 {
            us(fw + loader)
        } else {
            0
        },
        loader: us(loader),
        initrd: if initrd > 0.0 { us(kernel) } else { 0 },
        userspace,
        finish: userspace + us(user),
        kernel_realtime: 0,
    })
}

/// `loginctl list-sessions --json=short`. `None` se non è JSON (systemd vecchio: ignora l'opzione o la rifiuta).
pub fn parse_sessions_json(text: &str) -> Option<Vec<SessionInfo>> {
    let v: serde_json::Value = serde_json::from_str(text.trim()).ok()?;
    Some(
        v.as_array()?
            .iter()
            .map(|s| SessionInfo {
                id: s
                    .get("session")
                    .map(|x| x.as_str().map_or_else(|| x.to_string(), str::to_string))
                    .unwrap_or_default(),
                user: str_of(s, "user"),
                uid: s
                    .get("uid")
                    .and_then(|x| x.as_u64())
                    .and_then(|x| u32::try_from(x).ok())
                    .unwrap_or(0),
                seat: str_of(s, "seat"),
                tty: str_of(s, "tty"),
                since_usec: s.get("since").and_then(|x| x.as_u64()).unwrap_or(0),
                ..Default::default()
            })
            .collect(),
    )
}

/// Ripiego: `loginctl list-sessions --no-legend`. Formato nuovo `ID UID USER SEAT LEADER CLASS TTY IDLE SINCE`,
/// vecchio `ID UID USER [SEAT] [TTY]` (colonne vuote assenti).
pub fn parse_sessions_plain(text: &str) -> Vec<SessionInfo> {
    let dash = |s: &str| {
        if s == "-" {
            String::new()
        } else {
            s.to_string()
        }
    };
    text.lines()
        .filter_map(|l| {
            let t: Vec<&str> = l.split_whitespace().collect();
            if t.len() < 3 {
                return None;
            }
            let uid = t[1].parse::<u32>().ok()?;
            let (seat, tty) = if t.len() >= 7 && t[4].chars().all(|c| c.is_ascii_digit()) {
                (dash(t[3]), dash(t[6]))
            } else {
                let rest = &t[3..];
                match rest {
                    [] => (String::new(), String::new()),
                    [a] if a.starts_with("seat") => (a.to_string(), String::new()),
                    [a] => (String::new(), a.to_string()),
                    [a, b, ..] => (dash(a), dash(b)),
                }
            };
            Some(SessionInfo {
                id: t[0].to_string(),
                uid,
                user: t[2].to_string(),
                seat,
                tty,
                ..Default::default()
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collect::services::worker::{Client, Request, Response};
    use crate::source::FsStat;
    use std::ffi::OsString;
    use std::io;
    use std::path::{Path, PathBuf};

    fn fx(n: &str) -> String {
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/services-remote")
            .join(n);
        std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
    }

    /// Sorgente finta: file in memoria.
    struct FakeSrc(HashMap<String, String>, bool);
    impl Source for FakeSrc {
        fn read(&self, p: &Path) -> io::Result<Vec<u8>> {
            self.0
                .get(p.to_str().unwrap_or(""))
                .map(|s| s.clone().into_bytes())
                .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
        }
        fn read_dir(&self, _: &Path) -> io::Result<Vec<OsString>> {
            Err(io::Error::from(io::ErrorKind::NotFound))
        }
        fn read_link(&self, _: &Path) -> io::Result<PathBuf> {
            Err(io::Error::from(io::ErrorKind::NotFound))
        }
        fn owner_uid(&self, _: &Path) -> io::Result<u32> {
            Err(io::Error::from(io::ErrorKind::NotFound))
        }
        fn statvfs(&self, _: &Path) -> io::Result<FsStat> {
            Err(io::Error::from(io::ErrorKind::NotFound))
        }
        fn exists(&self, p: &Path) -> bool {
            self.0.contains_key(p.to_str().unwrap_or(""))
        }
        fn remote_status(&self) -> Option<crate::source::RemoteStatus> {
            Some(crate::source::RemoteStatus {
                target: "test".into(),
                hostname: None,
                latency_ms: None,
                last_bytes: 0,
                root: self.1,
                state: crate::source::RemoteState::Connected,
            })
        }
    }

    /// Esecutore finto: risponde in base ai primi argomenti; registra le chiamate.
    struct FakeRun {
        /// (testo che deve comparire nella riga di comando, codice, stdout)
        rules: Vec<(String, i32, String)>,
        calls: Mutex<Vec<String>>,
        delay: Duration,
    }
    impl FakeRun {
        fn new(rules: Vec<(&str, i32, String)>) -> Arc<FakeRun> {
            Arc::new(FakeRun {
                rules: rules
                    .into_iter()
                    .map(|(a, b, c)| (a.to_string(), b, c))
                    .collect(),
                calls: Mutex::new(Vec::new()),
                delay: Duration::ZERO,
            })
        }
    }
    impl CommandRunner for FakeRun {
        fn run(&self, argv: &[&str], _: Duration) -> io::Result<CommandOutput> {
            std::thread::sleep(self.delay);
            let line = argv.join(" ");
            self.calls.lock().unwrap().push(line.clone());
            for (pat, code, out) in &self.rules {
                if line.contains(pat.as_str()) {
                    return Ok(CommandOutput {
                        status: Some(*code),
                        stdout: if *code == 0 {
                            out.clone().into_bytes()
                        } else {
                            vec![]
                        },
                        stderr: if *code == 0 {
                            vec![]
                        } else {
                            out.clone().into_bytes()
                        },
                    });
                }
            }
            Ok(CommandOutput {
                status: Some(1),
                stdout: vec![],
                stderr: b"Unknown command".to_vec(),
            })
        }
    }

    fn src() -> Arc<dyn Source> {
        let mut m = HashMap::new();
        m.insert("/run/systemd/system".to_string(), String::new());
        m.insert(
            "/proc/stat".to_string(),
            "cpu 1 2 3\nbtime 1700000000\n".to_string(),
        );
        Arc::new(FakeSrc(m, false))
    }

    fn modern() -> Arc<FakeRun> {
        FakeRun::new(vec![
            ("list-units --all --output=json", 0, fx("list-units.json")),
            (
                "list-unit-files --output=json",
                0,
                fx("list-unit-files.json"),
            ),
            (
                "show -p Id,MainPID,ControlGroup --no-pager",
                0,
                fx("show-list.txt"),
            ),
            ("show -p Id,Description", 0, fx("show-detail.txt")),
            ("journalctl --unit=", 0, fx("journal.json")),
            (
                "loginctl list-sessions --json=short",
                0,
                fx("sessions.json"),
            ),
            ("loginctl show-session", 0, fx("show-session.txt")),
            ("systemd-analyze time", 0, fx("analyze-time.txt")),
            ("systemd-analyze blame", 0, fx("blame.txt")),
        ])
    }

    /// Systemd vecchio: niente JSON (opzioni rifiutate), solo output plain.
    fn legacy() -> Arc<FakeRun> {
        FakeRun::new(vec![
            (
                "--output=json",
                1,
                "systemctl: unrecognized option '--output=json'".into(),
            ),
            (
                "--json=short",
                1,
                "loginctl: unrecognized option '--json=short'".into(),
            ),
            ("list-units --all --plain", 0, fx("list-units.plain")),
            ("list-unit-files --plain", 0, fx("list-unit-files.plain")),
            (
                "show -p Id,MainPID,ControlGroup --no-pager",
                0,
                fx("show-list.txt"),
            ),
            (
                "loginctl list-sessions --no-legend",
                0,
                fx("sessions.plain"),
            ),
            ("loginctl show-session", 0, fx("show-session.txt")),
        ])
    }

    #[test]
    fn elenco_json_con_stati_e_pid() {
        let b = RemoteBackend::new(src(), modern());
        let u = b.list_units(Scope::System).unwrap();
        assert!(u.len() > 20);
        // Ordinate senza distinguere maiuscole.
        assert!(
            u.windows(2)
                .all(|w| w[0].name.to_lowercase() <= w[1].name.to_lowercase())
        );
        let ssh = u.iter().find(|u| u.name == "ssh.service");
        let acc = u
            .iter()
            .find(|u| u.name == "accounts-daemon.service")
            .unwrap();
        assert_eq!(acc.active_state, "active");
        assert!(!acc.unit_file_state.is_empty());
        assert!(acc.main_pid.is_some());
        assert_eq!(
            acc.cgroup.as_deref(),
            Some("/system.slice/accounts-daemon.service")
        );
        let _ = ssh;
        // Istanza di template: lo stato viene da `foo@.service`.
        let t = u.iter().find(|u| u.name == "foo@bar.service").unwrap();
        assert_eq!(t.unit_file_state, "disabled");
        assert_eq!(t.description, "Template instance");
    }

    #[test]
    fn elenco_ripiego_plain_senza_json() {
        let r = legacy();
        let b = RemoteBackend::new(src(), r.clone());
        let u = b.list_units(Scope::System).unwrap();
        let json = parse_units_json(&fx("list-units.json")).unwrap();
        assert_eq!(u.len(), json.len());
        let a = u
            .iter()
            .find(|u| u.name == "accounts-daemon.service")
            .unwrap();
        let j = json.iter().find(|u| u.name == a.name).unwrap();
        assert_eq!(a.description, j.description);
        assert_eq!(a.sub_state, j.sub_state);
        assert!(!a.unit_file_state.is_empty());
        assert_eq!(
            a.cgroup.as_deref(),
            Some("/system.slice/accounts-daemon.service")
        );
        let calls = r.calls.lock().unwrap();
        assert!(calls.iter().any(|c| c.contains("--output=json")));
        assert!(calls.iter().any(|c| c.contains("--plain --no-legend")));
    }

    #[test]
    fn plain_con_descrizioni_e_marcatori() {
        let u = parse_units_plain(
            "● foo.service   loaded failed failed  Un servizio rotto\n  bar.mount loaded active mounted\nRumore\n",
        );
        assert_eq!(u.len(), 2);
        assert_eq!(u[0].name, "foo.service");
        assert_eq!(u[0].description, "Un servizio rotto");
        assert_eq!(u[0].active_state, "failed");
        assert_eq!(u[1].description, "");
        // Le intestazioni/legende non diventano unità.
        assert!(parse_units_plain("UNIT LOAD ACTIVE SUB DESCRIPTION\n").is_empty());
        assert!(parse_units_json("systemctl: error").is_none());
    }

    #[test]
    fn cache_dei_pid_evita_comandi() {
        let r = modern();
        let b = RemoteBackend::new(src(), r.clone());
        b.list_units(Scope::System).unwrap();
        let n1 = r.calls.lock().unwrap().len();
        b.list_units(Scope::System).unwrap();
        let calls = r.calls.lock().unwrap();
        // Secondo giro: solo `list-units` (unit-files e show sono in cache).
        assert_eq!(calls.len() - n1, 1);
    }

    #[test]
    fn dettaglio() {
        let b = RemoteBackend::new(src(), modern());
        let d = b.unit_detail(Scope::System, "ssh.service").unwrap();
        assert_eq!(d.active_state, "active");
        assert_eq!(d.sub_state, "running");
        assert_eq!(d.description, "OpenBSD Secure Shell server");
        assert_eq!(d.fragment_path, "/usr/lib/systemd/system/ssh.service");
        assert_eq!(d.requires, ["sysinit.target", "system.slice"]);
        assert_eq!(d.wants, ["sshd-keygen.service"]);
        assert_eq!(d.mem_peak, Some(5_767_168));
        assert_eq!(d.tasks, Some(1));
        assert_eq!(d.cpu_nsec, Some(40_568_000));
        assert!(d.main_pid.is_some());
        assert_eq!(d.since_usec, Some(1_700_000_000_000_000 + 6_134_381));
        assert!(d.cgroup.is_some());
    }

    #[test]
    fn valori_non_impostati() {
        let p = parse_show(
            "MemoryCurrent=[not set]\nMainPID=0\nTasksMax=infinity\nMemoryPeak=18446744073709551615\n",
        );
        let d = detail_from_props("x.service", &p[0], None);
        assert_eq!(d.mem_current, None);
        assert_eq!(d.main_pid, None);
        assert_eq!(d.tasks_max, None);
        assert_eq!(d.mem_peak, None);
        assert_eq!(d.since_usec, None);
    }

    #[test]
    fn utente_azioni_e_nomi_pericolosi() {
        let b = RemoteBackend::new(src(), modern());
        assert_eq!(b.list_units(Scope::User).unwrap_err(), REMOTE_USER_MSG);
        assert_eq!(
            b.journal(Scope::User, "a.service", 5).unwrap_err(),
            REMOTE_USER_MSG
        );
        assert_eq!(
            b.unit_detail(Scope::User, "a.service").unwrap_err(),
            REMOTE_USER_MSG
        );
        for a in [
            Action::Start,
            Action::Stop,
            Action::Restart,
            Action::Enable,
            Action::Disable,
        ] {
            assert_eq!(
                b.action(Scope::User, "a.service", a),
                Err(ActionError::Failed(REMOTE_USER_MSG.into()))
            );
        }
        assert!(b.unit_detail(Scope::System, "--now; rm").is_err());
        assert!(b.unit_detail(Scope::System, "a b").is_err());
        assert!(b.journal(Scope::System, "a$(x)", 5).is_err());
        assert!(b.journal(Scope::System, "a\nb", 5).is_err());
    }

    #[test]
    fn nomi_che_iniziano_con_trattino() {
        let r = modern();
        let b = RemoteBackend::new(src(), r.clone());
        for n in ["-.mount", "-.slice"] {
            assert!(b.unit_detail(Scope::System, n).is_ok());
            assert!(b.journal(Scope::System, n, 5).is_ok());
        }
        let c = r.calls.lock().unwrap();
        assert!(c[0].ends_with("--no-pager -- -.mount"));
        assert!(c[1].starts_with("journalctl --unit=-.mount "));
        assert!(c[3].starts_with("journalctl --unit=-.slice "));
    }

    #[test]
    fn registro() {
        let r = modern();
        let b = RemoteBackend::new(src(), r.clone());
        let l = b.journal(Scope::System, "ssh.service", 50).unwrap();
        assert_eq!(l.len(), 4);
        assert!(l.windows(2).all(|w| w[0].time_usec <= w[1].time_usec));
        assert!(
            r.calls.lock().unwrap()[0].starts_with("journalctl --unit=ssh.service -n 50 -o json")
        );
        let rf = FakeRun::new(vec![(
            "journalctl",
            1,
            "Failed to open journal: Permission denied\nx".into(),
        )]);
        let b = RemoteBackend::new(src(), rf);
        assert_eq!(
            b.journal(Scope::System, "ssh.service", 5).unwrap_err(),
            "Failed to open journal: Permission denied"
        );
    }

    #[test]
    fn sessioni_json_e_plain() {
        let b = RemoteBackend::new(src(), modern());
        let s = b.sessions().unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].id, "2");
        assert_eq!(s[0].uid, 1000);
        assert_eq!(s[0].user, "alex");
        assert_eq!(s[0].seat, "seat0");
        assert_eq!(s[0].tty, "tty2");
        assert_eq!(s[0].kind, "wayland");
        assert_eq!(s[1].kind, "unspecified");
        assert_eq!(s[1].tty, "");
        assert_eq!(s[0].since_usec, 1_700_000_000_000_000 + 31_736_469);
        // Ripiego: la tabella.
        let b = RemoteBackend::new(src(), legacy());
        let p = b.sessions().unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].seat, "seat0");
        assert_eq!(p[0].tty, "tty2");
        assert_eq!(p[1].seat, "");
        assert_eq!(p[1].kind, "unspecified");
        // Formato vecchio: ID UID USER SEAT TTY, con colonne vuote omesse.
        let old =
            parse_sessions_plain("   c1 1000 bob seat0 tty1\n   4 0 root\n   5 1001 ann pts/0\n");
        assert_eq!(old.len(), 3);
        assert_eq!(
            (old[0].seat.as_str(), old[0].tty.as_str()),
            ("seat0", "tty1")
        );
        assert_eq!((old[1].seat.as_str(), old[1].tty.as_str()), ("", ""));
        assert_eq!((old[2].seat.as_str(), old[2].tty.as_str()), ("", "pts/0"));
        // `--output=json` ignorato da loginctl: la tabella non è JSON.
        assert!(parse_sessions_json(&fx("sessions.plain")).is_none());
    }

    #[test]
    fn fasi_di_avvio() {
        let b = RemoteBackend::new(src(), modern());
        let t = b.boot_times().unwrap();
        let p = super::super::boot_phases(&t).unwrap();
        let labels: Vec<_> = p.iter().map(|(f, _)| f.label()).collect();
        assert_eq!(labels, ["firmware", "loader", "kernel", "services"]);
        assert!((p[0].1 - 7.004).abs() < 1e-6);
        assert!((p[1].1 - 6.053).abs() < 1e-6);
        assert!((p[2].1 - 3.589).abs() < 1e-6);
        assert!((p[3].1 - 2.769).abs() < 1e-6);
        assert_eq!(t.kernel_realtime, 1_700_000_000_000_000);

        let t = parse_analyze_time(&fx("analyze-time-initrd.txt")).unwrap();
        let p = super::super::boot_phases(&t).unwrap();
        assert_eq!(p.len(), 4 - 1); // kernel, initrd, services
        assert!((p[0].1 - 2.301).abs() < 1e-6);
        assert!((p[1].1 - 1.912).abs() < 1e-6);
        assert!((p[2].1 - 6.387).abs() < 1e-6);

        let t = parse_analyze_time(&fx("analyze-time-min.txt")).unwrap();
        let p = super::super::boot_phases(&t).unwrap();
        assert!((p[0].1 - 63.25).abs() < 1e-6);
        assert!((p[3].1 - 61.1).abs() < 1e-6);

        // Solo kernel + userspace (VM), e avvio non finito.
        let t = parse_analyze_time(
            "Startup finished in 1.895s (kernel) + 4.478s (userspace) = 6.374s\n",
        )
        .unwrap();
        assert_eq!(super::super::boot_phases(&t).unwrap().len(), 2);
        assert!(parse_analyze_time("Bootup is not yet finished (org.freedesktop.systemd1.Manager.FinishTimestampMonotonic=0).\n").is_none());
        assert!(parse_analyze_time("").is_none());
    }

    #[test]
    fn blame_e_errori() {
        let b = RemoteBackend::new(src(), modern());
        let l = b.blame().unwrap();
        assert!(l.len() >= 10);
        assert_eq!(l[0].0, "systemd-binfmt.service");
        assert!((l[0].1 - 1.03).abs() < 1e-9);
        // Comando che non esiste / ssh senza risposta: errori, mai panic.
        let b = RemoteBackend::new(src(), FakeRun::new(vec![]));
        assert!(b.blame().is_err());
        assert!(b.sessions().is_err());
        assert!(b.boot_times().is_err());
        assert!(b.list_units(Scope::System).is_err());
    }

    #[test]
    fn scelta_del_backend_e_systemd() {
        assert!(super::super::systemd_present_in(&*src(), Path::new("/run")));
        let vuota: Arc<dyn Source> = Arc::new(FakeSrc(HashMap::new(), false));
        assert!(!super::super::systemd_present_in(
            &*vuota,
            Path::new("/run")
        ));
        let run: Arc<dyn CommandRunner> = modern();
        // Remoto → RemoteBackend: le azioni rispondono con il testo dedicato.
        let b = super::super::backend_for(&src(), &run);
        assert!(b.action(Scope::System, "a.service", Action::Start).is_err());
    }

    fn src_root() -> Arc<dyn Source> {
        Arc::new(FakeSrc(HashMap::new(), true))
    }

    #[test]
    fn azioni_argv_esatto() {
        let r = FakeRun::new(vec![("systemctl", 0, String::new())]);
        let b = RemoteBackend::new(src(), r.clone());
        for (a, want) in [
            (
                Action::Start,
                "systemctl --no-ask-password --no-pager --no-block start -- a.service",
            ),
            (
                Action::Stop,
                "systemctl --no-ask-password --no-pager --no-block stop -- a.service",
            ),
            (
                Action::Restart,
                "systemctl --no-ask-password --no-pager --no-block restart -- a.service",
            ),
            (
                Action::Enable,
                "systemctl --no-ask-password --no-pager enable -- a.service",
            ),
            (
                Action::Disable,
                "systemctl --no-ask-password --no-pager disable -- a.service",
            ),
        ] {
            assert_eq!(b.action(Scope::System, "a.service", a), Ok(()));
            assert_eq!(r.calls.lock().unwrap().last().unwrap(), want);
        }
    }

    #[test]
    fn azioni_nomi_non_validi_e_user_non_eseguono() {
        let r = FakeRun::new(vec![("systemctl", 0, String::new())]);
        let b = RemoteBackend::new(src(), r.clone());
        for n in ["", "a b", "a;rm -rf /", "$(x)", "a\nb\n"] {
            assert!(b.action(Scope::System, n, Action::Start).is_err(), "{n}");
        }
        assert_eq!(
            b.action(Scope::User, "a.service", Action::Stop),
            Err(ActionError::Failed(REMOTE_USER_MSG.to_string()))
        );
        assert!(r.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn azioni_errori() {
        let needs = ActionError::Failed("needs root: start ngtop with --sudo".to_string());
        for msg in [
            "Failed to start a.service: Interactive authentication required.",
            "Failed to stop a.service: Access denied",
            "Failed to enable unit: Not authorized",
        ] {
            let r = FakeRun::new(vec![("systemctl", 1, msg.to_string())]);
            let b = RemoteBackend::new(src(), r);
            assert_eq!(
                b.action(Scope::System, "a.service", Action::Start),
                Err(needs.clone())
            );
            // Con i privilegi il rifiuto resta il testo di systemctl.
            let r = FakeRun::new(vec![("systemctl", 1, msg.to_string())]);
            let b = RemoteBackend::new(src_root(), r);
            assert_eq!(
                b.action(Scope::System, "a.service", Action::Start),
                Err(ActionError::Failed(msg.to_string()))
            );
        }
        let r = FakeRun::new(vec![(
            "systemctl",
            5,
            "Failed to start x.service: Unit x.service not found.".to_string(),
        )]);
        let b = RemoteBackend::new(src(), r);
        assert_eq!(
            b.action(Scope::System, "x.service", Action::Start),
            Err(ActionError::Failed("unit not found".to_string()))
        );
        // Ultima riga significativa, senza caratteri di controllo.
        let r = FakeRun::new(vec![(
            "systemctl",
            1,
            "warning\n\x1b[31mJob failed\x07\n\n".to_string(),
        )]);
        let b = RemoteBackend::new(src(), r);
        assert_eq!(
            b.action(Scope::System, "a.service", Action::Stop),
            Err(ActionError::Failed("[31mJob failed".to_string()))
        );
    }

    #[test]
    fn il_thread_dell_interfaccia_non_si_blocca() {
        let r = Arc::new(FakeRun {
            rules: vec![("systemctl".into(), 0, "[]".into())],
            calls: Mutex::new(Vec::new()),
            delay: Duration::from_millis(400),
        });
        let backend: Arc<dyn Backend> = Arc::new(RemoteBackend::new(src(), r));
        let c = Client::spawn_with(backend, PathBuf::from("/sys/fs/cgroup"), src(), 4);
        let t = Instant::now();
        c.send(Request::Units(Scope::System));
        c.send(Request::Blame);
        // L'invio e la lettura non attendono il comando lento.
        assert!(c.try_recv().is_none());
        assert!(t.elapsed() < Duration::from_millis(100));
        let mut got = 0;
        while got < 2 && t.elapsed() < Duration::from_secs(5) {
            if let Some(r) = c.try_recv() {
                assert!(matches!(r, Response::Units(..) | Response::Blame(_)));
                got += 1;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(got, 2);
    }
}
