from build import ROOT, arrow, box, c, conta, fig, flow, note, numeri, p, rif, table, text, zone


def _tests():
    """Number of #[test] functions, counted from the sources at every build."""
    n = 0
    for f in list((ROOT / "src").rglob("*.rs")) + list((ROOT / "tests").glob("*.rs")):
        n += f.read_text(errors="replace").count("#[test]")
    return n


OVERVIEW = fig(
    zone(20, 12, 860, 262, "One process, one static executable")
    + box(40, 46, 250, 56, "Collectors", "read /proc, /sys, /etc, D-Bus", "navy")
    + box(325, 46, 250, 56, "System + App", "latest data + history", "blue")
    + box(610, 46, 250, 56, "Interface", "ratatui widgets, sections", "navy")
    + box(40, 140, 250, 56, "Linux kernel", "procfs, sysfs, DRM, cgroups", "dark")
    + box(325, 140, 250, 56, "systemd", "D-Bus: units, sessions, polkit", "dark")
    + box(610, 140, 250, 56, "Terminal", "keys, mouse, colours", "grey")
    + arrow(165, 138, 165, 104, "#475569") + arrow(450, 138, 235, 104, "#475569", True)
    + arrow(292, 74, 323, 74) + arrow(577, 74, 608, 74)
    + arrow(735, 104, 735, 138, "#475569") + arrow(735, 138, 735, 104, "#475569")
    + text(450, 232, "The collectors know nothing about the interface; the interface only reads what they have collected.",
           12, "#334155"),
    900, 290, "«FIG» — ngtop in one picture: collect, keep, draw")

S1 = p("<b>ngtop</b> is a system monitor for the terminal. It joins the processor, memory and process view of btop, "
       "the GPU view of nvtop and the network, disk and service views of a graphical task manager in one "
       "full-screen interface with five sections. It is one static executable for Linux x86_64: copy it to any "
       "distribution and it runs, with nothing to install. This manual explains how it works, for whoever wants "
       "to understand it, maintain it or extend it.", lead=True) + OVERVIEW + \
    table(["Item", "Value"], [
        ["Version", c("1.7.0-rc1") + " (" + c("Cargo.toml") + ")"],
        ["Language", "Rust, edition 2024; interface drawn with " + c("ratatui") + " 0.30 on " + c("crossterm") + " 0.29"],
        ["Target", c("x86_64-unknown-linux-musl") + ", fully static (" + rif("The static build") + ")"],
        ["Sections", "Overview, GPU, Processes, Network &amp; Disks, Services (keys " + c("1") + " to " + c("5") + ")"],
        ["Data", "Only the kernel's text files, a few system calls through " + c("rustix") + ", and D-Bus for systemd "
         "(" + rif("Data sources") + ")"],
        ["Remote machines", c("ngtop user@host") + " shows another Linux machine through one persistent " + c("ssh") + " session, with nothing installed on it; signals, nice and service actions work there too, as the login user or as root with " + c("--sudo") + " (" + rif("Remote monitoring") + ")"],
        ["Privileges", "None required. Whatever needs more (other users' file descriptors, power, GPU clients of "
         "other users) is shown as “—” when it cannot be read"],
        ["GPUs", "AMD (amdgpu), Intel (i915, xe) and NVIDIA (proprietary driver, through nvidia-smi)"],
        ["Licence", "PolyForm Noncommercial 1.0.0 (" + c("LICENSE.md") + "): free for noncommercial use"],
        ["Size", f"{conta('.rs'):,} lines of Rust, unit tests included; {_tests()} test functions"],
    ], "«TAB» — ngtop at a glance")

S2 = p("The design is the same in every file. These rules were fixed before the code was written and they explain "
       "choices that would otherwise look odd, such as a services thread that never shares a lock with the "
       "interface, or a signal dialog that checks a process twice.", lead=True) + \
    table(["Principle", "What it means in the code", "Where to see it"], [
        ["<b>Collect and draw are separate</b>", "A collector reads files and computes numbers; it has no idea that "
         "a screen exists. A view reads the numbers; it never opens a file. The two halves can be tested apart",
         c("src/collect/") + " and " + c("src/ui/") + "; " + rif("Architecture")],
        ["<b>One root for every path</b>", "Every collector receives the roots of " + c("/proc") + ", " + c("/sys")
         + " and " + c("/etc") + " as a " + c("Roots") + " value. Tests and screenshots point it at a fake tree",
         c("Roots") + " in " + c("src/collect/mod.rs")],
        ["<b>No panic on system data</b>", "A missing file, a permission error or a strange value gives " + c("None")
         + " or “—”, never a crash. Arithmetic on counters is saturating; every number that is drawn is clamped",
         rif("Conventions")],
        ["<b>Pure Rust dependencies</b>", "No crate that compiles C code, so the result is one static file",
         rif("The static build")],
        ["<b>Never wake a sleeping GPU</b>", "The metrics of a discrete card in a low-power state are not read; its clients "
         "are still counted from the files of the processes", rif("Safety")],
        ["<b>Never act on the wrong process</b>", "A signal goes to a process identified by its pid <i>and</i> its "
         "start time, through a pidfd when the kernel has one", rif("Signals: pidfd and start time")],
        ["<b>The interface never waits</b>", "Everything slow (D-Bus, subprocesses, " + c("statvfs") + " on a hung "
         "network mount) runs in another thread; the interface asks and later collects the answer",
         rif("Threads")],
        ["<b>Cheap by design</b>", "Static data is read once, slow-changing data every few cycles, expensive "
         "data only when the section that needs it is visible", rif("Performance")],
        ["<b>Say what is missing</b>", "A section without its data says so on screen (no GPU detected, systemd not "
         "found) instead of showing zeros", rif("The sections")],
    ], "«TAB» — The principles that shaped ngtop") + \
    note("Texts shown on screen are in English. Comments in the source are short and are still in Italian "
         "(" + rif("Conventions") + ").", "Language.")

S3 = p("The numbers below are counted from the sources every time this manual is built, so they cannot drift. "
       "Unit tests live next to the code they test, inside the same files, and are counted with them.", lead=True) + \
    numeri([
        ("Collectors", c("src/collect/"), "Readers of " + c("/proc") + ", " + c("/sys") + ", D-Bus; the services thread; unit tests",
         lambda f: f.startswith("src/collect/")),
        ("Interface", c("src/ui/"), "Application state, widgets, the five sections, colours, time zone; unit tests",
         lambda f: f.startswith("src/ui/")),
        ("Entry point and settings", c("src/main.rs") + ", " + c("src/lib.rs") + ", " + c("src/config.rs"),
         "Command line, diagnostics modes, the configuration file",
         lambda f: f in ("src/main.rs", "src/lib.rs", "src/config.rs")),
        ("Integration tests", c("tests/"), "Collectors and views on fake system trees",
         lambda f: f.startswith("tests/") and f.endswith(".rs")),
        ("Build and tools", c("build.sh") + ", " + c("scripts/") + ", " + c("site/"), "Static build, distribution test, "
         "third-party licence list, the web site",
         lambda f: f == "build.sh" or f.startswith(("scripts/", "site/"))),
        ("This manual", c("docs/sources/"), "The generator, the style and one file per chapter",
         lambda f: f.startswith("docs/sources/")),
    ], "«TAB» — The project in numbers") + \
    p("Besides code, " + c("tests/fixtures/") + " holds the fake system trees (snapshots of " + c("/proc") + ", "
      + c("/sys") + " and " + c("/etc") + " reduced to the files ngtop reads), and " + c("mockup/") + " holds the "
      "HTML mock-ups the interface was designed from; " + c("mockup/app.html") + " is the visual reference.")

CHAPTER = ("Overview", [
    ("What ngtop is", S1),
    ("Design principles", S2),
    ("The project in numbers", S3),
])
