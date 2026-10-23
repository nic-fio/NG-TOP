# Copyright (c) 2026 Nicola Fiorillo
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
"""User Manual, chapter 11 — troubleshooting.
Sources: src/main.rs (interactive terminal check, exit codes), src/ui/theme.rs (ColorMode::detect), src/ui/view.rs (draw: minimum
size), src/collect/gpu.rs (GpuState), src/ui/sections/gpu.rs (messages), src/ui/sections/servizi/ (messages),
src/collect/disk.rs (FS_TIMEOUT), src/ui/tz.rs, src/ui/mod.rs (restore_terminal)."""
from build import c, key, note, p, rif, steps, table, term, tip, ui, ul, warn

S_SMALL = p("<b>The screen is empty, or crowded.</b>", lead=True) + \
    ul(["Below <b>100 columns or 30 rows</b> ngtop uses the compact layout (" + rif("The compact layout") + "). Enlarge the window to get "
        "the full one.",
        "Below <b>10 columns or 4 rows</b> nothing can be drawn. Enlarge the window.",
        "On a narrow window some columns and the right part of the tab bar disappear. That is normal: they come back with the space."]) + \
    p("ngtop follows the resizing of the window at once; there is nothing to restart.")

S_COLOURS = p("<b>No colours, flat colours, or wrong colours.</b> ngtop decides by looking at two variables of the environment, "
              "<code>COLORTERM</code> and <code>TERM</code> (" + rif("Colours") + "). When that is wrong, tell it:", lead=True) + \
    table(["What you see", "Probable cause", "What to do"], [
        ["Colours look flat, with steps instead of smooth gradients", "The terminal has truecolor, but ngtop chose 256 colours, because <code>COLORTERM</code> is not set. This happens over SSH, inside <code>sudo</code>, and in some multiplexers.",
         "Set " + c('colors = "truecolor"') + " in the configuration file, or " + c("export COLORTERM=truecolor") + " before starting."],
        ["Garish or wrong colours, strange blocks", "ngtop chose truecolor but the terminal only has 256 colours.", "Start with " + c("ngtop --256") + ", or set " + c('colors = "256"') + "."],
        ["Only a handful of colours", "You are on the Linux text console (<code>TERM=linux</code>), which has 16 colours. ngtop is adapting.", "Nothing is wrong. A terminal emulator in a graphical session shows more."],
        ["Text is hard to read", "A very light terminal background. ngtop is designed for a dark background.", "Use a dark theme in your terminal."],
    ], "«TAB» — Colour problems") + \
    tip("Inside tmux or screen, start them with a terminal type that says 256 colours (for example <code>tmux-256color</code>) and "
        "make sure <code>COLORTERM</code> is passed through if your terminal has truecolor.")

S_BRAILLE = p("<b>The graphs show boxes, question marks or gaps.</b> The graphs are drawn with braille characters, and the frames with "
              "box-drawing characters.", lead=True) + \
    ul(["Check that the terminal uses UTF-8: <code>locale</code> should show a name ending in <code>UTF-8</code> (for example "
        "<code>en_US.UTF-8</code>). If not, set it (<code>export LANG=en_US.UTF-8</code>).",
        "Choose a font that has braille. Most monospace fonts of modern systems do (DejaVu Sans Mono, Noto Sans Mono, JetBrains Mono, "
        "Fira Code…). If the graphs have holes, change the font of the terminal.",
        "The font of the Linux text console has only a few of these characters. Use a terminal emulator in a graphical session, or "
        "read the numbers: they are always there."])

S_INTERACTIVE = p("<b>\"ngtop: an interactive terminal is required\".</b> ngtop needs a real terminal for its keys and its screen. It "
                  "refuses to start when its input or output is not one:", lead=True) + \
    term("""ngtop: an interactive terminal is required (run it from a terminal window). For a text summary use: ngtop --dump""") + \
    ul(["You ran it in a script, from <code>cron</code>, from a service, or with the output sent to a file or to a pipe (<code>ngtop | less</code>).",
        "You ran it over SSH as a command: " + c("ssh host ngtop") + ". Add <code>-t</code>: " + c("ssh -t host ngtop") + ".",
        "You want the numbers, not the screen: use " + c("ngtop --dump") + " (" + rif("A text summary with --dump") + ")."])

S_GPU = p("<b>The GPU is not shown, or shows no numbers.</b>", lead=True) + \
    table(["What you see", "Reason"], [
        [ui("No GPU detected"), "The kernel lists no graphics card: a virtual machine without GPU, a container, or a missing driver."],
        [ui("support coming soon (driver nvidia)"), "NVIDIA cards, and any card with a driver other than <code>amdgpu</code>, <code>i915</code> or <code>xe</code> (nouveau, radeon…), are only detected. See " + rif("Supported cards") + "."],
        [ui("suspended — not queried, to avoid waking it"), "A discrete card is asleep, and ngtop leaves it asleep on purpose (" + rif("Sleeping cards") + ")."],
        ["The card works but the table of processes is empty", "The processes using it belong to other users, and their GPU statistics are private. Run ngtop as root (" + rif("What you see without administrator rights") + ")."],
        ["Power or temperature are missing", "The driver does not report them. Intel cards often do not report power."],
    ], "«TAB» — GPU problems")

S_SERVICES = p("<b>The Services section does not work.</b>", lead=True) + \
    table(["What you see", "Reason and cure"], [
        [ui("Services unavailable: systemd not found"), "The machine does not use systemd (or ngtop runs in a container without it). The section cannot work here. The rest of ngtop is not affected."],
        [ui("loading…") + " for long", "systemd is busy. After ten seconds ngtop says " + ui("systemd is not responding") + ". Wait, or check the machine."],
        [ui("Permission denied: a polkit agent is needed, or run ngtop with sudo"), "You are not allowed to act on system units and nobody can ask for your password. Start ngtop with " + c("sudo") + " (" + rif("Passwords, polkit and sudo") + ")."],
        ["The journal panel is empty", "Your user is not in the group <code>systemd-journal</code>. Add it, or run as root (" + rif("The journal") + ")."],
        [ui("boot times unavailable (container?)"), "systemd has no data about the boot: a container, or a start not yet finished."],
    ], "«TAB» — Services problems")

S_NFS = p("<b>A file system says \"not responding\".</b> It is a network disk (NFS, CIFS) that does not answer, and ngtop shows it in red "
          "after five seconds (" + rif("A mount that does not respond") + "). ngtop itself keeps working. Check the server and the network. A shell "
          "that enters that folder would freeze: do not.", lead=True)

S_KEYS = p("<b>A key does nothing.</b>", lead=True) + \
    ul(["The terminal may keep some keys for itself: " + key("F1") + " (its own help), " + key("F10") + " (its menu), " + key("F11") + " (full screen). "
        "ngtop has other keys for most actions: " + key("q") + " quits, " + key("/") + " filters, " + key("<") + " and " + key(">") + " sort, " + key("k") +
        " opens the signal window in Processes.",
        "A window is open (help, signal, confirmation) and takes the keys. Close it with " + key("Esc") + ".",
        "You are typing in a filter box: every letter goes into it. Press " + key("Enter") + " or " + key("Esc") + ".",
        "Letters are case-sensitive: " + key("W") + " and " + key("w") + " are different keys.",
        "The key belongs to another section: the key bar shows the keys that work where you are."])

S_MOUSE = p("<b>The mouse does not work, or the terminal behaves oddly after ngtop.</b>", lead=True) + \
    ul(["Some terminals do not pass the mouse. Use the keyboard, or try another terminal.",
        "If the mouse prevents you from selecting text, hold " + key("Shift") + " while you drag, or start with " + c("--no-mouse") + ".",
        "ngtop restores the terminal when it leaves, even after an error or a signal like the one sent when a window closes. If ngtop is "
        "killed with <code>kill -9</code>, nothing can restore it. The terminal may then show strange characters or echo nothing. Type <code>reset</code> "
        "and press Enter (even if you do not see what you type)."])

S_TIME = p("<b>The clock or the dates are wrong.</b> ngtop shows the local time of the machine. It takes the time zone from the "
           "variable <code>TZ</code> or from <code>/etc/localtime</code>. If that file is missing or damaged, ngtop uses UTC. Check the time zone of "
           "the machine (<code>timedatectl</code>).", lead=True)

S_CPU = p("<b>ngtop uses too much CPU.</b> It normally uses about 1% of one core at the default interval. A short interval (250 ms), a very big "
          "process list or a slow link cost more. Press " + key("+") + " to refresh less often.", lead=True)

S_NUMBERS = p("<b>The numbers differ from another tool.</b> The definitions are different, not the data.", lead=True) + \
    ul(["The <b>CPU%</b> of a process is a share of the whole machine, 0–100 (top and htop count per core instead, so they show N times more on N cores).",
        "<b>Used</b> memory is the total minus the available memory; the file cache is not counted as used.",
        "Graphs and peaks start when ngtop starts; the totals of a network interface count since it was created, until you press " + key("z") + "."])

CHAPTER = ("Troubleshooting", [
    ("The terminal is too small", S_SMALL),
    ("Colours look wrong", S_COLOURS),
    ("Braille characters are not shown", S_BRAILLE),
    ("It needs an interactive terminal", S_INTERACTIVE),
    ("The GPU is not shown", S_GPU),
    ("Services does not work", S_SERVICES),
    ("A disk does not respond", S_NFS),
    ("Keys that do nothing", S_KEYS),
    ("Mouse and terminal problems", S_MOUSE),
    ("Wrong time", S_TIME),
    ("High CPU use", S_CPU),
    ("Numbers that differ from other tools", S_NUMBERS),
])
