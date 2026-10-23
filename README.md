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
| `--dump` | diagnostics: print a text summary and exit |

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
```

Colors are chosen from `COLORTERM`/`TERM`: truecolor if `COLORTERM` contains `truecolor` or
`24bit`, 16 colors on the `linux` console, otherwise 256.

## Known limits

- NVIDIA GPUs are only detected ("support coming soon"); AMD (amdgpu) and Intel
  (i915, xe) are supported. Suspended discrete GPUs are not queried, so as not to wake them.
- Some data needs root privileges: I/O and fds of other users' processes, power (RAPL),
  GPU usage of other users' processes. Without permissions you see "—".
- The Services section needs systemd (D-Bus); elsewhere it reports that systemd was not found.
- Linux x86_64 only.

## Licence

ngtop is released under the [PolyForm Noncommercial License 1.0.0](LICENSE.md): free for
noncommercial use. Third-party components and their licences are listed in
[THIRD-PARTY-LICENSES](THIRD-PARTY-LICENSES) (regenerate with `scripts/third-party.py`).
