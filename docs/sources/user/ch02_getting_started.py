# Copyright (c) 2026 Nicola Fiorillo
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
"""User Manual, chapter 2 — requirements, getting the file, first run, SSH, --dump.
Sources: README.md, build.sh, src/main.rs (interactive, dump), src/config.rs (parse_args, USAGE),
src/ui/theme.rs (ColorMode::detect), src/ui/mod.rs (terminal restore)."""
from build import c, key, note, ol, p, rif, steps, table, term, tip, ui, ul, warn

S_REQ = p("ngtop asks very little of the machine and of the terminal.", lead=True) + \
    table(["Requirement", "Details"], [
        ["Operating system", "Linux, on a 64-bit PC (x86_64). Any distribution, old or new: the file does not depend on "
         "any library of the system."],
        ["A terminal with UTF-8", "ngtop draws its graphs with <b>braille characters</b> and its frames with box-drawing "
         "characters. Almost every modern terminal and font has them. The terminal must use a UTF-8 locale."],
        ["Colours (optional)", "ngtop uses 24-bit colours (truecolor) when the terminal has them. Otherwise it uses 256 "
         "colours, or 16 on the Linux text console. It chooses by itself (" + rif("Colours") + ")."],
        ["Size", "At least 100 columns by 30 rows for the full layout. A smaller terminal still works, with the compact "
         "layout (" + rif("The compact layout") + ")."],
        ["Mouse (optional)", "If the terminal reports the mouse, you can click and scroll. Everything can also be done "
         "with the keyboard."],
    ], "«TAB» — What ngtop needs") + \
    note("Administrator rights are <b>not</b> required. A normal user sees almost everything. Only some details of "
         "other users' processes need root (" + rif("What you see without administrator rights") + ").")

S_GET = p("ngtop is a single file named <code>ngtop</code>.", lead=True) + \
    p("Download it from the project site, <code>https://ngtop.nicfio.it</code>. If you prefer to build it yourself you "
      "need Rust (installed with <code>rustup</code>) with the <code>x86_64-unknown-linux-musl</code> target; then, in "
      "the folder of the sources:") + \
    term("""$ sh build.sh
$ ls -l dist/ngtop""") + \
    p("<code>build.sh</code> produces <code>dist/ngtop</code> and checks that the file is really static, that is, that "
      "it needs no library. The technical manual describes the build in detail.") + \
    p("A downloaded file is not yet executable. Make it so, once:") + \
    term("""$ chmod +x ngtop""") + \
    p("You can leave the file where it is and start it as <code>./ngtop</code>, or copy it to a folder that is in your "
      "<code>PATH</code> to start it just as <code>ngtop</code>. For example, for yourself only:") + \
    term("""$ mkdir -p ~/.local/bin
$ cp ngtop ~/.local/bin/""") + \
    tip("To see which version you have, and the licence, run " + c("ngtop --version") + ". For the list of options, "
        "run " + c("ngtop --help") + ".")

S_FIRST = p("Open a terminal window and run the program:", lead=True) + \
    term("""$ ngtop""") + \
    p("The screen fills with the <b>Overview</b>. The numbers appear within a fraction of a second; the first CPU "
      "percentages are measured over a short first interval, then they follow the refresh interval (one second by "
      "default).") + \
    p("Try these first steps. None of them changes anything.") + \
    steps(["Press " + key("2") + " to see the GPU, " + key("3") + " for the processes, " + key("4") + " for network and disks, " +
           key("5") + " for the services, and " + key("1") + " to come back.",
           "Press " + key("F1") + ". A window lists all the keys. Close it with " + key("Esc") + ".",
           "In the Overview, press " + key("↓") + " a few times to select a process, then " + key("F6") + " to change the "
           "column used for sorting.",
           "Press " + key("/") + " and type a few letters of a program name: the list shows only the matching processes. "
           "Press " + key("Esc") + " to clear the filter.",
           "Press " + key("q") + " to leave."])

S_QUIT = p("Three ways to leave ngtop: " + key("q") + ", " + key("F10") + " and " + key("Ctrl", "C") + ". On exit ngtop gives the "
           "terminal back exactly as it was: the screen, the cursor and the mouse. This also happens if ngtop is "
           "stopped by a signal (such as the closing of the terminal window) or if it meets an error.", lead=True) + \
    p("A confirmation window (for a signal or for a service action) takes the keys first. In that case " + key("Esc") +
      " closes the window, and then " + key("q") + " leaves the program.")

S_SSH = p("ngtop is a good tool for a server you reach over SSH: it is a single file, so you can copy it there.", lead=True) + \
    term("""$ scp ngtop user@server:
$ ssh user@server
$ chmod +x ngtop
$ ./ngtop""") + \
    p("If you give the program as a command, ask SSH for a terminal with <code>-t</code>:") + \
    term("""$ ssh -t user@server ./ngtop""") + \
    p("Without <code>-t</code> there is no terminal, and ngtop says so instead of drawing garbage:") + \
    term("""$ ssh user@server ./ngtop
ngtop: an interactive terminal is required (run it from a terminal window). For a text summary use: ngtop --dump""") + \
    ul(["<b>Colours.</b> SSH often does not pass the <code>COLORTERM</code> variable, so ngtop falls back to 256 colours. "
        "That looks fine. If your terminal has truecolor and you want it, set " + c('colors = "truecolor"') + " in the "
        "configuration file on the server (" + rif("The configuration file") + ").",
        "<b>Slow links.</b> Press " + key("+") + " to refresh less often: each press lengthens the interval, up to "
        "5 seconds.",
        "<b>Inside tmux or screen</b> ngtop works as in any terminal. If the colours look flat, try " + c("ngtop --256") + "."])

S_DUMP = p("Sometimes there is no terminal at all: a script, a log, a bug report. Then use " + c("--dump") + ". It prints a "
           "short text summary and exits, without drawing the interface.", lead=True) + \
    term("""$ ngtop --dump
CPU: AMD Ryzen 9 7900X 12-Core Processor (24 logical, 12 physical)
  total  18.2%
  core  0   22.0%  4710 MHz
  core  1   14.3%  4650 MHz
  ...
Memory: total 31.20 GiB used 9.84 GiB available 21.36 GiB cache 11.02 GiB buffers 0.31 GiB free 9.10 GiB
Swap: total 8.00 GiB used 0.00 GiB free 8.00 GiB
Load: 0.52 0.61 0.58  processes 2/611
Uptime: 4d 11:20:07
    PID USER         CPU%        MEM COMMAND
   2210 alice         6.1   0.42 GiB /usr/lib/firefox/firefox
   1534 alice         3.4   0.21 GiB /usr/bin/gnome-shell
    ...""", "Console") + \
    p("The summary has the CPU with one line for each core, the memory and the swap, the load, the uptime, and the 15 "
      "processes that use the most CPU. It is also the quickest way to check that ngtop works on a new machine.") + \
    note("<code>--dump</code> is a diagnostic, not a monitor: it measures over half a second and prints once.")

CHAPTER = ("Getting started", [
    ("Requirements", S_REQ),
    ("Getting the file", S_GET),
    ("First run", S_FIRST),
    ("Leaving ngtop", S_QUIT),
    ("Running over SSH", S_SSH),
    ("A text summary with --dump", S_DUMP),
])
