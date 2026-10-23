from build import arrow, box, c, fig, note, p, rif, table, term, text, zone

S1 = p("A monitor that uses a lot of CPU gives the wrong picture of the machine it watches. ngtop's goal is to be invisible: "
       "about <b>one percent of one core</b> at the default 1-second refresh on a small machine, a few megabytes of memory, and a start "
       "that feels instant.", lead=True) + \
    table(["Measure", "Value", "How it was measured"], [
        ["CPU at 1 s refresh", "about 1 %", "Intel N100, default interval. One cycle takes 8 to 11 ms (measured with " + c("ngtop --bench") + "), once a second"],
        ["Resident memory", "about 5.6 MB", "Same machine, as reported on the project page"],
        ["Binary", "about 3.5 MB", c("dist/ngtop") + ", static, stripped"],
        ["Start to first frame", "25 to 30 ms", c("ngtop --bench-start") + ": cold start and the mean of 10 starts, at 182 × 52"],
        ["One cycle", "8 to 11 ms", c("ngtop --bench") + " (below)"],
    ], "«TAB» — The figures") + \
    p("<b>What the benchmark does.</b> " + c("ngtop --bench") + " (hidden option, " + c("bench()") + " in " + c("src/main.rs") + ") sets a "
      "182 × 52 test terminal, shows each section in turn, runs three warm-up cycles and then thirty measured ones, each with its three "
      "parts timed separately: " + c("refresh") + " (all collectors), " + c("update") + " (histories, process list) and " + c("draw") + " (a whole frame into a buffer). "
      "Terminal output is not included: ratatui sends only changed cells, and that is cheap by comparison.") + \
    term("""$ ngtop --bench
1 Overview           refresh  7.42 + update 0.17 + draw 2.91 = 10.50 ms
2 GPU                refresh  6.93 + update 0.17 + draw 2.08 =  9.19 ms
3 Processes          refresh  6.66 + update 0.40 + draw 2.15 =  9.21 ms
4 Network & Disks    refresh  8.02 + update 0.28 + draw 2.26 = 10.56 ms
5 Services           refresh  6.67 + update 0.17 + draw 1.52 =  8.37 ms
(248 processes, 1 GPU(s))""", "One run on the Intel N100 (4 cores), 23 October 2026") + \
    note("The figures depend on the number of processes, because reading " + c("/proc") + " is most of the cost. On a quiet N100 a cycle takes 7 to 8 ms. "
         "Run " + c("ngtop --bench") + " before and after a change that could slow things down, on the same machine.", "Compare on the same machine.")

S2 = p("Two thirds or more of a cycle is " + c("refresh") + ", and most of that is reading one small file per process. Drawing a frame is "
       "2 to 3 ms. The design follows from this: <b>read less, not faster</b>.", lead=True) + \
    table(["Data", "Cache or schedule", "Where"], [
        ["Owner of a process", "Read from " + c("status") + " only for a new process, then every 15th cycle (staggered by pid, so a few per cycle); a daemon can change user without " + c("exec"), c("read_one()")],
        ["Command line of a process", "Read once; again only if the process was replaced (new start time or name). An empty one is retried", c("read_one()")],
        ["File reads", "One buffer and one path buffer are reused for every file of every process: no allocation per file", c("read_into()")],
        ["Open GPU descriptors of a process", "Scanned when a pid appears, then every 5th cycle (with descriptors) or every 20th (without), staggered by pid; never again if denied", c("GpuCollector::refresh()")],
        ["Static GPU data", "Name, ids, driver read once per card. The " + c("pci.ids") + " database (large) is read only when a new card appears and freed at once", c("read_static()") + ", " + c("discover()")],
        ["Socket to process", "Cache by inode, incremental; the walk of all fds stops when everything wanted is found; unreadable processes are remembered", c("scan_fds()")],
        ["Connections", "Collected only with the section visible, every second cycle", c("System::refresh_with_dt()")],
        ["Routes, addresses, DNS, link speed", "Every 5th cycle", c("net::refresh()")],
        ["Filesystem sizes", "Every 5th cycle, in threads, answers collected without waiting", c("FsMonitor")],
        ["Sensor files", "Found once (also for each disk); then only the value is read", c("find_cpu_temp_path()") + ", " + c("find_disk_temp()")],
        ["Disk model, type, size", "Once per disk", c("read_disk_static()")],
        ["Heavy process detail", "Only the open tab. " + c("smaps_rollup") + " (tens of ms on large processes) only on the General tab and every 5th cycle", c("DetailCollector::refresh()")],
        ["Services", "Unit file states cached 60 s; cgroup path kept while the unit lives; main pid asked again every 10 s or on a sub-state change; detail and journal asked 200 ms after the selection stops", c("dbus.rs") + ", " + c("Servizi::schedule()")],
        ["Cgroup memory and CPU", "Read from files, one per unit, rather than one D-Bus call per row", c("CgroupSampler")],
    ], "«TAB» — What is cached, and how often it is renewed") + \
    p("<b>Why staggered?</b> If every pid were rechecked in the same cycle (say every 15th), that cycle would be much slower than the others and the "
      "display would stutter. Adding the pid to the cycle counter spreads the work evenly. The same trick is used for the GPU descriptors.")

S3 = p("The interface side is kept cheap too.", lead=True) + \
    table(["Choice", "Effect"], [
        ["Redraw only when needed", "A frame is drawn after a key, a mouse event, a resize, a service answer or a tick. Between ticks an idle ngtop sleeps in " + c("event::poll") + ", waking at most four times a second to look for signals"],
        ["Diff on output", "ratatui compares the new buffer with the previous one and sends only the changed cells; a steady screen costs few bytes"],
        ["Fixed-size histories", "Each series is a ring of 600 samples (" + c("History") + "): memory does not grow with the run time. Histories of interfaces, disks and GPUs that disappear are dropped"],
        ["Short intervals are explicit", "250 ms is the fastest the user may choose (key " + c("-") + "); the cost is then about four times as high as at 1 s"],
        ["No work for hidden sections", "The Processes detail, the connection list and the Services thread are updated only when their section is visible"],
        ["Release build", c("opt-level = 3") + ", LTO, one codegen unit: the compiler sees the whole program and inlines across crates"],
    ], "«TAB» — Cheap interface") + \
    p("<b>Known costs, for whoever wants to go further.</b> " + c("App::update()") + " clones the process vector from " + c("System") + " on every cycle and sorts it; "
      "the tree and the filter are rebuilt each time. With thousands of processes this would be the first thing to optimise. And the whole refresh runs in the "
      "interface thread: a very slow " + c("/proc") + " (a machine under heavy load) delays the next frame by the same amount.")

CHAPTER = ("Performance", [
    ("Budget and measurements", S1),
    ("Caches and schedules", S2),
    ("The interface side", S3),
])
