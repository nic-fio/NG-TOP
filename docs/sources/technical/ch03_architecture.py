from build import arrow, box, c, fig, flow, note, p, path, rif, seq, table, text, zone

LAYERS = flow([
    ("Files and D-Bus", "/proc /sys /etc", "dark"),
    ("Collectors", "src/collect/", "navy"),
    ("System", "latest data", "blue"),
    ("App", "state, histories", "blue"),
    ("Views", "src/ui/sections/", "navy"),
    ("Terminal", "one buffer per frame", "grey"),
], "«FIG» — The data path, left to right: nothing on the right of a box reaches back into the box on its left")

CYCLE = seq([
    ("event_loop", "src/ui/mod.rs", "navy"),
    ("System", "src/collect/", "blue"),
    ("App", "src/ui/app.rs", "blue"),
    ("view::draw", "src/ui/view.rs", "navy"),
    ("Terminal", "crossterm", "grey"),
], [
    ("sep", "the tick is due"),
    (0, 1, "collect_conns = (section is Network & Disks)"),
    (0, 1, "refresh()"),
    (1, 0, "new data in place", True),
    (0, 2, "update(&sys)"),
    ("note", 2, "histories, process list, view"),
    ("sep", "the screen is dirty"),
    (0, 3, "draw(frame, app, sys)"),
    (3, 0, "buffer", True),
    (0, 0, "convert_buffer(colours)"),
    (0, 4, "flush the changed cells"),
], "«FIG» — One cycle of the event loop")

THREADS = fig(
    zone(20, 12, 420, 270, "Main thread (the interface)")
    + box(40, 44, 380, 50, "event_loop()", "keys, mouse, resize, tick, draw", "navy")
    + box(40, 112, 380, 50, "System::refresh()", "all collectors, in sequence, here", "blue")
    + box(40, 180, 380, 50, "App / sections", "state; poll() the answers", "blue")
    + text(230, 262, "never waits for D-Bus, subprocesses or statvfs", 11.5, "#334155")
    + zone(480, 12, 400, 130, "One thread per mounted filesystem")
    + box(500, 44, 360, 50, "ngtop-statvfs", "statvfs() on one mount point", "dark")
    + text(680, 126, "asked every 5th cycle; answers come back on a channel", 11, "#334155")
    + zone(480, 162, 400, 120, "Services thread (first visit to section 5)")
    + box(500, 192, 170, 50, "services thread", "D-Bus, cgroups", "dark")
    + box(690, 192, 170, 50, "short threads", "journal, blame, action, unit files", "grey")
    + arrow(422, 120, 478, 70, "#475569", True) + arrow(478, 80, 422, 130, "#475569", True)
    + arrow(422, 200, 478, 215, "#475569", True) + arrow(478, 225, 422, 210, "#475569", True)
    + arrow(672, 217, 688, 217, "#475569"),
    900, 296, "«FIG» — The threads of ngtop and how they talk (dashed arrows are mpsc channels)")

S1 = p("ngtop has two halves. The <b>collectors</b> (" + c("src/collect/") + ") read the kernel's files and turn "
       "counters into numbers: percentages, rates, latencies. The <b>interface</b> (" + c("src/ui/") + ") keeps the "
       "recent past of those numbers and draws them. The only thing that ties them is the " + c("System") + " "
       "structure, which the interface reads and never writes.", lead=True) + LAYERS + \
    p(c("System") + " (" + c("src/collect/mod.rs") + ") holds the latest value of everything: " + c("cpu") + ", "
      + c("mem") + ", " + c("load") + ", " + c("uptime") + ", " + c("processes") + ", " + c("net") + ", " + c("gpu")
      + ", " + c("disks") + ", " + c("conns") + ", " + c("cpu_temp") + " and " + c("zram") + ". It also holds the "
      "previous sample of the counters it needs for differences (for example " + c("prev_cpu") + ", "
      + c("prev_cores") + " and " + c("prev_procs") + "). A collector never stores history: it knows "
      "only “now” and “the sample before”.") + \
    p(c("App") + " (" + c("src/ui/app.rs") + ") keeps what the interface needs to remember: the histories "
      "(" + c("cpu_hist") + ", " + c("mem_hist") + ", and one per network interface, disk and GPU), the process list in "
      "the order shown, the selection, the filter, the open dialog and the state of each section. After every "
      "refresh, " + c("App::update()") + " pushes the new values into the histories (600 samples each, "
      + c("HISTORY_LEN") + ") and rebuilds the process list.") + \
    note("Because the collectors depend on nothing but " + c("Roots") + ", the same code runs on the real machine, on a "
         "fake tree in a test, behind " + c("ngtop --snapshot") + " when a screenshot is made for these manuals "
         "(" + rif("Testing") + "), and on another machine through ssh (" + rif("Sources and runners") + "). " + c("Roots") + " carries two handles besides the folders: "
         + c("src") + ", through which every file is read, and " + c("run") + ", through which every external command is run.", "Why the split.")

S2 = p(c("System::refresh()") + " measures the time since the previous call with the real clock and passes it to "
       + c("refresh_with_dt(dt)") + ". Tests call " + c("refresh_with_dt") + " directly with a chosen " + c("dt")
       + ", so rates do not depend on how fast the test runs. With " + c("dt <= 0") + " (the first sample) every "
       "rate is zero.", lead=True) + \
    table(["Order", "Step", "Function", "How often"], [
        ["0", "Tell the source a refresh begins", c("begin_refresh()"), "every cycle; local: nothing. Remote: all the files the collectors will read are fetched here in one round trip (" + rif("RemoteSource: manifest and bulk reads") + ")"],
        ["1", "CPU times and per-core frequency", c("refresh_cpu()"), "every cycle"],
        ["2", "Memory, load average, uptime", c("read_meminfo()") + ", " + c("read_loadavg()") + ", "
         + c("read_uptime()"), "every cycle"],
        ["3", "All processes", c("refresh_processes()") + " → " + c("read_all()"), "every cycle; owner, "
         "command line and ids are cached"],
        ["4", "GPUs and per-process GPU use", c("refresh_gpu()") + " → " + c("GpuCollector::refresh()"),
         "every cycle; open DRM files are rescanned every 5th or 20th cycle"],
        ["5", "Network interfaces", c("net::refresh()"), "counters every cycle; routes and addresses every 5th"],
        ["6", "Disk I/O", c("disk::refresh_io()"), "every cycle"],
        ["7", "Filesystems", c("FsMonitor") + " (" + c("request()") + ", " + c("poll()") + ", " + c("snapshot()") + ")",
         "mount list and a new request every 5th cycle; answers collected every cycle"],
        ["8", "Connections", c("conn::refresh_with()"), "only if the Network &amp; Disks section is visible, and "
         "then every 2nd cycle"],
        ["9", "CPU temperature, ZRAM", c("read_temp()") + ", " + c("read_zram()"), "every cycle (the sensor "
         "file is found once)"],
    ], "«TAB» — What one refresh does, in order") + \
    p("Step 4 also adds to each process the GPU share and GPU memory it found, so the process table "
      "can sort by GPU without knowing where the number came from. Details of each step are in "
      + rif("Data sources") + ".")

S3 = p("The loop is in " + c("event_loop()") + " (" + c("src/ui/mod.rs") + "). It is single-threaded and sleeps most of "
       "the time.", lead=True) + CYCLE + \
    table(["Rule", "Value", "Why"], [
        ["First tick", "250 ms after start", "Percentages appear almost at once instead of after a whole second"],
        ["Interval", c("INTERVALS_MS") + " = 250, 500, 1000, 2000, 5000 ms; default 1000", "Keys " + c("+") + " and "
         + c("-") + " step through the list; the configuration is rounded to the nearest step (" + c("round_interval()") + ")"],
        ["Longest sleep", "250 ms", "So that SIGTERM, SIGINT and SIGHUP are noticed quickly"],
        ["Sleep while waiting for services", "100 ms", c("App::background_active()") + " is true while the Services "
         "section has a request in flight"],
        ["Redraw", "only when something changed", "A flag, " + c("dirty") + ", is set by a key, mouse or resize event "
         "that changed the state, by a service answer, or by a tick. An idle ngtop does not redraw"],
        ["Interval change", "at once", "The next tick is brought forward if the new interval is shorter than the time left"],
        ["Queued events", "all read before the next draw", "Holding a key does not queue a hundred redraws"],
    ], "«TAB» — The rules of the event loop") + \
    p("At the start, " + c("run()") + " does everything that can fail or allocate a lot (time zone, the first "
      "sample) <i>before</i> it touches the terminal. Then it switches to raw mode and the alternate screen, "
      "optionally turns on the mouse, and enters the loop. How the terminal is given back, even after an error or a "
      "signal, is in " + rif("Terminal restore") + ".")

S4 = p("ngtop starts three kinds of thread, and none of them is a pool: each exists because one call can block.",
       lead=True) + THREADS + \
    table(["Thread name", "Started by", "Lives", "Does", "Talks to the interface through"], [
        ["(main)", "the process", "whole run", "event loop, all collectors except the ones below, drawing", "—"],
        [c("ngtop-statvfs"), c("FsMonitor::set_mounts()"), "while its mount exists",
         c("statvfs") + " on one mount point. A hung network mount blocks only its own thread",
         "a request channel in, one shared answer channel out"],
        [c("ngtop-nvidia"), c("NvClient::spawn()"), "from the first awake NVIDIA card to exit",
         "runs " + c("nvidia-smi") + " every 2 s (through the " + c("sh") + " guard, with a 3 s timeout) for the awake NVIDIA cards",
         "a shared " + c("NvSnap") + " under a mutex"],
        [c("ngtop-services"), c("Client::spawn()"), "from the first visit to the Services section to exit",
         "executes " + c("Request") + " values: unit lists, details, sessions, boot times", "two " + c("mpsc") + " channels"],
        [c("ngtop-journal") + ", " + c("ngtop-blame"), c("run()") + " in " + c("worker.rs"), "one request",
         "run " + c("journalctl") + " or " + c("systemd-analyze blame") + " with a time limit", "the services answer channel"],
        [c("ngtop-action"), c("run()") + " in " + c("worker.rs"), "one action",
         "start, stop, restart, enable or disable a unit. At most one at a time; a second one is refused at once",
         "the services answer channel"],
        ["(ssh helpers)", c("Session::start()"), "remote mode only, for the whole session", "read the ssh process's stdout and stderr, write its stdin", "channels to the session"],
        ["(reconnection)", c("lost()"), "remote mode only, until the connection is back", "tries to start a new session with a growing wait (" + rif("Reconnection and the interval") + ")", "a shared state"],
        [c("ngtop-unitfiles"), c("unit_file_states()"), "one refresh",
         "reads the list of unit files (a slow D-Bus call) and stores it in a cache", "a shared cache"],
    ], "«TAB» — Threads") + \
    p("The pattern is always the same: the interface sends a request and goes on; later " + c("poll()") + " takes "
      "whatever answers have arrived, without waiting (" + c("try_recv") + "). The interface can therefore draw, "
      "scroll and take keys while systemd is slow or absent. The tests " + c("l_interfaccia_non_attende_il_backend") + " and "
      + c("un_azione_lenta_non_blocca_la_coda_e_ce_ne_e_una_sola") + " check it with a deliberately slow backend.") + \
    note("Signals are not handled by a thread. " + c("signal_hook") + " installs handlers that set an atomic flag; the "
         "event loop reads the flag. See " + rif("Terminal restore") + ".", "No signal thread.")

S5 = p("The interface state is one " + c("App") + " value passed by reference to the drawing code. The pieces "
       "that are specific to a section live in their own structure, so a section can be read on its own.",
       lead=True) + \
    table(["State", "Type", "Holds"], [
        [c("app.proc"), c("ProcState"), "detail tab, folded branches, scroll, the status line, the signal and start-time functions "
         "(replaceable in tests)"],
        [c("app.rd"), c("RdState"), "connection list, filter, sort, selected row, disk scroll"],
        [c("app.servizi"), c("Servizi"), "unit list, filter chip, search, selection, pending requests, the confirmation, "
         "the services client"],
        [c("app.rows") + " / " + c("app.view"), c("Vec<Process>"), "all processes, sorted; and the indices actually shown "
         "(after tree order, folding and filter) with their tree prefix"],
        [c("app.selected_pid"), c("Option<u32>"), "the selection is kept as a pid, so it stays on the same process "
         "when the list is reordered"],
        [c("app.hits"), c("Hits"), "click targets computed by the last draw: tab ranges, the table area, the columns. The "
         "mouse handler uses them; there is no separate hit-testing pass"],
    ], "«TAB» — The parts of App") + \
    p("<b>Layout mode.</b> " + c("LayoutMode::for_size()") + " chooses between large and compact: compact when the "
      "terminal is narrower than 100 columns <i>or</i> lower than 30 rows. The mode is recomputed on every "
      "resize event; only the Overview has a different compact layout (" + rif("The Overview section") + ").") + \
    p("<b>Drawing.</b> " + c("view::draw()") + " splits the screen into the top line, the body and the bottom line, "
      "asks the active section to draw the body, then draws the key bar for that section, the status line of signals "
      "and nice (which replaces the key bar for five seconds), and last the help or the dialog on top. Every widget "
      "writes into the same ratatui " + c("Buffer") + " (" + rif("The UI framework") + ").")

CHAPTER = ("Architecture", [
    ("Collect and draw", S1),
    ("One refresh, step by step", S2),
    ("The event loop", S3),
    ("Threads", S4),
    ("State and drawing", S5),
])
