import re

from build import ROOT, c, code, file_map, p, rif, sorgenti, table, term, tree, note, sysf


def manual_rows():
    """The manual's own sources: the generator, the style, the script and one file per chapter, with its title."""
    rows = []
    for f in sorted(f for f in sorgenti() if f.startswith("docs/sources/")):
        name = f.rsplit("/", 1)[-1]
        if f.endswith("/build.py"):
            role = ("Generates the two manuals: text, table and SVG figure helpers, numbering, the file map and "
                    "the numbers, the screenshots, and the " + c("--check") + " checks")
        elif name == "style.css":
            role = "The common style sheet every manual page starts with"
        elif name == "manual.js":
            role = "The script embedded in the page: sidebar, search, figure zoom"
        else:
            m = re.search(r'^CHAPTER = \("([^"]+)"', (ROOT / f).read_text(), re.M) if (ROOT / f).is_file() else None
            n = re.match(r"ch(\d\d)_", name)
            kind = "Technical Manual" if "/technical/" in f else "User Manual" if "/user/" in f else "Manual"
            role = (f"{kind}, chapter {int(n.group(1))}: " if n else f"{kind}: ") + (m.group(1) if m else "chapter source")
        rows.append((f, role))
    return rows


GROUPS = [
    ("Entry point and settings", [
        ("src/main.rs", "Starts ngtop: reads the command line, runs the interface, or one of the diagnostic modes "
         "(" + c("--dump") + ", " + c("--snapshot") + ", " + c("--bench") + ", " + c("--bench-start") + ", " + c("--doc-json") + ")"),
        ("src/lib.rs", "Library root: declares " + c("collect") + ", " + c("config") + " and " + c("ui") + ", so the tests "
         "can use the program as a crate"),
        ("src/config.rs", "Command line, the configuration file (a small TOML subset), merging of the two, atomic "
         "saving with " + c("W")),
    ]),
    ("Reading sources and remote machines", [
        ("src/source/mod.rs", "The " + c("Source") + " and " + c("CommandRunner") + " traits, their local implementations, " + c("RemoteStatus") + ": the single place through which the collectors read"),
        ("src/source/actions.rs", "The " + c("ActionBackend") + " trait and " + c("LocalActions") + ": signals and nice locally and remotely"),
        ("src/remote/mod.rs", c("connect()") + ", " + c("ConnectOptions") + ", the ssh command (" + c("NGTOP_SSH") + ")"),
        ("src/remote/agent.sh", "The POSIX shell script sent to the remote machine on stdin and run in memory: the reader that answers the requests"),
        ("src/remote/session.rs", "The persistent ssh session, the nonce, the record protocol, " + c("sh_quote()")),
        ("src/remote/source.rs", c("RemoteSource") + " and " + c("RemoteRunner") + ": manifest, caches, bulk reads, reconnection"),
        ("src/remote/actions.rs", c("RemoteActions") + ": signals and nice on the remote machine, with start-time check"),
        ("src/remote/sudo.rs", "Running the remote agent under " + c("sudo") + ": password prompt on the terminal, sentinel and error classification"),
        ("src/remote/target.rs", "The " + c("TARGET") + " argument: parsing and validation"),
        ("src/remote/tests.rs", "Unit tests of the protocol and of the remote source, with a local " + c("sh")),
    ]),
    ("Collectors: src/collect/", [
        ("src/collect/mod.rs", c("System") + ", the state of the whole machine, and " + c("Roots") + "; "
         + c("refresh()") + " runs every collector in order"),
        ("src/collect/cpu.rs", "CPU times from " + c("/proc/stat") + ", model and topology, frequencies"),
        ("src/collect/mem.rs", "Memory and swap from " + c("/proc/meminfo")),
        ("src/collect/load.rs", "Load average and uptime"),
        ("src/collect/sensors.rs", "CPU temperature: which hwmon or thermal zone to read"),
        ("src/collect/process.rs", "Every process from " + c("/proc/[pid]") + ": CPU%, caches, tree order, sorting"),
        ("src/collect/procdetail.rs", "The extra detail of the one selected process (threads, files, sockets, "
         "environment, limits, cgroup)"),
        ("src/collect/net.rs", "Interfaces, rates, routes, addresses, DNS"),
        ("src/collect/disk.rs", "Disks (IOPS, latency, queue), mounts, and the " + c("statvfs") + " worker threads"),
        ("src/collect/conn.rs", "TCP/UDP connections and the socket-inode to pid cache"),
        ("src/collect/gpu.rs", "GPU discovery, amdgpu / i915 / xe metrics, per-process use from " + c("fdinfo") + ", and NVIDIA integration"),
        ("src/collect/nvidia.rs", "Parsers of " + c("nvidia-smi") + " output (" + c("--query-gpu") + ", " + c("pmon") + ", " + c("--query-compute-apps") + "), field fallback, hostile-input limits"),
        ("src/collect/nvidia_worker.rs", c("NvClient") + " and the " + c("ngtop-nvidia") + " worker thread: " + c("sh") + " guard for sleeping cards, 3 s timeout, backoff"),
        ("src/collect/services/mod.rs", "systemd data model, the " + c("Backend") + " trait, boot phases, parsers of "
         "journal and blame output"),
        ("src/collect/services/dbus.rs", "The real backend: D-Bus through " + c("zbus") + ", subprocesses with a time limit"),
        ("src/collect/services/worker.rs", "The services thread and its request / response channels"),
        ("src/collect/services/cgroup.rs", "Memory and CPU% of a unit, read straight from its cgroup"),
        ("src/collect/services/remote.rs", "The remote backend: systemd through commands, no D-Bus; service actions with systemctl"),
        ("src/collect/services/fake.rs", "A fake backend with demo data, for tests and screenshots"),
    ]),
    ("Interface: src/ui/", [
        ("src/ui/mod.rs", "Terminal set-up and restore, signal flags, the event loop"),
        ("src/ui/app.rs", c("App") + ": all interface state, key and mouse handling, histories"),
        ("src/ui/view.rs", "Draws one frame: tab bar, the section, key bar, status line, help and dialogs"),
        ("src/ui/keys.rs", "The key tables: bottom bars and the F1 help. The single source, also used by " + c("--doc-json") + ""),
        ("src/ui/doc.rs", "The hidden " + c("--doc-json") + " output: keys and command-line options as JSON, for the manuals"),
        ("src/ui/history.rs", "Fixed-size ring buffer of samples"),
        ("src/ui/fmt.rs", "Number formatting: bytes, rates, uptime, temperature"),
        ("src/ui/theme.rs", "Colours, gradients, and the conversion to 256 and 16 colours"),
        ("src/ui/tz.rs", "Local time zone in pure Rust (TZ variable, TZif files)"),
        ("src/ui/snapshot.rs", "Renders one frame on a test backend, as text or ANSI colours"),
        ("src/ui/widgets/mod.rs", "Safe writing into the buffer (" + c("put") + ", " + c("putn") + ", " + c("clip") + ")"),
        ("src/ui/widgets/braille.rs", "The braille graph"),
        ("src/ui/widgets/meter.rs", "The bar of filled cells"),
        ("src/ui/widgets/sparkline.rs", "The one-line sparkline"),
        ("src/ui/widgets/panel.rs", "The panel with a rounded border and titles on the border"),
        ("src/ui/widgets/proctable.rs", "Sortable table with fixed and flexible columns"),
        ("src/ui/widgets/tabbar.rs", "The top line: brand, tabs, host information"),
        ("src/ui/widgets/keybar.rs", "The bottom key bar"),
        ("src/ui/widgets/dialog.rs", "The centred dialog window"),
        ("src/ui/sections/mod.rs", "Declares the five sections"),
        ("src/ui/sections/panoramica.rs", "Section 1, Overview, large and compact layouts"),
        ("src/ui/sections/gpu.rs", "Section 2, GPU, and the GPU pieces the Overview reuses"),
        ("src/ui/sections/processi.rs", "Section 3, Processes: table, detail tabs, signals, nice, folding"),
        ("src/ui/sections/retedischi.rs", "Section 4, Network &amp; Disks"),
        ("src/ui/sections/servizi/mod.rs", "Section 5, Services: state, scheduling of requests, keys"),
        ("src/ui/sections/servizi/draw.rs", "Drawing of the Services section"),
    ]),
    ("Tests", [
        ("tests/cli.rs", "The program without a terminal: help, version, refusal to start"),
        ("tests/collect.rs", "Collectors on two fake snapshots of the system"),
        ("tests/gpu.rs", "GPU collectors and views on the GPU fixtures"),
        ("tests/retedischi.rs", "Network, disks and connections: collectors, views, keys"),
        ("tests/servizi.rs", "The Services section with a fake backend"),
        ("tests/ui.rs", "Overview at many sizes, many-core machines"),
        ("tests/remote.rs", "The remote protocol through a local " + c("sh") + ", compared with the local read, on the real machine and on a fixture"),
        ("tests/hostile.rs", "Security tests against a hostile remote server, fake " + c("sudo") + " programs and hostile " + c("nvidia-smi") + " output"),
        ("tests/source_only.rs", "The collectors read only through " + c("Source") + ": an in-memory source, and a scan of the sources"),
    ]),
    ("Build, tools and web site", [
        ("Cargo.toml", "Dependencies, release profile (LTO, one codegen unit, strip)"),
        ("build.sh", "Static build and the check that the result has no dynamic dependencies"),
        ("scripts/test-distro.sh", "Runs the static binary in containers of several distributions"),
        ("scripts/test-remote.sh", "End-to-end tests of " + c("ngtop TARGET") + " against containers with sshd (Alpine, Debian, Debian with systemd); with " + c("--measure") + ", the measurements"),
        ("scripts/measure-remote.sh", "Shortcut for " + c("test-remote.sh --measure")),
        ("scripts/third-party.py", "Writes the list of third-party licences of the crates compiled into the binary"),
        ("site/publish.sh", "Builds and publishes the web site"),
        ("site/licence-page.py", "Builds the licence page from " + c("LICENSE.md")),
        ("site/landing/index.html", "The landing page"),
        ("site/landing/ngterm.css", "Style of the demonstration terminal on the page"),
        ("site/landing/ngterm.js", "The demonstration terminal"),
        ("site/landing/tui.js", "The interface drawing code of the demonstration"),
    ]),
]


def full_map():
    """The groups above, limited to files that exist in the sources, plus the manual's own sources.
    Anything the generator counts and this list does not know goes to a last group, so no file is lost."""
    known = sorgenti()
    groups = [(g, [(f, r) for f, r in rows if f in known]) for g, rows in GROUPS]
    groups.append(("This manual", manual_rows()))
    listed = {f for _, rows in groups for f, _ in rows}
    rest = sorted(set(known) - listed)
    if rest:
        groups.append(("Other files", [(f, "—") for f in rest]))
    return [(g, rows) for g, rows in groups if rows]


S1 = p("Everything is in one Cargo package. The library holds the collectors and the interface; the program is a "
       "thin " + c("main.rs") + " on top of it, so the integration tests can call the same functions the "
       "program does.", lead=True) + tree([
    "NG-TOP/",
    "├── Cargo.toml  # one package, release profile tuned for size and speed",
    "├── build.sh  # static build + check",
    "├── src/",
    "│   ├── main.rs  # entry point, diagnostic modes",
    "│   ├── lib.rs  # library root",
    "│   ├── config.rs  # command line and configuration file",
    "│   ├── source/  # Source, CommandRunner: how reads are made",
    "│   ├── remote/  # ssh session, protocol, RemoteSource, agent.sh",
    "│   ├── collect/  # collectors: no interface code",
    "│   │   ├── mod.rs  # System, Roots",
    "│   │   ├── cpu.rs  mem.rs  load.rs  sensors.rs",
    "│   │   ├── process.rs  procdetail.rs",
    "│   │   ├── net.rs  disk.rs  conn.rs  gpu.rs  nvidia.rs  nvidia_worker.rs",
    "│   │   └── services/  # systemd: backend, worker thread, cgroup, fake",
    "│   └── ui/  # interface: ratatui",
    "│       ├── mod.rs  app.rs  view.rs  keys.rs  doc.rs  history.rs  fmt.rs  theme.rs  tz.rs  snapshot.rs",
    "│       ├── widgets/  # Panel, BrailleGraph, Meter, Sparkline, ProcTable, TabBar, KeyBar, Dialog",
    "│       └── sections/  # one file per section; servizi/ is a folder",
    "├── tests/  # integration tests",
    "│   └── fixtures/  # fake /proc, /sys, /etc trees; remote/ has the Containerfiles",
    "├── scripts/  # test-distro.sh, test-remote.sh, measure-remote.sh, third-party.py",
    "├── site/  # landing page, publishing, licence page",
    "├── mockup/  # HTML mock-ups of the interface (visual reference)",
    "├── docs/  # these manuals",
    "│   └── sources/  # generator, style, chapters",
    "└── dist/  # the finished binary (not in the repository)",
], "«TAB» — The repository, folder by folder") + \
    note("Section files and some module names keep their original Italian names (" + c("panoramica.rs") + ", "
         + c("processi.rs") + ", " + c("retedischi.rs") + ", " + c("servizi/") + "). The screen names are English: "
         "Overview, Processes, Network &amp; Disks, Services. The correspondence is in "
         + rif("Glossary") + ".", "Names.")

S2 = p("Four tools are enough to build and test ngtop. Rust is installed with " + c("rustup") + " in "
       + sysf("~/.cargo/bin") + "; add it to " + c("PATH") + " if it is missing.", lead=True) + \
    table(["Command", "What it does"], [
        [c("cargo build"), "Debug build for the machine's own libc, for quick tries"],
        [c("cargo test"), "All unit and integration tests"],
        [c("cargo clippy --all-targets -- -D warnings"), "Lint; warnings are errors"],
        [c("cargo fmt"), "Formatting"],
        [c("build.sh"), "Release build for " + c("x86_64-unknown-linux-musl") + ", copy to " + c("dist/ngtop") + ", check that it is static"],
        [c("scripts/test-distro.sh"), "Runs " + c("dist/ngtop") + " in containers of several distributions " + "(" + rif("Testing") + ")"],
        [c("scripts/test-remote.sh"), "End-to-end tests of remote monitoring against containers with sshd; " + c("--measure") + " (or " + c("scripts/measure-remote.sh") + ") takes the measurements (" + rif("Remote measurements") + ")"],
        [c("scripts/third-party.py"), "Regenerates the list of third-party licences"],
        [c("python3 docs/sources/build.py"), "Regenerates these manuals (" + c("--check") + " verifies they match the code)"],
    ], "«TAB» — The commands of the project") + \
    p("<b>What </b>" + c("build.sh") + "<b> does.</b> It stops at the first error (" + c("set -euo pipefail") + "), puts "
      + sysf("~/.cargo/bin") + " on the path, runs " + c("cargo build --release --target x86_64-unknown-linux-musl")
      + ", copies the result to " + c("dist/ngtop") + " (after removing the old file, so a running copy does not give "
      "“text file busy”), and finally runs " + c("ldd") + " on it. If " + c("ldd") + " does not say “not a dynamic "
      "executable” or “statically linked”, the script prints the dependencies and exits with an error. Only then is "
      "the file called static: see " + rif("The static build") + ".") + \
    p("<b>The release profile</b> (" + c("Cargo.toml") + "): " + c("opt-level = 3") + ", " + c("lto = \"fat\"") + " (the "
      "whole program is optimised together), " + c("codegen-units = 1") + " and " + c("strip = true") + " (no symbols). "
      "It is slower to compile and makes the file smaller and faster.") + \
    term("""$ bash build.sh
...
OK: dist/ngtop is static (3.9M)""", "The last line of a successful build")

S3 = p("Each folder outside " + c("src/") + " has one job.", lead=True) + \
    table(["Folder", "Job"], [
        [c("scripts/"), c("test-distro.sh") + " starts the binary in containers (podman, rootless) of eight distributions "
         "and checks exit codes and output; " + c("third-party.py") + " reads " + c("cargo tree") + " and "
         + c("cargo metadata") + " and writes the licence list of the crates in the binary"],
        [c("site/"), "The web site of the project: the landing page with a demonstration terminal, the licence page "
         "built from " + c("LICENSE.md") + ", and " + c("publish.sh") + " that builds " + c("site/public/")
         + " and uploads it. The manuals of " + c("docs/") + " are published with it"],
        [c("mockup/"), "HTML pages from which the interface was designed. " + c("mockup/tui.js") + " holds the "
         "original gradients; " + c("src/ui/theme.rs") + " repeats them in Rust"],
        [c("tests/fixtures/"), "Fake system trees, described in " + rif("Testing")],
        [c("docs/"), "The two manuals, " + c("decisions-and-history.md") + " (why ngtop is the way it is) and the "
         "generator in " + c("docs/sources/")],
        [c("dist/"), "Output of " + c("build.sh") + ". Not part of the sources"],
        [c("target/"), "Cargo's build folder. Not part of the sources"],
    ], "«TAB» — The folders outside src")

S4 = p("Every source file with its line count. The counts are taken from the files at every build of the manual. A "
       "file that exists but is not described here would appear under “Other files” at the end, with a dash "
       "instead of a role: that is the sign that this list needs a new line.", lead=True) + \
    file_map(full_map(), "«TAB» — The file map")

CHAPTER = ("Source tree and tools", [
    ("The repository", S1),
    ("Tools and build commands", S2),
    ("Folders outside src", S3),
    ("The file map", S4),
])
