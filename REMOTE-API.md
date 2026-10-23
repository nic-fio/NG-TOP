# Remote monitoring — shared contract for the 5 parallel workers (temporary file, removed at the end)

Branch: `remote-ssh`. Nobody commits: the coordinator commits. Each worker edits ONLY the files it owns.
Shared interface (already written, do not change without asking): `src/source/mod.rs`
(`Source`, `LocalSource`, `FsStat`, `CommandRunner`, `LocalRunner`, `CommandOutput`, `RemoteStatus`, `RemoteState`).

## Agreed names (code against these even before they exist)

### W1 — collectors (owns `src/collect/**` except `src/collect/services/**`, and `src/ui/tz.rs`)
```rust
// src/collect/mod.rs
pub struct Roots {
    pub proc: PathBuf, pub sys: PathBuf, pub etc: PathBuf,
    pub src: Arc<dyn Source>,          // every read goes through this
    pub run: Arc<dyn CommandRunner>,   // every external command goes through this
}
impl Roots {
    pub fn local() -> Roots;                                  // /proc, /sys, /etc + LocalSource + LocalRunner (== Default)
    pub fn with_dirs(proc: PathBuf, sys: PathBuf, etc: PathBuf) -> Roots;   // fixtures, local source
    pub fn remote(src: Arc<dyn Source>, run: Arc<dyn CommandRunner>) -> Roots; // /proc, /sys, /etc on the remote side
}
impl System { pub fn remote_status(&self) -> Option<RemoteStatus>; }  // = roots.src.remote_status()
```
`System::refresh` calls `roots.src.begin_refresh()` first. The time zone (`src/ui/tz.rs`) and the user table are read
through `Source` too (remote machine's /etc/localtime, /usr/share/zoneinfo, /etc/passwd).

### W2 — transport (owns `src/remote/**`)
```rust
// src/remote/mod.rs
pub struct Target { /* user, host, port */ }
impl Target { pub fn parse(s: &str) -> Result<Target, String>; pub fn display(&self) -> String; }
    // accepts: "host", "user@host", "ssh://user@host:2222", "ssh://host"
pub struct ConnectOptions { pub compress: bool /* default true */ }
pub fn connect(t: &Target, o: &ConnectOptions) -> std::io::Result<(Arc<RemoteSource>, Arc<RemoteRunner>)>;
    // runs BEFORE the TUI (ssh may ask for a password / host key on the terminal)
pub struct RemoteSource;  // impl Source (+ remote_status)
pub struct RemoteRunner;  // impl CommandRunner, through the same ssh session
```
The ssh command is `ssh` from PATH; the environment variable `NGTOP_SSH` replaces it (it may contain arguments, split
on whitespace; e.g. `NGTOP_SSH="ssh -F /tmp/x/ssh_config"`). For tests without network, `NGTOP_SSH=local-sh` (or a
test-only constructor) runs the same protocol through a local `sh`.

### W3 — services on a remote machine (owns `src/collect/services/**`)
A backend that implements the existing services `Backend` trait through `Roots.run` (`CommandRunner`) and `Roots.src`
(cgroup files, /run/systemd/system). Chosen automatically when `System::remote_status()` is `Some`. Actions return an
error with the text: `Not available on remote machines in this version`.

### W4 — command line and interface (owns `src/config.rs`, `src/main.rs`, `src/ui/**` except `src/ui/tz.rs`, README.md)
`ngtop [options] [TARGET]` → remote when TARGET is given; also `--host TARGET` and the configuration key `host`.
Shows host and latency in the tab bar, connection state in the status line; disables signals, nice and service
actions remotely with the same text as W3.

### W5 — end-to-end tests and measurements (owns `scripts/test-remote.sh`, `tests/remote.rs`, `tests/fixtures/remote/**`)
