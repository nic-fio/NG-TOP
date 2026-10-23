# Copyright (c) 2026 Nicola Fiorillo
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
"""User Manual, chapter 8 — the Services section (systemd).
Sources: src/ui/sections/servizi/mod.rs (Chip, on_key, ask_confirm, start_label), servizi/draw.rs (layout, detail, journal,
boot, users & sessions), src/collect/services/ (Action, DENIED_TEXT, boot_phases, systemd_present, worker, dbus),
docs/decisions-and-history.md (D8)."""
from build import c, key, keys_table, note, p, rif, snap, steps, sysf, table, term, tip, ui, ul, warn

S_SCREEN = p("The Services section is a window on <b>systemd</b>, the program that starts and supervises the services of most "
             "Linux distributions. It lists the units, shows what each one is doing and what it wrote in the journal, and lets "
             "you start, stop and restart it.", lead=True) + \
    snap("services", "«FIG» — The Services section on a large terminal", "182x52") + \
    p("On a wide terminal (100 columns or more) the list of units is on the left and the details of the selected unit on the "
      "right. On a narrow terminal the details go under the list. On a large one (at least 120 columns by 38 rows) two more "
      "panels appear at the bottom: the boot time and the users with their sessions.") + \
    note("All the questions to systemd are asked from a separate thread. If systemd is slow, the screen does not freeze: it says " +
         ui("loading…") + ", or, after ten seconds without an answer, " + ui("systemd is not responding") + ".")

S_LIST = p("The list shows one unit on each line.", lead=True) + \
    table(["Column", "Meaning"], [
        [ui("Unit"), "The name, such as <code>cron.service</code>, <code>sshd.socket</code> or <code>apt-daily.timer</code>."],
        [ui("Description"), "What it is, in a few words. Hidden on a narrow panel."],
        [ui("State"), "What it is doing now (see below)."],
        [ui("Startup"), "Whether it starts at boot: " + ui("enabled") + ", " + ui("disabled") + ", " + ui("static") +
         " (it cannot be enabled, it is started by others), " + ui("masked") + " (blocked), " + ui("indirect") + ", " +
         ui("generated") + ", " + ui("transient") + ", " + ui("alias") + ", " + ui("linked") + ". " + ui("enabled (rt)") +
         " means enabled until the next reboot."],
        [ui("PID"), "The main process of the unit."],
        [ui("Mem") + ", " + ui("CPU%"), "Memory and CPU used by the whole unit, including its children."],
    ], "«TAB» — Columns of the unit list") + \
    p("The state has a symbol and a colour:") + \
    table(["Symbol", "State", "Meaning"], [
        [ui("●") + " green", ui("active"), "Running, or mounted, or listening."],
        [ui("○") + " grey", ui("inactive"), "Not running."],
        [ui("✖") + " red", ui("failed"), "It ended with an error. Look at the journal."],
        [ui("◷") + " yellow", ui("waiting"), "A timer that waits for its time."],
        [ui("◔") + " yellow", ui("starting") + " / " + ui("stopping"), "In transition."],
    ], "«TAB» — States of a unit")

S_FILTERS = p("A real system has hundreds of units. Along the top of the list there are six <b>filters</b>, each with the number of "
              "units it matches:", lead=True) + \
    term("""All 280  Active 176  Failed 0  Enabled 37  Timers 8  Sockets 19""") + \
    ul([key("f") + " goes to the next filter: " + ui("All") + ", " + ui("Active") + ", " + ui("Failed") + ", " + ui("Enabled") + ", " +
        ui("Timers") + ", " + ui("Sockets") + ". The chosen one is highlighted.",
        key("/") + " opens a search box in the title of the list. It looks in the name and in the description, ignoring case. " +
        key("Enter") + " keeps the search, " + key("Esc") + " clears it. The search and the filter work together: you can look for " +
        c("ssh") + " among the " + ui("Active") + " units only.",
        "A quick way to see what is wrong: press " + key("f") + " until " + ui("Failed") + " is highlighted."]) + \
    tip("The " + ui("Failed") + " counter is the first thing to read on a server after a reboot. If it is not zero, select the failed "
        "unit and read its journal.")

S_USER = p("There are two worlds of units. At the right end of the filter line you see " + ui("‹ system │ user ›") + ". Press " + key("u") +
           " to switch.", lead=True) + \
    ul(["<b>system</b>: the units of the machine, started at boot and run as root: the web server, the network, the clock. ",
        "<b>user</b>: the units of <b>your</b> login, started by your own systemd instance: the sound server, a sync tool, a timer you "
        "wrote. They need no password."]) + \
    p("When you switch, the list is read again from the other side.")

S_DETAIL = p("The panel on the right is named after the selected unit. It shows:", lead=True) + \
    table(["Line", "Meaning"], [
        [ui("State"), "The state, the sub-state (<code>running</code>, <code>exited</code>, <code>mounted</code>) and since when, with "
         "how long ago: <code>since 2026-10-20 07:30 (3d 6h ago)</code>."],
        [ui("PID"), "The main process, its name, the number of tasks in the unit and the limit."],
        [ui("Memory") + ", " + ui("CPU"), "Memory now and its peak, and the CPU time used since the unit started."],
        [ui("Depends"), "The units it needs."],
        [ui("Restarts"), "How many times systemd restarted it, and the restart policy (<code>Restart=on-failure</code>)."],
    ], "«TAB» — The detail of a unit") + \
    p("Below these lines is the end of the unit's <b>journal</b>, the log that systemd keeps. Its heading is "
      "<code>journal (journalctl -u NAME)</code>, or <code>--user-unit</code> for a user unit. It is read again every five seconds.") + \
    p("When the detail panel is too small for all the lines, the least important ones are dropped first.")

S_JOURNAL = p("To read the journal at length, press " + key("j") + ". The journal of the selected unit takes the whole screen, titled " +
      ui("journal · NAME") + ". Press " + key("j") + " or " + key("Esc") + " to go back.", lead=True) + \
    warn("To read the journal your user must be allowed to: normally by being a member of the group " + c("systemd-journal") + ", or by running "
         "ngtop as root. Otherwise the panel says <code>(journal empty or unreadable: the systemd-journal group is needed)</code>.",
         "The journal needs permission.")

S_ACTIONS = p("Four keys act on the selected unit:", lead=True) + \
    table(["Key", "Action", "Result"], [
        [key("s"), "Start", "Starts the unit."],
        [key("t"), "Stop", "Stops it."],
        [key("r"), "Restart", "Stops and starts it again."],
        [key("e"), "Enable / Disable", "If the unit starts at boot, the key disables it; if not, it enables it. The label tells which."],
    ], "«TAB» — Actions on a unit") + \
    p("Nothing happens at once. A window asks for confirmation:") + \
    snap("services", "«FIG» — The confirmation window for a restart", "120x36", overlay="confirm") + \
    p("The window shows the unit, the scope (<code>system</code> or <code>user</code>) and a reminder that the system may ask for a "
      "password. Two buttons: " + ui("Cancel") + " and " + ui("Confirm") + ". <b>Cancel is selected</b>, so pressing " + key("Enter") +
      " by habit does nothing. Move to " + ui("Confirm") + " with " + key("←") + " " + key("→") + " or " + key("Tab") + ", then " + key("Enter") +
      ". " + key("Esc") + " or " + key("n") + " cancel.") + \
    p("The result is written on the lower border of the list: " + ui("Restart requested: cron.service") + ", " + ui("Enabled: NAME") +
      " or the error in red. The list and the detail are read again immediately, so you see the new state. Only one action is "
      "handled at a time.") + \
    note("<b>Stop</b> and <b>restart</b> act on a real service. Stopping <code>NetworkManager</code> on a remote machine cuts you off. "
         "Think before you press " + key("Enter") + ".", "Be careful.")

S_PASSWORD = p("ngtop does not ask for passwords and does not keep any. When you act on a <b>system</b> unit, ngtop asks systemd, and "
               "systemd asks <b>polkit</b> whether you are allowed. What happens next depends on where you are.", lead=True) + \
    table(["Situation", "What happens"], [
        ["In a desktop session (GNOME, KDE…), in a terminal window", "A polkit window of the desktop opens and asks for your password. After it, "
         "the action goes through."],
        ["Over SSH, or on a text console, without a desktop", "There is nobody to ask. The action is refused and the message on the lower border of the list says " +
         ui("Permission denied: a polkit agent is needed, or run ngtop with sudo") + "."],
        ["As root, or with " + c("sudo ngtop"), "No question. The action goes through."],
        ["On a user unit", "No password is needed, because the units are yours."],
    ], "«TAB» — Passwords and actions") + \
    p("So on a server, the practical way is to start ngtop with sudo when you need to act on services, and without it for just "
      "looking:") + \
    term("""$ sudo ngtop --section 5""") + \
    p("<b>On a remote machine</b> there is no polkit window: if the user you log in as cannot manage services, ngtop says <code>needs root: start ngtop with --sudo</code>. "
      "Start it as " + c("ngtop --sudo alex@server") + " to act as root on the server (" + rif("Acting as root: --sudo") + ").") + \
    tip("Running everything as root is not needed. The monitoring never requires it (" + rif("Permissions and running as root") + ").")

S_BOOT = p("On a large terminal the bottom left panel is <b>boot</b>. Its title says when the machine last started and how long the "
           "start took: <code>last boot: 2026-10-20 07:30 · 19.4 s</code>.", lead=True) + \
    ul(["A bar split into the phases of the start: " + ui("firmware") + ", " + ui("loader") + ", " + ui("kernel") + ", " + ui("initrd") + " and " +
        ui("services") + ", each with its seconds. Phases that do not exist on the machine are left out.",
        "Under the bar, " + ui("slowest (systemd-analyze blame)") + ": the units that took longest to start, with a bar and their time."]) + \
    p("This is the first place to look when a machine seems to start slowly. If the data are not available (in a container, or if the "
      "start has not finished), the panel says <code>boot times unavailable (container?)</code>.")

S_SESSIONS = p("The bottom right panel is <b>users &amp; sessions</b>: who is logged in and what they use.", lead=True) + \
    table(["Column", "Meaning"], [
        [ui("User"), "The account."],
        [ui("Session"), "Where the session is: the seat, the terminal and the kind (for example <code>seat0 tty2 wayland</code>)."],
        [ui("From"), "For a remote session, the address of the other machine. For a desktop session, the desktop."],
        [ui("Since"), "When the session began."],
        [ui("Procs") + ", " + ui("CPU") + ", " + ui("Mem"), "How many processes the user has and how much CPU and memory they use."],
    ], "«TAB» — Columns of the users and sessions panel") + \
    p("Users that have no session but run processes (the system accounts) follow, ordered by memory.")

S_NOSYSTEMD = p("The section needs systemd. On a machine that uses something else (OpenRC, runit, a minimal container), the section "
                "shows one line: " + ui("Services unavailable: systemd not found") + ". The other four sections are not affected.", lead=True)

S_KEYSSERV = p("The keys of the Services section, as the program reports them.", lead=True) + keys_table("Services")

CHAPTER = ("Services", [
    ("The Services screen", S_SCREEN),
    ("The list of units", S_LIST),
    ("Filters and search", S_FILTERS),
    ("System and user units", S_USER),
    ("The detail of a unit", S_DETAIL),
    ("The journal", S_JOURNAL),
    ("Starting and stopping", S_ACTIONS),
    ("Passwords, polkit and sudo", S_PASSWORD),
    ("Boot time", S_BOOT),
    ("Users and sessions", S_SESSIONS),
    ("When systemd is not there", S_NOSYSTEMD),
    ("Keys of the Services section", S_KEYSSERV),
])
