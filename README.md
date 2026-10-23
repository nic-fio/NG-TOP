# ngtop

A terminal system monitor that combines a bit of btop (CPU, memory, processes), nvtop (GPU) and
the Windows Task Manager (network, disks, services) in **one static executable**.

## Requirements

None: `dist/ngtop` is a static binary for Linux x86_64 (musl). Copy it to any distribution and it
runs, with nothing to install. You need a terminal whose font has the braille characters (almost
all of them do); colors adapt on their own to 256 or 16 colors.

## Building

```
./build.sh          # produces dist/ngtop and checks that it is static
cargo test          # tests
```

You need Rust (rustup) with the `x86_64-unknown-linux-musl` target.

## Usage

```
ngtop                       # full-screen interface
ngtop -d 500 -s 3           # refresh every 500 ms, start on Processes
ngtop alex@server           # monitor a remote machine over ssh
ngtop --help                # all options
```

| Option | Meaning |
|---|---|
| `-h`, `--help` | help |
| `-V`, `--version` | version and licence |
| `-d`, `--delay MS` | refresh interval, 250–5000 ms |
| `-s`, `--section N` | start section (1–5) |
| `--no-mouse` | do not use the mouse |
| `--256` | force 256 colors |
| `--config FILE` | use a different configuration file |
| `--host TARGET` | monitor a remote machine (same as giving `TARGET`) |
| `--local` | monitor this machine even if the configuration file has `host` |
| `--sudo` | with a `TARGET`: run the commands on the server with sudo (see Remote machines) |
| `--dump [TARGET]` | diagnostics: print a text summary and exit |

Command-line options take precedence over the configuration file.

### Keys

| Key | Action |
|---|---|
| `1`–`5`, `Tab`, `Shift-Tab` | switch section: 1 Overview, 2 GPU, 3 Processes, 4 Network & Disks, 5 Services |
| `F1` | help with all the keys |
| `+` / `-` | refresh interval |
| `W` | writes only the current interval and section to the configuration file (comments and other lines stay intact) |
| `q`, `F10`, `Ctrl-C` | quit |

Per section (the full list is in `F1`):

- **Overview**: arrows select, `F6` `<` `>` sort, `i` invert, `F5` tree/list,
  `/` filter, `F9` send a signal, `n` switch network interface, `d` switch GPU.
- **GPU**: `d` switches device; table of the processes using the GPU.
- **Processes**: tree (`Space` folds or opens a branch), detail tabs (`←` `→`), `F7`/`F8` nice,
  `F9` or `k` signal.
- **Network & Disks**: `i` interface, `c` TCP/UDP, `l` listening only, Enter goes to the process,
  `F9` sends a signal to the process of the selected connection.
- **Services** (systemd): `/` search, `f` filters, `u` system/user, `s` `t` `r`
  start/stop/restart, `e` enable/disable, `j` full-screen journal (`Esc` to go back).

The mouse works too: click tabs, rows and headers; use the wheel to scroll.

### Configuration file

`$XDG_CONFIG_HOME/ngtop/config.toml` (usually `~/.config/ngtop/config.toml`). If it is missing the
defaults are used; an error is reported on stderr and the program exits with code 2.

```toml
interval = 1000          # ms, 250–5000
section = 1              # 1 Overview … 5 Services
mouse = true
colors = "auto"          # "auto", "truecolor" or "256"
temperature = "C"        # "C" or "F"
left_width = 40          # % of the left column of the Overview (20–70)
# host = "alex@server"   # monitor this machine over ssh (see Remote machines)
# sudo = true            # with host: run the remote commands with sudo (same as --sudo)
```

Colors are chosen from `COLORTERM`/`TERM`: truecolor if `COLORTERM` contains `truecolor` or
`24bit`, 16 colors on the `linux` console, otherwise 256.

## Remote machines

`ngtop [options] [TARGET]` shows another machine instead of this one. `TARGET` is `host`,
`user@host` or `ssh://user@host:port`; `--host TARGET` and the `host` key of the configuration
file do the same (the command line wins; `--local` forces this machine).

```
ngtop alex@server
ngtop --host ssh://alex@10.0.0.5:2222 -s 3
ngtop --dump alex@server            # text summary of the remote machine
ngtop --snapshot 120x40 alex@server
```

- ngtop uses your `ssh` command and configuration (keys, `~/.ssh/config`, `ProxyJump`, `known_hosts`).
  Nothing is installed on the remote machine: it only needs an ssh server and a POSIX shell with `cat`.
  Set `NGTOP_SSH` to use another ssh command (it may contain arguments).
- The connection is opened before the interface starts, so ssh can ask for a password or a host key
  on the terminal. Errors (host unreachable, authentication failed) are printed and the exit code is 2.
- The top bar shows `⇄ hostname` and the round-trip time (green below 50 ms, yellow below 200 ms,
  red above). If the connection drops, the status line says "connection lost — reconnecting…" and
  ngtop reconnects by itself.
- If an update takes longer than the refresh interval, ngtop widens the interval by one step
  (250 → 500 → 1000 → 2000 → 5000 ms) and says so in the status line.
- Signals (`F9`, `k`), nice (`F7`/`F8`) and the start/stop/restart/enable actions of system
  services work on the remote machine too, with the same confirmations as locally; the dialogs
  say which machine they act on (`On: alex@server`). The pid is checked against its start time
  right before acting, so a reused pid is never hit; PID 1 and the ngtop session on the server
  are protected. Without the right permissions the server refuses ("permission denied",
  "needs root: start ngtop with --sudo", "root is needed to lower the nice value").
  User services (`u`) stay unavailable remotely.
- `--sudo` (or `sudo = true` in the configuration file, only with a `TARGET`; it also works with
  `--dump`) runs the commands on the server with sudo, so the actions run as root; the top bar
  then shows `root`. If sudo needs a password, ngtop asks for it before the interface starts
  (not echoed). It is kept in memory only, to reconnect, and never written anywhere. `--sudo`
  without a `TARGET` is an error.

## Known limits

- GPUs: AMD (amdgpu), Intel (i915, xe) and NVIDIA (proprietary driver, read in a background
  thread every 2 s through `nvidia-smi`: card query then `pmon`, or `--query-compute-apps` for
  CUDA processes only with a note in the process table title for the selected card, retrying
  `pmon` every 30 rounds; fallback to basic fields on older drivers; `"install nvidia-smi"` if
  missing or exit code 127; 3 s timeout or error → `"nvidia-smi not responding"` with backoff up
  to 30 s, and results older than 10 s are discarded; output capped at 256 KB, control characters
  stripped from names, numbers bounded; over SSH each command occupies the session while it runs;
  not yet tested on real hardware in 1.7.0-rc1). Only awake cards are queried (`-i <bus id>`), and
  every launch goes through a small `sh` guard on the observed machine that re-reads
  `power/runtime_status` in the same shell right before running `nvidia-smi` and launches nothing
  if a card is `suspended`/`suspending` or unreadable, waiting a full 2 s round if asleep (the
  guard does not check `power_state=D3cold`, which is covered by the pre-check on the last
  refresh's data). On GeForce cards some fields are `[Not Supported]` and graphics applications
  are not always listed; the open-source `nouveau` driver is not supported.
- Some data needs root privileges: I/O and fds of other users' processes, power (RAPL),
  GPU usage of other users' processes. Without permissions you see "—".
- The Services section needs systemd (D-Bus); elsewhere it reports that systemd was not found.
- Linux x86_64 only.

## Licence

ngtop is released under the [PolyForm Noncommercial License 1.0.0](LICENSE.md): free for
noncommercial use. Third-party components and their licences are listed in
[THIRD-PARTY-LICENSES](THIRD-PARTY-LICENSES) (regenerate with `scripts/third-party.py`).
