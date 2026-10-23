# Copyright (c) 2026 Nicola Fiorillo
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
"""User Manual, chapter 1 — what ngtop is, its five sections, the conventions of this manual.
Sources: README.md, src/ui/app.rs (Section), src/ui/view.rs, docs/decisions-and-history.md (D1, D4, D5)."""
from build import c, key, p, rif, snap, table, ui, ul, note, tip, term

S_WHAT = p("ngtop is a system monitor for the terminal. It joins in one program what <code>btop</code> shows about the "
           "CPU, the memory and the processes, what <code>nvtop</code> shows about the graphics card, and what the "
           "Windows Task Manager shows about the network, the disks and the services.", lead=True) + \
    p("It is one file. You copy it to a Linux machine and run it. There is nothing to install, no library to add and "
      "no daemon to start. It works on any distribution for the 64-bit PC (x86_64), old or new.") + \
    snap("overview", "«FIG» — The Overview screen of ngtop, the one you see at start-up", "150x42") + \
    p("ngtop is light on purpose. It uses about 1% of one CPU core at the default refresh interval, so the monitor "
      "is not the busiest process on the machine it watches.") + \
    note("ngtop only <b>shows</b> and, when you ask, <b>acts</b>: it can send a signal to a process, change the "
         "priority of a process, and start or stop a systemd service. It never does anything by itself.")

S_SECTIONS = p("The interface has five sections. You switch between them with the keys " + key("1") + " to " + key("5") +
               " or with " + key("Tab") + ", or by clicking the tab bar at the top of the screen.", lead=True) + \
    table(["Key", "Section", "What it is for", "Chapter"], [
        [key("1"), ui("Overview"), "Everything on one page: CPU, memory, GPU, network, disks and the busiest processes.", rif("Overview")],
        [key("2"), ui("GPU"), "One panel for each graphics card, with history, engines, power and the processes that use the GPU.", rif("GPU")],
        [key("3"), ui("Processes"), "The full process tree or list, with a detail panel for the selected process, signals and nice.", rif("Processes")],
        [key("4"), ui("Network & Disks"), "Interfaces, the connections and the process that owns each one, disks and mounted file systems.", rif("Network & Disks")],
        [key("5"), ui("Services"), "The systemd units, their journal, start and stop, boot time and user sessions.", rif("Services")],
    ], "«TAB» — The five sections")

S_LIMITS = p("Before you start, it helps to know what ngtop is not.", lead=True) + \
    ul(["It runs on <b>Linux x86_64</b> only. There is no version for other systems or other processors.",
        "It shows <b>the present</b>, with a short history in the graphs. It does not record data over days.",
        "It watches the machine it runs on. To watch a remote machine, run ngtop there (for example over SSH: " +
        rif("Running over SSH") + ").",
        "<b>AMD</b> and <b>Intel</b> graphics cards are read in full. <b>NVIDIA</b> cards are only detected and listed: "
        "full support is planned.",
        "The Services section needs <b>systemd</b>. Without it the section says so and the rest of ngtop works as usual.",
        "Some numbers need administrator rights. Where ngtop is not allowed to read something it shows <b>—</b>, never a "
        "wrong number (" + rif("What you see without administrator rights") + ")."])

S_CONV = p("The text of this manual uses a few simple conventions.", lead=True) + \
    table(["You see", "It means"], [
        [key("F9"), "A key to press. " + key("Ctrl", "C") + " means: hold " + key("Ctrl") + " and press " + key("C") + "."],
        [ui("Processes"), "A label exactly as it appears on the screen: a tab, a panel title, a column, a button."],
        [c("ngtop --delay 500"), "Something you type at the shell prompt, or a name from the program."],
        [c("Q") + " (capital)", "A capital letter in a key means you hold " + key("Shift") + ". The key " + key("W") + " is not the key " + key("w") + "."],
    ], "«TAB» — Conventions used in this manual") + \
    p("Examples labelled <b>Console</b> show a session at the shell prompt. The prompt is <code>$</code>; what comes "
      "after it is what you type, and the lines below are the answer.") + \
    term("""$ ngtop --version
ngtop 0.1.0
Licence: PolyForm Noncommercial 1.0.0 — free for noncommercial use""", "Console") + \
    p("The pictures of ngtop in this manual are real screens of the program, taken on a demonstration machine with "
      "sample data. On your computer the numbers and the names are of course different.") + \
    tip("The tables of keys and of options in this manual are produced by the program itself, so they always match "
        "the version you have. The same list is in the program: press " + key("F1") + ".")

CHAPTER = ("Introduction", [
    ("What ngtop is", S_WHAT),
    ("The five sections", S_SECTIONS),
    ("What ngtop does not do", S_LIMITS),
    ("Conventions of this manual", S_CONV),
])
