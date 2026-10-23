# ngtop — Design decisions and project history

This document records **why** ngtop is the way it is: the decisions taken while
designing and building it, and the history of the project. It complements the
[user manual](User%20Manual.html) (what ngtop does) and the
[technical manual](Technical%20Manual.html) (how it is built).

Decisions were discussed with the project owner in Italian. The interface and
the manuals are in English (D12).

**Format of each decision:** context → decision → consequences. Status is
*accepted* unless noted otherwise.

---

## Contents

1. [Goals](#1-goals)
2. [Decisions](#2-decisions)
3. [History](#3-history)
4. [Open questions and known gaps](#4-open-questions-and-known-gaps)

---

## 1. Goals

ngtop is a system monitor for the terminal that joins what `btop`, `nvtop` and
the Windows Task Manager each show in part: CPU, memory, GPUs, processes,
network, disks and services, in one program. Working rules:

- **One file to copy.** A single executable, with nothing to install on the
  machine that runs it (D1).
- **Look before building.** The interface was drawn as HTML mockups and chosen
  by the owner before the Rust code was written (D4).
- **Light on the machine it watches.** A monitor must not be the busiest
  process (D9).
- **Careful with actions.** Signals and service commands act on the real
  system, so they are checked at the moment they are sent (D7, D8).

---

## 2. Decisions

### D1. One static binary

- **Context.** A monitor is most useful on a machine that is already in
  trouble, where installing packages may not be possible or wise.
- **Decision.** Rust built for `x86_64-unknown-linux-musl` and linked
  statically (`./build.sh` produces `dist/ngtop`, checked with `ldd`). It runs
  on any Linux x86_64. `scripts/test-distro.sh` runs it in containers; it was
  tested on Ubuntu 24.04, Debian 12, Debian 10, Fedora 41, Arch, Alpine 3.20,
  openSUSE Tumbleweed and CentOS 7.
- **Consequences.** Anything that needs a dynamic library is out of reach (D2,
  D5). Where an old kernel lacks a call, ngtop has a fallback (D7).

### D2. Rust with ratatui and crossterm; pure-Rust dependencies only

- **Context.** A crate that compiles C code pulls in a C library and a C
  toolchain, and breaks the static build.
- **Decision.** The interface uses `ratatui` and `crossterm`. System calls go
  through `rustix`. D-Bus is spoken by `zbus` instead of `libsystemd`. Every
  dependency must be pure Rust.
- **Consequences.** The rule is written in `CLAUDE.md`. The licence notices of
  the included crates are collected in `THIRD-PARTY-LICENSES`.

### D3. Read the kernel interfaces directly

- **Decision.** The collectors read `/proc`, `/sys` and the DRM `fdinfo` files
  themselves. They call no external tool, need no daemon and, for the common
  case, no root. Missing data or a denied permission give "—", never a panic.
  The root of `/proc` and `/sys` is a parameter, so the tests run on fake trees
  in `tests/fixtures/`.

### D4. The interface: five sections

- **Context.** Ten HTML mockups were drawn in `mockup/`.
- **Decision.** The owner chose five sections: **Overview** (mockup 01) as the
  main page, **GPU** (04), **Processes** as a tree (07), **Network & Disks**
  (08) and **Services** (09). The compact layout (05) is not a section: it is
  used automatically when the terminal is narrower than 100 columns or lower
  than 30 rows.
- **Consequences.** `mockup/app.html` is the visual reference. The other
  mockups were not built as pages of their own.

### D5. GPUs: AMD and Intel first, NVIDIA detected only (superseded by D26)

- **Decision.** AMD (`amdgpu`) and Intel (`i915`, `xe`) are read fully. NVIDIA
  cards are detected and listed, nothing more.
- **Reason.** NVIDIA's data comes from NVML, a dynamic library that a static
  binary cannot load cleanly (D1).
- **Consequences.** Full NVIDIA support was planned; it arrived in 1.7.0-rc1 (D26).

### D6. Never wake a sleeping discrete GPU

- **Context.** Reading the sysfs files of a discrete GPU in runtime power
  management wakes it, which costs power and defeats the point of letting it
  sleep.
- **Decision.** A suspended discrete GPU is shown as suspended and none of its
  device files is read. Integrated GPUs are always read: their runtime PM
  switches on and off continuously while they are in use, so there is nothing
  to save.

### D7. Signals that cannot reach the wrong process

- **Context.** A PID can be reused between the moment the user selects a
  process and the moment the signal is sent.
- **Decision.** ngtop opens a `pidfd` (`pidfd_open`), re-reads the start time of
  the process from `/proc/PID/stat`, compares it with the one on screen, and
  only then calls `pidfd_send_signal`; the pidfd stays bound to that process
  even if the PID is reused. The subtree is recomputed and verified at the
  moment of sending. The subtrees of PID 1 and PID 2 are forbidden, because they
  are nearly the whole system. A second confirmation is asked above 50
  processes and for processes of other users. Without `pidfd` (kernel older than
  5.3, or no permission) ngtop checks and then uses `kill`.
- **Consequences.** Failures are reported in words ("the process no longer
  exists", "the pid now belongs to another process").

### D8. Services through D-Bus in a worker thread

- **Decision.** systemd is queried over D-Bus by a worker thread, so the screen
  never waits for it. Queries have a time limit of 10 s; actions (which may wait
  for polkit authentication) 120 s. The journal is read with `journalctl`, run
  with a time limit (15 s).
- **Consequences.** A stuck systemd shows up as a timeout, not a frozen screen.

### D9. Performance budget

- **Decision.** About 1 % of one CPU at the default 1 s refresh. Command lines,
  user names and file descriptors are cached between cycles; `statvfs` runs in
  threads, so a hung NFS mount cannot freeze the screen.
- **Consequences.** Measured on an Intel N100 with `--bench` and `--bench-start`:
  about 8–11 ms per cycle and about 25–30 ms to the first frame; about 1 % CPU
  at the default 1 s refresh and about 5.6 MB of RAM.

### D10. Time zone read from the system files

- **Decision.** The local time zone comes from `TZ` or `/etc/localtime`, parsed
  by an in-house TZif (v1, v2, v3) parser with no C library. It is hardened
  against corrupt files (inconsistent counts are rejected); on any error the
  zone is UTC.
- **Consequences.** Dates are shown as `2026-10-20 07:30`.

### D11. Colours

- **Decision.** The palette is truecolor, taken from the mockups. When the
  terminal needs it, ngtop converts to 256 or 16 colours. The mode comes from
  the setting or, failing that, from `COLORTERM` and `TERM`.

### D12. English interface and manuals; licence

- **Decision.** The interface, the help (F1), the messages and the manuals are
  in English, like the author's other projects. Code comments are in Italian.
  The licence is PolyForm Noncommercial 1.0.0; commercial licences on request
  at ngtop@nicfio.it. Source files carry an SPDX header.

### D13. Configuration

- **Decision.** `~/.config/ngtop/config.toml` is read by a small hand-written
  parser (no TOML crate), enough for the few keys ngtop has. The **W** key
  writes the current interval and section (the other lines are kept)
  atomically (temporary file, `sync`, `rename`).

### D14. Website

- **Decision.** https://ngtop.nicfio.it. The owner chose the direction
  "Daylight" among ten proposals; the others were deleted. The terminal on the
  page is live, with sample data.
- **Consequences.** The site is published by `site/publish.sh`.

### D15. Remote machines over ssh, without agents

- **Context.** The owner wanted to monitor servers without installing anything
  on them.
- **Decision.** `ngtop [user@]host` reads the remote `/proc` and `/sys` through
  one persistent ssh session. Nothing is installed or written on the server;
  it needs only an ssh server and a POSIX shell (busybox included).

### D16. The system ssh command, not an ssh library

- **Decision.** ngtop runs the `ssh` command of the system. The user keeps their
  keys, agent, `~/.ssh/config`, `ProxyJump` and `known_hosts`.
- **Reason.** The ssh libraries for Rust need cryptography written in C, which
  would break the static binary (D1, D2).

### D17. One request per refresh, with a learned manifest

- **Context.** Opening a new ssh connection costs about 0.6 s.
- **Decision.** The session stays open and each refresh asks for all the files
  at once. Reads are bulk, one process per group (`grep -H`, `wc` + `cat`,
  `ls -lq`), to keep the load on the server under about 1 % CPU.

### D18. Read-only first

- **Decision.** In 1.5 signals, nice and service actions are disabled on remote
  machines; they come in a later phase (D21–D25, version 1.6).
- **Reason.** The `pidfd` protection of D7 does not exist across ssh.

### D19. Defensive protocol

- **Decision.** The end of each reply is marked by a random 128-bit nonce chosen
  by the client for each session and never put on a command line, so it cannot
  appear in the data. Every argument is quoted, memory is limited (the reply
  size has a cap), every block is validated, and hostile data never causes a
  panic.
- **Reason.** A hostile process on the server must not be able to forge the
  data of others.

### D20. Version 1.5

- **Decision.** The owner chose 1.5 for the release that adds remote machines.

### D21. Actions on remote machines (1.6)

- **Context.** After 1.5 the owner wanted to do on a server what one does
  locally: signals, nice, and service actions.
- **Decision.** Signals and nice go through one interface (`ProcActions`) with
  a local and a remote implementation; service actions use `systemctl` through
  the same ssh connection. The confirmations and protections stay the ones of
  the local case.
- **Consequences.** The remote machine never receives an action built from a
  bare PID (D22). User units stay unavailable remotely: ssh does not reach that
  bus.

### D22. Start-time check instead of pidfd

- **Context.** `pidfd` (D7) pins a process, but it does not exist across ssh and
  not on every system (busybox).
- **Decision.** ngtop sends the start time it knows together with the PID. The
  script on the server reads the start time again and runs `kill` (or `renice`)
  only if the two are equal, in one command.
- **Consequences.** A PID reused by another program is never hit. A window of a
  few milliseconds between the check and the `kill` remains; it is stated in the
  manuals, and it needs thousands of new processes in that instant to matter.

### D23. `--sudo` and the marker

- **Decision.** `--sudo` starts the script on the server under `sudo`: first
  without a password (`sudo -n`), then, if needed, with the password asked on the
  terminal before the interface opens. After sending the password ngtop waits
  for a marker printed by the shell that sudo started, and only then sends the
  script.
- **Reason.** If the password were wrong, sudo would read the following lines as
  password attempts, and pieces of the script would end up in the server's logs.
  Without the marker nothing of the script is sent.

### D24. The sudo password stays in memory

- **Decision.** The password lives only in the memory of ngtop, so that a
  reconnection after a network drop needs no new question. It is cleared when
  ngtop exits and is never written to disk, to the configuration, to logs or to
  a command line.
- **Alternative rejected.** Not keeping it and asking to restart ngtop after
  every drop: safer on paper, but a poor experience on unstable links.

### D25. Version 1.6.0

- **Decision.** 1.6.0: a new function that does not change how the program is
  used.

### D26. NVIDIA through nvidia-smi, version 1.7.0-rc1

- **Context.** NVIDIA's proprietary driver publishes its numbers only through
  NVML, a C shared library (D5, D1).
- **Decision.** ngtop reads NVIDIA cards by running `nvidia-smi` (installed with
  the driver) from a separate thread every ~2 s, and parsing its text output as
  hostile input. A sleeping card is never queried (D6). If `nvidia-smi` is
  missing the card is only listed; if it times out (3 s) the state is "not
  responding" and ngtop retries with a growing pause. The same code works on
  remote machines, through `ssh`. The release is a candidate (`1.7.0-rc1`)
  because it has been tested only on sample outputs, not on a real card.
- **Alternative rejected.** Binding NVML: it is a C library, it would bring a
  dynamic dependency back and break the single static executable.
- **Consequences.** A process is started every ~2 s. On GeForce cards some
  fields are "Not Supported" and graphics processes may not be listed (only CUDA
  ones). The final 1.7.0 waits for the checklist of tests on real hardware in the
  technical manual.

---

## 3. History

All work took place on 2026-10-23, in phases. Every step of the plan ended with
an independent review before the next began.

| Phase | What happened |
|---|---|
| **1. Mockups** | Ten HTML mockups of the interface in `mockup/`; the owner chose five sections (D4). |
| **2. Plan** | A written plan in eight steps, approved by the owner. |
| **3. Steps 1–8** | 1 collectors; 2 TUI frame; 3 Overview; 4 GPU; 5 Processes; 6 Network & Disks; 7 Services; 8 finishing touches. |
| **4. Parallel work** | Steps 3–5 were written by three programmers in parallel, each on a separate copy of the project, and joined with a three-way merge. |
| **5. Distributions** | Tests on eight distributions with `scripts/test-distro.sh` (D1). |
| **6. Website** | Ten proposals, direction "Daylight" chosen; site published (D14). |
| **7. English and manuals** | Interface moved to English, licence set (D12), user and technical manuals written, with this document. |
| **8. Remote machines (1.5.0)** | Evening of 2026-10-23. A plan for remote monitoring; the `Source` interface written by the coordinator; five programmers in parallel on collectors, ssh transport, remote services, command line and interface; end-to-end tests on containers (Alpine, Debian, Debian with systemd) and on two real hosts. An independent review found four security issues (forged fd links through file names with newlines, shifted command lines, panics and unbounded memory with a hostile server); all were fixed and verified again (D15–D20). |
| **9. Actions on remote machines (1.6.0)** | Phase 2 of the remote work. A common interface for process actions written by the coordinator; five programmers in parallel on transport, services, interface and command line, end-to-end tests, and manuals and site; tests in containers with sudo (with and without password) and with systemd (D21–D25). |
| **10. NVIDIA (1.7.0-rc1)** | Support for NVIDIA cards through `nvidia-smi` (D26). The public types of the collector were written by the coordinator; three programmers in parallel on the output parsers, the link to the collector with its tests, and interface, manuals and site. Tested on sample outputs only; the tests on real hardware are listed in the technical manual. |

---

## 4. Open questions and known gaps

| Topic | Status |
|---|---|
| NVIDIA | Read through `nvidia-smi` (D26); release candidate, not yet tested on a real card. |
| Architectures | x86_64 only. |
| Remote machines | Signals, nice and service actions work (D21); user units stay unavailable, and the start-time check has a window of a few milliseconds (D22). |
| Real hardware | Measured on an Intel N100 machine and tested in containers on eight distributions; other hardware, especially other GPUs, is unverified. |
