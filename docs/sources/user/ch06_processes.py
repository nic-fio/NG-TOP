# Copyright (c) 2026 Nicola Fiorillo
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
"""User Manual, chapter 6 — the Processes section: tree and list, filter, sorting, details, signals, nice.
Sources: src/ui/sections/processi.rs (SIGNALS, SignalDialog, send_signal, protected, change_nice, DetailTab,
tab_lines, general_lines, on_key), src/ui/app.rs (filter_key, matches_filter, sorting), docs/decisions-and-history.md (D7)."""
from build import c, key, keys_table, note, ol, p, rif, snap, steps, table, term, tip, ui, ul, warn

S_SCREEN = p("The Processes section shows every process of the machine and, for the one you select, a panel of details. "
             "It is the place to find out what is slow, what is eating memory, and to stop what should not be running.", lead=True) + \
    snap("processes", "«FIG» — The Processes section: the tree on the left, the details of the selected process on the right", "150x42") + \
    p("On the left there is the table of the processes. On the right there are three panels about the selected process: the "
      "details (with tabs), its CPU history, and its memory and input/output. The selection stays on the same process when "
      "the order of the list changes, because ngtop follows the PID, not the row.")

S_TREE = p("The table has two views, which you switch with " + key("F5") + ".", lead=True) + \
    table(["View", "What it is"], [
        [ui("process tree"), "Each process is drawn under its parent, with lines (<code>├─</code> <code>└─</code>). It shows who started "
         "whom, so you can see that ten processes are all the children of one browser."],
        [ui("process list"), "A flat list, sorted by the column you choose. Best for finding the heaviest process."],
    ], "«TAB» — The two views") + \
    p("The columns are " + ui("PID") + ", " + ui("User") + ", " + ui("CPU%") + ", " + ui("Mem") + ", " + ui("GPU%") + " and " +
      ui("Name") + ". On a narrow panel the user and GPU columns are hidden first. " + ui("Mem") + " is the resident memory "
      "(the part of the process that is in RAM now). " + ui("CPU%") + " is a share of the whole machine (0–100); the " + ui("CPU") + " panel of the detail shows the same process as % of one core.") + \
    p("The lower border counts everything: <code>251 processes · 828 threads · sel. 1/251</code> means 251 processes, 828 "
      "threads in total, and the first of 251 rows is selected.")

S_FOLD = p("In the tree, a process that has children shows <code>▾</code> when its branch is open and <code>▸</code> when it is "
           "folded. Press " + key("Space") + " on a process to fold or open its branch. A folded branch hides all the "
           "descendants. The top border reminds you: <code>▸ closed ▾ open · Space folds</code>.", lead=True) + \
    tip("Fold the big branches you do not care about (the desktop session, a container manager) and the tree becomes short.")

S_FILTER = p("To look for a process, press " + key("/") + ". A text box opens in the title of the table: " + c("filter: _") +
             ". Type a few letters. The list shrinks at every letter.", lead=True) + \
    p("The filter looks, without regard to upper and lower case, in the <b>name</b>, the <b>whole command line</b>, the "
      "<b>user</b> and the <b>PID</b>. So <code>fire</code> finds Firefox, <code>alice</code> finds her processes and "
      "<code>4121</code> finds that PID.") + \
    table(["Key", "While you type"], [
        [key("Enter"), "Confirm. The box closes and the filter stays active."],
        [key("Esc"), "Cancel. The box closes and the filter is cleared."],
        [key("Backspace"), "Delete the last letter."],
        [key("↑") + " " + key("↓"), "Move the selection in the filtered list."],
    ], "«TAB» — Keys of the filter box") + \
    note("While the box is open every letter goes into it, even " + key("q") + ". Press " + key("Enter") + " or " + key("Esc") +
         " first. The keys " + key("F3") + " and " + key("F4") + " do the same as " + key("/") + ": the key bar shows two "
         "labels, " + ui("Search") + " and " + ui("Filter") + ", but there is only one box.", "Typing.") + \
    p("The filter works in the Overview and in the GPU section too, and it is the same filter: if you leave it on, it is still "
      "there when you change section.")

S_SORT = p("In the list view, and in the tree too, you can sort by a column.", lead=True) + \
    ul([key("F6") + " or " + key(">") + ": next column. " + key("<") + ": previous column. The cycle is PID, name, user, memory, CPU, GPU, VRAM.",
        key("i") + ": reverse the order of the current column.",
        "Click a column header: sort by it. Click again to reverse.",
        "The arrow in the header (<code>CPU%▼</code>) shows the column and the direction."]) + \
    p("When you pick memory, CPU, GPU or VRAM, the biggest value goes on top. The other columns start from the smallest.")

S_DETAIL = p("The right side shows the process that is selected. The title of the first panel is the name of the process and its "
             "PID, and on the right the owner. A row of <b>tabs</b> gives seven views. Change tab with " + key("←") + " and " + key("→") +
             ", or click it. If a tab has more lines than fit, scroll with " + key("[") + " and " + key("]") + " or with the "
             "mouse wheel over the panel.", lead=True) + \
    table(["Tab", "What it shows"], [
        [ui("General"), "The command line; the working directory; the executable; the parent (PID and name); when it started and how long "
         "ago; the state (running, sleeping, disk wait, stopped, zombie…); nice, priority and scheduling policy; the cores it may use and the "
         "one it ran on last; its cgroup; the user (and uid); the number of threads."],
        [ui("Threads"), "One line for each thread: TID, name, state and CPU%. The tab shows the count (<code>Threads 27</code>)."],
        [ui("Files"), "The number of open files and the limit; then the list of what they are (files, sockets, pipes…)."],
        [ui("Net"), "The TCP and UDP sockets of this process: protocol, local address, remote address, state."],
        [ui("Env"), "The environment variables of the process, with the names highlighted."],
        [ui("Limits"), "The resource limits (open files, memory, processes…), with soft limit, hard limit and units."],
        [ui("cgroup"), "The control group of the process: path, memory used and limit, CPU time, how often it was throttled, and the number "
         "of processes in it."],
    ], "«TAB» — The seven tabs of the detail panel") + \
    p("Only the tab you look at is read. When you change tab, the panel says <code>reading…</code> for an instant. This keeps ngtop "
      "light, even for a process with thousands of open files.") + \
    p("Under the tabs there are two more panels.") + \
    ul([ui("CPU") + ": the history of the CPU use of this process, and on the right its current value in <code>% of one core</code>.",
        ui("memory & I/O") + ": " + ui("RSS") + " (resident memory), " + ui("PSS") + " (the fair share of memory when pages are "
        "shared), " + ui("Shared") + ", " + ui("Swap") + ", " + ui("VRAM") + " (graphics memory), and the reading and writing totals "
        "(" + ui("Read") + ", " + ui("Write") + ") and speeds (" + ui("R/s") + ", " + ui("W/s") + ")."]) + \
    warn("For the processes of other users, some values show <b>—</b> or <code>— … (permission denied?)</code>: the working "
         "directory, the executable, the open files, the environment, the memory split and the input/output counters are "
         "protected by the system. Run ngtop as root to see them (" + rif("What you see without administrator rights") + ").",
         "Not all is visible.")

S_SIGNAL = p("A <b>signal</b> is a message that the system delivers to a process. The common use is to ask a program to quit, or, when it "
             "is stuck, to stop it. To send one:", lead=True) + \
    steps(["Select the process in the table (or in the Overview, or in the GPU table).",
           "Press " + key("F9") + " (in Processes also " + key("k") + "). The window " + ui("Send signal") + " opens.",
           "Choose the signal with " + key("↑") + " and " + key("↓") + ". The first one, " + ui("SIGTERM") + ", is the polite request and "
           "is already selected.",
           "Optionally press " + key("Space") + " to send the signal to the <b>whole subtree</b>: the process and all its children.",
           "Press " + key("Enter") + " to send, or " + key("Esc") + " to cancel."]) + \
    snap("processes", "«FIG» — The signal window", "120x36", overlay="signal") + \
    table(["Signal", "What it does"], [
        [ui("SIGTERM") + " (15)", "<b>Ask to quit</b> (recommended). The program can save its work and close cleanly."],
        [ui("SIGKILL") + " (9)", "<b>Stop now.</b> The program cannot refuse and cannot save: unsaved work is lost. Use it only when SIGTERM did not work."],
        [ui("SIGSTOP") + " (19)", "<b>Pause</b> the process, as if frozen."],
        [ui("SIGCONT") + " (18)", "<b>Resume</b> a paused process."],
        [ui("SIGHUP") + " (1)", "Many servers reload their configuration when they get it."],
        [ui("SIGINT") + " (2)", "The same as pressing " + key("Ctrl", "C") + " in the program."],
        [ui("SIGUSR1") + " (10)", "A signal whose meaning each program decides."],
        [ui("SIGUSR2") + " (12)", "A second one of the same kind."],
    ], "«TAB» — The signals ngtop can send") + \
    p("The result appears on the status line at the bottom: <code>SIGTERM sent to 4121 firefox</code> in green, or the reason of the "
      "failure in red.")

S_SAFETY = p("A signal acts on the real system, and a mistake can be costly. ngtop adds several protections, so that you cannot hit the "
             "wrong target by accident.", lead=True) + \
    table(["Protection", "How it works"], [
        ["The right process", "Between the moment you select a process and the moment you press " + key("Enter") + ", the process might "
         "have ended and the system might have given its number (the PID) to a new, different program. ngtop checks, <b>at the moment of "
         "sending</b>, that the process is still the same one (it compares the start time), and sends the signal in a way that stays bound to "
         "that very process. If the PID now belongs to another process, nothing is sent and the status line says "
         "<code>the pid now belongs to another process (reused)</code> or <code>process N has exited, no signal sent</code>."],
        ["PID 1 is never touched", "PID 1 is the first process, the one that starts the system. ngtop does not send it any signal. The "
         "window says <code>WARNING: PID 1 (init) will not be touched</code>."],
        ["No signal to the system as a whole", "With " + key("Space") + " you can choose the whole subtree, but not for PID 1 and PID 2 "
         "(the kernel's own threads): their subtrees are nearly the whole system, so that choice is refused with <code>the subtree of PID "
         "N is not allowed</code>."],
        ["ngtop itself", "If you select ngtop and send it a signal, the window warns you (<code>you are signalling ngtop itself</code>). In a "
         "subtree, ngtop skips itself: <code>ngtop itself (PID N) will be skipped</code>."],
        ["Big subtrees need a second Enter", "If the subtree has <b>more than 50 processes</b>, or contains processes of <b>other users</b>, "
         "the first " + key("Enter") + " only arms the window: it says <code>Warning: large subtree… press Enter twice</code>, and then "
         "<code>CONFIRM: press Enter again to send</code>. Changing the signal or the subtree choice disarms it."],
        ["The system has the last word", "A normal user can signal only their own processes. For the others the system answers " +
         "<code>permission denied (EPERM)</code>, which ngtop shows."],
    ], "«TAB» — What ngtop protects") + \
    note("If the PID has been reused, a plain <code>kill</code> would hit the wrong program. ngtop avoids it by asking the kernel for a "
         "stable handle to the process (a pidfd) before it checks, then it signals through the handle. On a very old kernel that does not "
         "have it (before 5.3) ngtop checks first and then sends the signal in the traditional way.",
         "Why it is safe.")

S_NICE = p("Every process has a <b>nice</b> value from −20 to 19. The higher the number, the more \"polite\" the process is, and the "
           "less CPU it gets when the machine is busy. The default is 0.", lead=True) + \
    ul([key("F8") + ": nice +1 on the selected process (less priority).",
        key("F7") + ": nice −1 (more priority)."]) + \
    p("Like <code>htop</code>, ngtop changes the <b>main thread</b> of the process. The status line says what happened: "
      "<code>nice of 4121 firefox (main thread): 0 → 1</code>.") + \
    p("Any user can raise the nice value of their own processes. To <b>lower</b> it (give more priority) you need root, and ngtop "
      "says <code>root is needed to lower the nice value</code>. At the limits the message is <code>already at the limit</code>.") + \
    p("The current nice and the priority are in the " + ui("General") + " tab, on the line " + ui("Priority") + ".")

S_KEYSPROC = p("The keys of the Processes section, as the program reports them.", lead=True) + keys_table("Processes")

CHAPTER = ("Processes", [
    ("The Processes screen", S_SCREEN),
    ("Tree and list", S_TREE),
    ("Folding branches", S_FOLD),
    ("Filtering", S_FILTER),
    ("Sorting", S_SORT),
    ("The detail panel", S_DETAIL),
    ("Sending a signal", S_SIGNAL),
    ("What ngtop protects you from", S_SAFETY),
    ("Changing the priority", S_NICE),
    ("Keys of the Processes section", S_KEYSPROC),
])
