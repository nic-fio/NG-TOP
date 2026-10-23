# Remote actions (ngtop 1.6) — shared contract for the 5 parallel workers (temporary file, removed at the end)

Branch: `remote-actions`. Plan: `PIANO.md` (Italian). Nobody commits: the coordinator commits.
Each worker edits ONLY the files it owns. Run `cargo test` / clippy as usual (`export PATH=$HOME/.cargo/bin:$PATH`);
other workers edit other files at the same time, so a failure in a file you do not own is not yours: report it, do not fix it.

## Already written by the coordinator (do not change without asking)
- `src/source/actions.rs`: trait `ProcActions` (`live_starts`, `signal`, `set_nice`, `protected`), `LocalActions`
  (the old local code, moved from the UI), `NoActions` (refuses everything), error texts `ERR_GONE`, `ERR_REUSED`,
  `ERR_PERM`, `ERR_ACCESS`. Remote implementations MUST return these same texts.
- `Roots.act: Arc<dyn ProcActions>`: `Roots::local()`/`with_dirs` → `LocalActions`; `Roots::remote(src, run)` →
  `NoActions` (never local actions on a remote pid); `.with_actions(act)` replaces it.
- `RemoteStatus.root: bool` (script on the server runs as uid 0).
- `ConnectOptions { compress, sudo }` (`sudo` default false).
- `ngtop::remote::actions_for(&Arc<RemoteSource>) -> Arc<dyn ProcActions>` — today a stub returning `NoActions`;
  P1 implements it. `main.rs` already calls it.
- Version 1.6.0 in Cargo.toml.

## P1 — transport (`src/remote/**`; may add the `termios` feature to rustix in Cargo.toml, nothing else there)
- Agent requests (agent.sh), same record/nonce format as today, one round trip per call:
  - start times of N pids (field 22 of /proc/PID/stat: strip up to the LAST ") ", then the 20th field);
  - signal: for each `pid start`, re-read the start time and `kill -NUM pid` only if equal, in the same agent call;
    per-target result mapped to `ERR_GONE` / `ERR_REUSED` / `ERR_PERM` / other short text;
  - nice: same check, then `renice PRIORITY -p PID` (absolute value; check that it is absolute on busybox AND util-linux).
  - Validate pid/start/signal/nice as plain integers on both sides (Rust and sh); never interpolate unquoted text.
- `RemoteActions` implementing `ProcActions`; `actions_for` returns it. `protected`: PID 1 ("PID 1 (init)"), the agent's
  own `sh` and its ancestors up to (not including) PID 1, i.e. the sudo/sshd session ("the ngtop session on the server").
- `--sudo` (`ConnectOptions.sudo`): the remote command becomes sudo + sh.
  1. First try `sudo -n` (NOPASSWD).
  2. If a password is required and the session is interactive (before the TUI), ask for it on /dev/tty without echo,
     with the prompt `[sudo] password for USER@HOST: `. Provide `pub fn` for the prompt inside `src/remote`.
  3. Send it once with `sudo -S -p ''`, then WAIT for a marker printed by the shell started by sudo, BEFORE sending
     agent.sh. Wrong password, or no marker within a timeout → close stdin and kill the session, so sudo never reads
     script lines as passwords. Then return the error `sudo: wrong password` (or `sudo: a terminal is required
     (requiretty)` / `sudo: not allowed for this user` when recognisable from stderr).
  4. Reconnection (decided with the user): keep the password only in memory for the life of the process, reuse it to
     reconnect, overwrite it with zeros on drop; never on disk, never in logs, never in error texts, never in argv.
- `RemoteStatus.root` from `id -u` on the server.
- Tests in `src/remote/tests.rs` with `local-sh` (signals to a local `sleep` child, reused pid, protected pids).

## P2 — services (`src/collect/services/**`)
- `RemoteBackend::action` for System scope: `systemctl --no-ask-password <verb> -- <unit>` through `run`, behaving as
  close as possible to the local D-Bus backend (read `dbus.rs`: same verbs, same blocking/non-blocking behaviour, same
  meaning of success). The unit name check stays (already there).
- Errors: authorisation refused ("Interactive authentication required", "Access denied", polkit texts) →
  `ActionError::Failed("needs root: start ngtop with --sudo")` when the status says not root; unit not found → short
  text; otherwise the last meaningful stderr line, sanitised (no control characters).
- User scope stays `REMOTE_USER_MSG`. Remove `REMOTE_ACTION_MSG` only if nothing else uses it (P3 is removing the UI
  uses; keep the const until the end, the coordinator will clean up).
- After a successful action, the unit list/detail refreshes as for local.
- Tests with a fake `CommandRunner` (argv exactly as expected, error mapping, invalid names never run).

## P3 — interface and command line (`src/ui/**` except `src/ui/tz.rs`, `src/main.rs`, `src/config.rs`, README.md)
- Remove the "Not available on remote machines" blocks for signals, nice, F9 and service actions
  (`app.rs remote_blocked`, `processi.rs send_dialog/change_nice`, `servizi/mod.rs` keys s/t/r/e). Keep `u` blocked.
- Process actions go through `ProcActions` (from `sys.roots.act`), replacing `KillFn`/`StartFn`/`set_nice`/`protected`
  in `processi.rs`; tests use a fake `ProcActions`. Batch calls (one `live_starts`, one `signal` for a subtree).
  Nice also verifies the start time (`Process.start_ticks`).
- Remotely: confirmation dialogs name the machine ("on alex@server"); the top bar shows `root` when
  `RemoteStatus.root`; lowering nice without root → "root is needed to lower the nice value".
- `--sudo` option (only with a TARGET; error otherwise), configuration key `sudo = true`, also for `--dump`.
  `--help`, F1 help and README updated (English). Do NOT touch `docs/` (the coordinator regenerates doc.json).

## P4 — end-to-end tests (`tests/remote.rs`, `scripts/test-remote.sh`, `tests/fixtures/remote/**`)
- Containers (podman rootless, as today): users with sudo NOPASSWD and with password sudo; the systemd container with
  a harmless test service (start/stop/restart/enable/disable).
- Cases: signal and nice on a `sleep` started in the container; reused pid not hit; PID 1 protected; service actions
  with `--sudo` (both kinds of user) and the "needs root" text without it; wrong password → clear error and NO script
  line in the server's auth log/journal; requiretty if feasible.
- Actions are never run against real servers. P1–P3 work in parallel: write the tests against this contract; what
  cannot pass yet, report it — the coordinator will ask you to run everything again at the end.

## P5 — manuals and website (`docs/sources/**` except `doc.json`, `docs/decisions-and-history.md`, `docs/index.html`,
`site/landing/**`, `site/licence-page.py`)
- User manual: remote chapter (actions, `--sudo`, password, protections, what stays unavailable), processes and services
  chapters (remote notes), configuration (`sudo`), troubleshooting (sudo errors). Technical manual: action protocol,
  start-time check and its small window remotely (no pidfd on the server), sudo handshake and why the marker matters,
  password in memory only. Decisions and history: phase 2 decisions. Style as the existing chapters (NESH style).
- Website: "New in 1.6" (remote actions, `--sudo`), versions 1.6.0 (landing, ngterm.js tab bar, docs/index.html,
  licence-page CURRENT).
- Write from this contract now; at the end the coordinator will ask you to align the text with the real code and to
  run `python3 docs/sources/build.py` and `--check`.
