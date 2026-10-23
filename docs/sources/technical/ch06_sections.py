from build import c, key, note, p, rif, snap, table, ui

S0 = p("The five sections are the five files in " + c("src/ui/sections/") + ". Each has the same shape: a " + c("draw()") + " "
       "function that fills the body rectangle, and, when it has keys of its own, an " + c("on_key()") + " that "
       "gets the key <i>before</i> the general handler of " + c("App::on_key()") + ". The order matters: when the dialog is open it "
       "takes every key; then the Services section; then the filter being typed; then the Network &amp; Disks and Processes "
       "sections; then the keys common to all.", lead=True) + \
    table(["Key", "Everywhere"], [
        [key("1") + " … " + key("5") + ", " + key("Tab") + ", " + key("Shift-Tab"), "Switch section"],
        [key("F1"), "Help window (" + c("help_lines()") + " in " + c("keys.rs") + "); " + key("Esc") + ", " + key("Enter") + ", " + key("F1") + " or " + key("q") + " closes it"],
        [key("+") + " / " + key("-"), "Refresh interval: the next or the previous of 250, 500, 1000, 2000, 5000 ms"],
        [key("W"), "Save the interval and the section to the configuration file (other lines and comments are kept)"],
        [key("q") + ", " + key("F10") + ", " + key("Ctrl-C"), "Quit"],
        ["Mouse", "Click a tab, a row or a column header; the wheel scrolls"],
    ], "«TAB» — Keys valid in every section")

S1 = p("The first section shows the whole machine at a glance, and is the one the program starts in.", lead=True) + \
    snap("overview", "«FIG» — The Overview section in the large layout (120 × 36)") + \
    table(["Panel", "Shows", "From"], [
        [ui("cpu"), "A braille graph of total CPU use on the left. On the right: a meter with total percentage, temperature and average clock, and one "
         "meter per core (C0, C1 …). The model and the thread count are on the top border. When there are too many cores to fit, the bottom border says “+N cores not shown”",
         c("cpu.rs") + ", " + c("sensors.rs")],
        [ui("mem"), "Meters Used, Available, Cache, Free and Swap, with a memory graph beside them when there is room. A last line adds ZRAM (original → compressed and the ratio), "
         "zswap and Dirty if present", c("mem.rs")],
        [ui("gpu"), "The selected GPU in brief, only if there is one and there is room (key " + key("d") + " switches card)", c("gpu.rs")],
        [ui("net"), "Rates of the chosen interface with a graph; its name, address and “(down)” on the border; " + key("n") + " / " + key("N") + " switch interface", c("net.rs")],
        [ui("disks"), "One line per filesystem: usage bar and activity", c("disk.rs")],
        [ui("processes"), "The process table: PID, Name, Command, User, Thr, Mem, GPU% and VRAM (with a GPU), CPU% — columns appear as the width allows", c("process.rs")],
    ], "«TAB» — The panels of the Overview") + \
    table(["Key", "Action"], [
        [key("Up") + " " + key("Down") + " " + key("PgUp") + " " + key("PgDn") + " " + key("Home") + " " + key("End"), "Move the selection. It is kept on the same pid when the list is reordered"],
        [key("F6") + ", " + key(">") + ", " + key("<"), "Next / previous sort column (PID, Name, User, Mem, CPU, GPU, VRAM). Clicking a header does the same"],
        [key("i"), "Invert the sort direction"],
        [key("F5"), "List or tree. The tree setting of the Overview is separate from that of Processes (default: list)"],
        [key("/") + ", " + key("F3") + ", " + key("F4"), "Filter: type text; it matches the name, the command line, the user or the pid, ignoring case. " + key("Enter") + " keeps the filter, " + key("Esc") + " removes it"],
        [key("F9"), "Send a signal to the selected process (" + rif("Signals: pidfd and start time") + ")"],
        [key("n") + " / " + key("N"), "Next / previous network interface"],
        [key("d"), "Next GPU"],
    ], "«TAB» — Keys of the Overview") + \
    p("<b>Compact layout.</b> Under 100 columns or 30 rows, the Overview becomes a header of one-line meters (CPU with a "
      "sparkline of the cores, MEM, GPU, NET, DSK) above a table of PID, User, CPU%, Mem and Command. The bottom bar shortens to "
      "Help, Filter, Tree, Sort, Sig, Quit.") + \
    snap("compact", "«FIG» — The compact layout (80 × 24)", "80x24")

S2 = p("The GPU section is nvtop inside ngtop: one panel per device, a history of the selected one, and the processes that use any GPU.",
       lead=True) + \
    snap("gpu", "«FIG» — The GPU section (120 × 36)") + \
    table(["Part", "Shows"], [
        ["Device panels", "One per card, side by side if there is room (70 cells each). A cell for each available quantity: usage meter, VRAM (and GTT / system memory), "
         "ENC / DEC / VID engines, temperature, power and its cap, clocks, fan, PCIe link (current / maximum). The title has the name. A sleeping card shows its state instead (" + rif("GPU: devices") + ")"],
        [ui("GPU N history"), "Braille graphs of usage and VRAM, temperature and power of the selected card, 600 samples"],
        [ui("engines · GPU N"), "Busy percentage of each engine (graphics, compute, copy, video decode and encode) as read from the clients' fdinfo"],
        [ui("power"), "Present with room: power draw against the limit"],
        [ui("GPU processes"), "Only the processes using a GPU: PID, User, DEV (which card), Type of work (Graphics, Compute, Video, Copy), GPU%, ENC%, DEC%, VRAM, CPU%, Mem, Name and Command, as the width allows. When the selected card is an NVIDIA GPU whose processes come from " + c("--query-compute-apps") + ", the top border adds “only CUDA processes are listed on this GPU”"],
    ], "«TAB» — The GPU section") + \
    table(["Key", "Action"], [
        [key("d"), "Select the next device; the history and engines follow"],
        [key("F6") + ", " + key("/") + ", " + key("F9"), "Sort, filter, signal: as in the Overview"],
    ], "«TAB» — Keys of the GPU section") + \
    p("With no GPU the section shows a panel saying “No GPU detected”. With an NVIDIA card without nvidia-smi, one that does not answer, or a card asleep, the device panel says what it "
      "can: ngtop never invents a figure for what it did not read.")

S3 = p("Processes is the process manager: a large table, and for the selected process a detail area with seven tabs.", lead=True) + \
    snap("processes", "«FIG» — The Processes section (120 × 36)") + \
    table(["Part", "Shows"], [
        ["Table", "The panel “process tree” (or “process list”): PID, User, CPU%, Mem, GPU% (with a GPU), Name. The default is the <b>tree</b> (the other sections default to a list). The bottom border shows the number of processes and threads and the selected row; the top right shows the filter"],
        ["Detail", "A panel titled “NAME · PID N”, with the owner on the border and the tabs inside. The General tab shows:"],
        ["Detail, General", "Command, directory, executable, parent, start time, state, nice / priority / scheduling policy, CPU affinity and last core, cgroup, user, threads"],
        ["Detail tabs", ui("General") + ", " + ui("Threads") + ", " + ui("Files") + ", " + ui("Net") + ", " + ui("Env") + ", " + ui("Limits") + ", " + ui("cgroup")
         + ". Only the open tab's data is read (" + rif("Processes") + ")"],
        [ui("CPU") + ", " + ui("memory &amp; I/O"), "A graph of the selected process's CPU (as a share of one core, with its scale on the border); and RSS, PSS, shared, swap, VRAM, bytes read and written and their rates"],
    ], "«TAB» — The Processes section") + \
    table(["Key", "Action"], [
        [key("Space"), "Fold or unfold the branch under the selected process. Folded branches are forgotten when their process disappears"],
        [key("F5"), "Tree or list"],
        [key("Left") + " " + key("Right"), "Previous / next detail tab"],
        [key("[") + " " + key("]"), "Scroll the detail up / down by 3 lines (the wheel too, over the detail)"],
        [key("F7") + " / " + key("F8"), "Nice −1 / +1 for the main thread of the selected process (" + rif("Nice") + ")"],
        [key("F9") + " or " + key("k"), "Signal dialog"],
        [key("F3") + " / " + key("F4") + " / " + key("/"), "Search / filter"],
    ], "«TAB» — Keys of Processes") + \
    note("Where another section shows a process (a connection in Network &amp; Disks), " + key("Enter") + " there jumps here: the folded branches above the "
         "process are opened and a filter that would hide it is removed (" + c("App::jump_to_pid()") + ").", "Jump to process.")

S4 = p("This section joins what a network monitor and a disk monitor would show on two screens.", lead=True) + \
    snap("network", "«FIG» — The Network &amp; Disks section (120 × 36)") + \
    table(["Panel", "Shows"], [
        ["Interface", "Received and sent rates of the chosen interface as two mirrored braille graphs, the peak of the session, totals since start, IPv4 / IPv6, link speed and duplex, gateway and DNS"],
        [ui("interfaces"), "Every interface with state, address, rates; click one to select it"],
        [ui("connections"), "The TCP and UDP sockets, with local and remote address, state, queues, pid and process name. The border shows the filter, LISTEN and the sort"],
        ["One panel per disk", "Name on the border; model, type (HDD, SSD, NVMe), size, temperature; read and write rates; IOPS; busy %; latency; queue"],
        [ui("filesystems"), "Mount point, device, type, usage bar, available space, inode use; “not responding” for a stuck mount"],
    ], "«TAB» — The panels of Network &amp; Disks") + \
    table(["Key", "Action"], [
        [key("i") + " / " + key("I"), "Next / previous interface"],
        [key("c"), "Protocols: TCP+UDP, then TCP, then UDP"],
        [key("l"), "Show or hide LISTEN sockets (shown by default)"],
        [key("F6"), "Sort the connections by state, proto, local, remote, pid, process, queue, in turn"],
        [key("z"), "Reset the session peaks and totals"],
        [key("Enter"), "Go to the process of the selected connection (the Processes section)"],
        [key("F9"), "Signal dialog for that process"],
        [key("Up") + " " + key("Down") + " " + key("Home") + " " + key("End"), "Move in the connections"],
        [key("PgUp") + " " + key("PgDn"), "Scroll the disk panels"],
    ], "«TAB» — Keys of Network &amp; Disks") + \
    p("The connection list is collected only while this section is visible (" + rif("Connections: inode to pid") + "). The height of the blocks is shared out "
      "by " + c("allocate()") + " (" + rif("Layout") + ").")

S5 = p("Services is a systemd manager: it lists the units, shows the detail and the journal of the selected one, and "
       "starts, stops, restarts, enables or disables it after asking.", lead=True) + \
    snap("services", "«FIG» — The Services section (120 × 36)") + \
    table(["Part", "Shows"], [
        ["List", "Filter chips (" + ui("All") + ", " + ui("Active") + ", " + ui("Failed") + ", " + ui("Enabled") + ", " + ui("Timers") + ", " + ui("Sockets") + ", each with its count), then every unit: "
         "in the panel “systemd units”: a state symbol (● active, ◷ timer waiting, ◔ starting or stopping, ✖ failed, ○ inactive), name, description, state, "
         "whether it starts at boot, PID, memory and CPU%. Narrow terminals drop the description and the boot column first"],
        ["Detail", "Of the selected unit: description, states and since when, main pid and its command, memory with its peak, tasks, CPU time, restarts and policy, "
         "dependencies, the unit file; and below it the last journal lines"],
        [ui("boot"), "Where boot time went: firmware, loader, kernel, initrd, services; and the slowest units"],
        [ui("users &amp; sessions"), "The login sessions, with the processes and memory of each user"],
        [ui("journal · NAME"), "The journal of the selected unit in full screen"],
    ], "«TAB» — The parts of the Services section") + \
    table(["Key", "Action"], [
        [key("/"), "Search by name or description (" + key("Enter") + " keeps it, " + key("Esc") + " clears it)"],
        [key("f"), "Next filter chip"],
        [key("u"), "Switch between system units and the user's own units (session bus)"],
        [key("s") + ", " + key("t") + ", " + key("r"), "Start, stop, restart: each opens a confirmation (" + ui("Cancel") + " / " + ui("Confirm") + ")"],
        [key("e"), "Enable, or disable if it is already enabled (the same confirmation)"],
        [key("j"), "Journal full screen; " + key("Esc") + " goes back"],
        [key("Up") + " " + key("Down") + " " + key("PgUp") + " " + key("PgDn") + " " + key("Home") + " " + key("End"), "Move"],
    ], "«TAB» — Keys of Services") + \
    p("<b>Layout.</b> With 120 columns × 38 rows or more (a body of 120 × 36): list (58 %) and detail (42 %) on top, boot and sessions below. From 100 columns: list and detail side "
      "by side. Narrower: the list above and the detail below. <b>Waiting.</b> While a request is outstanding, the section polls the thread every 100 ms. "
      "If a request has waited 10 seconds, the status line says “systemd is not responding”. <b>No systemd.</b> If " + c("/run/systemd/system") + " does "
      "not exist, the section shows a single panel: “Services unavailable: systemd not found”.") + \
    p("<b>Rhythm of requests</b> (" + c("Servizi::schedule()") + "): the unit list every 2 s (every 0.6 s until it has been received twice); the detail "
      "and the journal only 200 ms after the selection has stopped moving (so scrolling quickly asks for nothing), the detail every 2 s and the journal "
      "every 5 s; the sessions every 10 s; boot times and the slowest units once. All requests are in " + rif("Services, sessions, boot") + ".")

CHAPTER = ("The sections", [
    ("Switching sections", S0),
    ("The Overview section", S1),
    ("The GPU section", S2),
    ("The Processes section", S3),
    ("The Network & Disks section", S4),
    ("The Services section", S5),
])
