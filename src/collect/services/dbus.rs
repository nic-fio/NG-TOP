// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Backend reale: D-Bus (zbus, API bloccante, Rust puro) per systemd e login1;
//! sottoprocessi per `journalctl` e `systemd-analyze blame`.

use std::collections::HashMap;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use zbus::blocking::{Connection, Proxy};
use zbus::proxy::{CacheProperties, MethodFlags};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

use super::{
    Action, ActionError, Backend, BootTimestamps, JournalLine, Scope, SessionInfo, UnitDetail,
    UnitInfo, classify_error, parse_blame, parse_journal_json,
};

const SYSTEMD: &str = "org.freedesktop.systemd1";
const MANAGER_PATH: &str = "/org/freedesktop/systemd1";
const MANAGER_IFACE: &str = "org.freedesktop.systemd1.Manager";
const LOGIN1: &str = "org.freedesktop.login1";

type Props = HashMap<String, OwnedValue>;
type UnitTuple = (
    String,
    String,
    String,
    String,
    String,
    String,
    OwnedObjectPath,
    u32,
    String,
    OwnedObjectPath,
);

/// `ListUnitFiles` costa centinaia di ms (systemd scandisce le cartelle): si tiene in cache.
const UNIT_FILES_TTL: Duration = Duration::from_secs(60);

/// Tempo massimo per le interrogazioni D-Bus.
const QUERY_TIMEOUT: Duration = Duration::from_secs(10);
/// Tempo massimo per le azioni: polkit può aspettare la password dell'utente.
const ACTION_TIMEOUT: Duration = Duration::from_secs(120);
/// Il PID principale si rilegge se cambia lo stato secondario o dopo questo tempo.
/// Tempo massimo per `journalctl`: poi il figlio viene terminato.
const JOURNAL_TIMEOUT: Duration = Duration::from_secs(15);
const PID_TTL: Duration = Duration::from_secs(10);

#[derive(Clone)]
struct SvcEntry {
    cgroup: Option<String>,
    pid: Option<u32>,
    sub_state: String,
    pid_at: Instant,
}

type UnitFileCache = HashMap<Scope, (Instant, HashMap<String, String>)>;

pub struct DbusBackend {
    /// Connessioni per (ambito, lunga): le "lunghe" servono alle azioni (polkit può attendere).
    conns: Mutex<HashMap<(Scope, bool), Connection>>,
    unit_files: Arc<Mutex<UnitFileCache>>,
    unit_files_busy: Arc<AtomicBool>,
    /// Cgroup e PID principale dei servizi già visti.
    svc_cache: Mutex<HashMap<String, SvcEntry>>,
}

impl Default for DbusBackend {
    fn default() -> Self {
        Self::new()
    }
}

/// Come `Command::output` ma con tempo massimo: scaduto, kill + wait e errore `TimedOut`.
fn run_with_timeout(cmd: &mut Command, limit: Duration) -> std::io::Result<std::process::Output> {
    use std::io::Read;
    use std::process::Stdio;
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut so = child.stdout.take();
    let mut se = child.stderr.take();
    let t_out = std::thread::spawn(move || {
        let mut v = Vec::new();
        if let Some(s) = so.as_mut() {
            let _ = s.read_to_end(&mut v);
        }
        v
    });
    let t_err = std::thread::spawn(move || {
        let mut v = Vec::new();
        if let Some(s) = se.as_mut() {
            let _ = s.read_to_end(&mut v);
        }
        v
    });
    let start = Instant::now();
    let status = loop {
        if let Some(st) = child.try_wait()? {
            break st;
        }
        if start.elapsed() >= limit {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::ErrorKind::TimedOut.into());
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    Ok(std::process::Output {
        status,
        stdout: t_out.join().unwrap_or_default(),
        stderr: t_err.join().unwrap_or_default(),
    })
}

fn es(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Percorso D-Bus di un'unità (`foo.service` → `/org/freedesktop/systemd1/unit/foo_2eservice`).
pub fn unit_object_path(name: &str) -> String {
    let mut s = String::from("/org/freedesktop/systemd1/unit/");
    for b in name.bytes() {
        if b.is_ascii_alphanumeric() {
            s.push(b as char);
        } else {
            s.push_str(&format!("_{b:02x}"));
        }
    }
    s
}

fn as_u64(v: &Props, k: &str) -> Option<u64> {
    match &**v.get(k)? {
        Value::U64(x) => Some(*x),
        Value::U32(x) => Some(u64::from(*x)),
        Value::I64(x) => u64::try_from(*x).ok(),
        Value::I32(x) => u64::try_from(*x).ok(),
        _ => None,
    }
}

/// u64::MAX significa "non impostato / non disponibile" per memoria e task.
fn as_limited(v: &Props, k: &str) -> Option<u64> {
    as_u64(v, k).filter(|&x| x != u64::MAX)
}

fn as_str(v: &Props, k: &str) -> String {
    match v.get(k).map(|x| &**x) {
        Some(Value::Str(s)) => s.to_string(),
        _ => String::new(),
    }
}

fn as_strs(v: &Props, k: &str) -> Vec<String> {
    match v.get(k).map(|x| &**x) {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|x| match x {
                Value::Str(s) => Some(s.to_string()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn non_empty(s: String) -> Option<String> {
    (!s.is_empty()).then_some(s)
}

impl DbusBackend {
    pub fn new() -> Self {
        DbusBackend {
            conns: Mutex::new(HashMap::new()),
            unit_files: Arc::new(Mutex::new(HashMap::new())),
            unit_files_busy: Arc::new(AtomicBool::new(false)),
            svc_cache: Mutex::new(HashMap::new()),
        }
    }

    fn conn(&self, scope: Scope) -> Result<Connection, String> {
        self.conn_with(scope, false)
    }

    fn conn_with(&self, scope: Scope, long: bool) -> Result<Connection, String> {
        let mut g = self
            .conns
            .lock()
            .map_err(|_| "invalid internal state".to_string())?;
        if let Some(c) = g.get(&(scope, long)) {
            return Ok(c.clone());
        }
        let timeout = if long { ACTION_TIMEOUT } else { QUERY_TIMEOUT };
        let builder = match scope {
            Scope::System => zbus::blocking::connection::Builder::system(),
            Scope::User => zbus::blocking::connection::Builder::session(),
        };
        let c = builder
            .and_then(|b| b.method_timeout(timeout).build())
            .map_err(|e| match scope {
                Scope::System => format!("system bus unreachable: {e}"),
                Scope::User => format!("session bus unavailable: {e}"),
            })?;
        g.insert((scope, long), c.clone());
        Ok(c)
    }

    fn system_conn(&self) -> Result<Connection, String> {
        self.conn(Scope::System)
    }

    /// Una proprietà del tipo `org.freedesktop.systemd1.Service`.
    fn get_prop(conn: &Connection, path: &str, prop: &str) -> Option<OwnedValue> {
        let msg = conn
            .call_method(
                Some(SYSTEMD),
                path,
                Some("org.freedesktop.DBus.Properties"),
                "Get",
                &("org.freedesktop.systemd1.Service", prop),
            )
            .ok()?;
        msg.body().deserialize::<OwnedValue>().ok()
    }

    fn get_all(conn: &Connection, dest: &str, path: &str, iface: &str) -> Result<Props, String> {
        let msg = conn
            .call_method(
                Some(dest),
                path,
                Some("org.freedesktop.DBus.Properties"),
                "GetAll",
                &(iface,),
            )
            .map_err(es)?;
        msg.body().deserialize::<Props>().map_err(es)
    }

    /// Stato di avvio per nome file. `ListUnitFiles` è lento (centinaia di ms): non si aspetta,
    /// si risponde con la cache (anche scaduta o vuota) e la si aggiorna in un thread a parte.
    fn unit_file_states(&self, conn: &Connection, scope: Scope) -> HashMap<String, String> {
        let Ok(g) = self.unit_files.lock() else {
            return HashMap::new();
        };
        let cached = g.get(&scope);
        let fresh = cached.is_some_and(|(at, _)| at.elapsed() < UNIT_FILES_TTL);
        let out = cached.map(|(_, m)| m.clone()).unwrap_or_default();
        drop(g);
        if !fresh && !self.unit_files_busy.swap(true, Ordering::AcqRel) {
            let (conn, cache, busy) = (
                conn.clone(),
                Arc::clone(&self.unit_files),
                Arc::clone(&self.unit_files_busy),
            );
            let spawned = std::thread::Builder::new()
                .name("ngtop-unitfiles".into())
                .spawn(move || {
                    let map = Self::fetch_unit_files(&conn);
                    if let (Some(map), Ok(mut g)) = (map, cache.lock()) {
                        g.insert(scope, (Instant::now(), map));
                    }
                    busy.store(false, Ordering::Release);
                });
            if spawned.is_err() {
                self.unit_files_busy.store(false, Ordering::Release);
            }
        }
        out
    }

    fn fetch_unit_files(conn: &Connection) -> Option<HashMap<String, String>> {
        let mgr = Self::manager_nocache(conn).ok()?;
        let files: Vec<(String, String)> = mgr.call("ListUnitFiles", &()).ok()?;
        Some(
            files
                .into_iter()
                .filter_map(|(path, st)| Some((path.rsplit('/').next()?.to_string(), st)))
                .collect(),
        )
    }

    /// Proxy del Manager senza cache delle proprietà (nessuna sottoscrizione ai segnali).
    fn manager_nocache(conn: &Connection) -> Result<Proxy<'static>, String> {
        zbus::blocking::proxy::Builder::new(conn)
            .destination(SYSTEMD)
            .map_err(es)?
            .path(MANAGER_PATH)
            .map_err(es)?
            .interface(MANAGER_IFACE)
            .map_err(es)?
            .cache_properties(CacheProperties::No)
            .build()
            .map_err(es)
    }
}

impl Backend for DbusBackend {
    fn list_units(&self, scope: Scope) -> Result<Vec<UnitInfo>, String> {
        let conn = self.conn(scope)?;
        let mgr = Self::manager_nocache(&conn)?;
        let units: Vec<UnitTuple> = mgr.call("ListUnits", &()).map_err(es)?;
        let states = self.unit_file_states(&conn, scope);
        let mut out = Vec::with_capacity(units.len());
        let mut cache = self.svc_cache.lock().map(|g| g.clone()).unwrap_or_default();
        for (name, description, load, active, sub, _follow, opath, ..) in units {
            // Istanze di template (`foo@bar.service`) hanno lo stato in `foo@.service`.
            let template = name
                .split_once('@')
                .and_then(|(a, b)| b.rsplit_once('.').map(|(_, ext)| format!("{a}@.{ext}")));
            let unit_file_state = states
                .get(&name)
                .or_else(|| template.as_ref().and_then(|t| states.get(t)))
                .cloned()
                .unwrap_or_default();
            let mut info = UnitInfo {
                name,
                description,
                load_state: load,
                active_state: active,
                sub_state: sub,
                unit_file_state,
                main_pid: None,
                cgroup: None,
            };
            // PID principale e cgroup: solo per i servizi attivi. `GetAll` costa parecchio a
            // systemd, quindi due `Get` mirati; il cgroup resta in cache finché l'unità vive.
            if info.name.ends_with(".service")
                && matches!(
                    info.active_state.as_str(),
                    "active" | "activating" | "reloading"
                )
            {
                let old = cache.get(&info.name);
                let fresh = old
                    .is_some_and(|e| e.sub_state == info.sub_state && e.pid_at.elapsed() < PID_TTL);
                let (pid, pid_at) = match old {
                    Some(e) if fresh => (e.pid, e.pid_at),
                    _ => (
                        Self::get_prop(&conn, opath.as_str(), "MainPID")
                            .and_then(|v| v.downcast_ref::<u32>().ok())
                            .filter(|&x| x > 0),
                        Instant::now(),
                    ),
                };
                let cgroup = match old.and_then(|e| e.cgroup.clone()) {
                    Some(c) => Some(c),
                    None => Self::get_prop(&conn, opath.as_str(), "ControlGroup")
                        .and_then(|v| v.downcast_ref::<String>().ok())
                        .and_then(non_empty),
                };
                info.main_pid = pid;
                info.cgroup = cgroup.clone();
                cache.insert(
                    info.name.clone(),
                    SvcEntry {
                        cgroup,
                        pid,
                        sub_state: info.sub_state.clone(),
                        pid_at,
                    },
                );
            }
            out.push(info);
        }
        out.sort_by_cached_key(|u| u.name.to_lowercase());
        cache.retain(|k, _| out.iter().any(|u| &u.name == k));
        if let Ok(mut g) = self.svc_cache.lock() {
            *g = cache;
        }
        Ok(out)
    }

    fn unit_detail(&self, scope: Scope, name: &str) -> Result<UnitDetail, String> {
        let conn = self.conn(scope)?;
        let path = unit_object_path(name);
        let u = Self::get_all(&conn, SYSTEMD, &path, "org.freedesktop.systemd1.Unit")?;
        let kind = name.rsplit('.').next().unwrap_or("");
        let mut d = UnitDetail {
            name: name.to_string(),
            description: as_str(&u, "Description"),
            active_state: as_str(&u, "ActiveState"),
            sub_state: as_str(&u, "SubState"),
            since_usec: as_u64(&u, "StateChangeTimestamp").filter(|&t| t > 0),
            requires: as_strs(&u, "Requires"),
            wants: as_strs(&u, "Wants"),
            fragment_path: as_str(&u, "FragmentPath"),
            ..Default::default()
        };
        if matches!(kind, "service" | "socket" | "mount" | "swap" | "scope") {
            let iface = format!(
                "org.freedesktop.systemd1.{}{}",
                kind[..1].to_uppercase(),
                &kind[1..]
            );
            if let Ok(p) = Self::get_all(&conn, SYSTEMD, &path, &iface) {
                d.main_pid = as_u64(&p, "MainPID")
                    .filter(|&x| x > 0)
                    .and_then(|x| u32::try_from(x).ok());
                d.n_restarts = as_u64(&p, "NRestarts").and_then(|x| u32::try_from(x).ok());
                d.restart = as_str(&p, "Restart");
                d.mem_current = as_limited(&p, "MemoryCurrent");
                d.mem_peak = as_limited(&p, "MemoryPeak");
                d.cpu_nsec = as_limited(&p, "CPUUsageNSec");
                d.tasks = as_limited(&p, "TasksCurrent");
                d.tasks_max = as_limited(&p, "TasksMax");
                d.cgroup = non_empty(as_str(&p, "ControlGroup"));
            }
        }
        Ok(d)
    }

    fn action(&self, scope: Scope, name: &str, action: Action) -> Result<(), ActionError> {
        let fail = |e: String| classify_error(&e);
        let conn = self.conn_with(scope, true).map_err(ActionError::Failed)?;
        let mgr = Self::manager_nocache(&conn).map_err(ActionError::Failed)?;
        // Flag D-Bus "autorizzazione interattiva": polkit può chiedere la password all'agente.
        let flags = MethodFlags::AllowInteractiveAuth;
        if matches!(action, Action::Enable | Action::Disable)
            && let Ok(mut g) = self.unit_files.lock()
        {
            // Cache scaduta: il prossimo elenco la rilegge (intanto mostra i valori vecchi).
            for (at, _) in g.values_mut() {
                *at = at.checked_sub(UNIT_FILES_TTL).unwrap_or(*at);
            }
        }
        let call_job = |method: &str| -> Result<(), ActionError> {
            mgr.call_with_flags::<_, _, OwnedObjectPath>(method, flags.into(), &(name, "replace"))
                .map(|_| ())
                .map_err(|e| fail(e.to_string()))
        };
        match action {
            Action::Start => call_job("StartUnit"),
            Action::Stop => call_job("StopUnit"),
            Action::Restart => call_job("RestartUnit"),
            Action::Enable => {
                mgr.call_with_flags::<_, _, (bool, Vec<(String, String, String)>)>(
                    "EnableUnitFiles",
                    flags.into(),
                    &(vec![name], false, false),
                )
                .map_err(|e| fail(e.to_string()))?;
                mgr.call_with_flags::<_, _, ()>("Reload", flags.into(), &())
                    .map(|_| ())
                    .map_err(|e| fail(e.to_string()))
            }
            Action::Disable => {
                mgr.call_with_flags::<_, _, Vec<(String, String, String)>>(
                    "DisableUnitFiles",
                    flags.into(),
                    &(vec![name], false),
                )
                .map_err(|e| fail(e.to_string()))?;
                mgr.call_with_flags::<_, _, ()>("Reload", flags.into(), &())
                    .map(|_| ())
                    .map_err(|e| fail(e.to_string()))
            }
        }
    }

    fn boot_times(&self) -> Result<BootTimestamps, String> {
        let conn = self.system_conn()?;
        let p = Self::get_all(&conn, SYSTEMD, MANAGER_PATH, MANAGER_IFACE)?;
        let g = |k: &str| as_u64(&p, k).unwrap_or(0);
        Ok(BootTimestamps {
            firmware: g("FirmwareTimestampMonotonic"),
            loader: g("LoaderTimestampMonotonic"),
            initrd: g("InitRDTimestampMonotonic"),
            userspace: g("UserspaceTimestampMonotonic"),
            finish: g("FinishTimestampMonotonic"),
            kernel_realtime: g("KernelTimestamp"),
        })
    }

    fn sessions(&self) -> Result<Vec<SessionInfo>, String> {
        let conn = self.system_conn()?;
        let msg = conn
            .call_method(
                Some(LOGIN1),
                "/org/freedesktop/login1",
                Some("org.freedesktop.login1.Manager"),
                "ListSessions",
                &(),
            )
            .map_err(es)?;
        let list: Vec<(String, u32, String, String, OwnedObjectPath)> =
            msg.body().deserialize().map_err(es)?;
        let mut out = Vec::new();
        for (id, uid, user, seat, path) in list {
            let p = Self::get_all(
                &conn,
                LOGIN1,
                path.as_str(),
                "org.freedesktop.login1.Session",
            )
            .unwrap_or_default();
            out.push(SessionInfo {
                id,
                user,
                uid,
                seat,
                tty: as_str(&p, "TTY"),
                kind: as_str(&p, "Type"),
                remote_host: as_str(&p, "RemoteHost"),
                desktop: as_str(&p, "Desktop"),
                since_usec: as_u64(&p, "Timestamp").unwrap_or(0),
            });
        }
        Ok(out)
    }

    fn journal(&self, scope: Scope, name: &str, n: usize) -> Result<Vec<JournalLine>, String> {
        let mut cmd = Command::new("journalctl");
        cmd.arg(match scope {
            Scope::System => "-u",
            Scope::User => "--user-unit",
        })
        .arg(name)
        .arg("-n")
        .arg(n.to_string())
        .args(["-o", "json", "--no-pager", "-q"]);
        let out = run_with_timeout(&mut cmd, JOURNAL_TIMEOUT).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "journalctl not found".to_string()
            } else if e.kind() == std::io::ErrorKind::TimedOut {
                "journal too slow".to_string()
            } else {
                e.to_string()
            }
        })?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            return Err(err
                .lines()
                .next()
                .unwrap_or("journalctl failed")
                .to_string());
        }
        Ok(parse_journal_json(&String::from_utf8_lossy(&out.stdout)))
    }

    fn blame(&self) -> Result<Vec<(String, f64)>, String> {
        let out = Command::new("systemd-analyze")
            .args(["blame", "--no-pager"])
            .output()
            .map_err(es)?;
        if !out.status.success() {
            return Err("systemd-analyze blame unavailable".to_string());
        }
        Ok(parse_blame(&String::from_utf8_lossy(&out.stdout)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Prova manuale su un sistema con systemd: `cargo test reale -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn reale_dettaglio_e_sessioni() {
        let b = DbusBackend::new();
        let d = b.unit_detail(Scope::System, "ssh.service").unwrap();
        println!("{d:#?}");
        println!("{:#?}", b.sessions());
        println!("{:#?}", b.boot_times());
        println!("{:?}", b.journal(Scope::System, "ssh.service", 3));
        println!("{:?}", b.list_units(Scope::User).map(|v| v.len()));
        let _ = b.list_units(Scope::System);
        std::thread::sleep(Duration::from_secs(1));
        let t = std::time::Instant::now();
        let n = b.list_units(Scope::System).map(|v| v.len());
        println!("list_units system: {n:?} in {:?}", t.elapsed());
    }

    #[test]
    fn tempo_massimo_dei_sottoprocessi() {
        let t = Instant::now();
        let e = run_with_timeout(Command::new("sleep").arg("5"), Duration::from_millis(200))
            .unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::TimedOut);
        assert!(t.elapsed() < Duration::from_secs(2));
        let o = run_with_timeout(Command::new("echo").arg("ciao"), Duration::from_secs(5)).unwrap();
        assert_eq!(String::from_utf8_lossy(&o.stdout).trim(), "ciao");
    }

    #[test]
    fn percorso_oggetto() {
        assert_eq!(
            unit_object_path("docker.service"),
            "/org/freedesktop/systemd1/unit/docker_2eservice"
        );
        assert_eq!(
            unit_object_path("dev-sda2.device"),
            "/org/freedesktop/systemd1/unit/dev_2dsda2_2edevice"
        );
    }
}
