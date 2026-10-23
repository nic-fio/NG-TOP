from build import arrow, box, c, diamond, fig, flow, note, p, path, rif, sysf, table, text, zone, warn

S1 = p("A collector is a function that reads a few small text files and returns a plain value. All of them follow "
       "the same four rules, which is why a missing file never becomes an error on screen.", lead=True) + \
    table(["Rule", "In the code"], [
        ["The root is a parameter", "Reads go through " + c("roots.src") + ", never straight to the disk. Paths are built from " + c("roots.proc") + ", " + c("roots.sys") + " and "
         + c("roots.etc") + ". On a real machine they are " + c("/proc") + ", " + c("/sys") + ", " + c("/etc")
         + "; in a test they point into " + c("tests/fixtures/")],
        ["Reading cannot fail", c("read_text()") + " of " + c("Source") + " (" + c("src/source/mod.rs") + ") returns " + c("Option<String>")
         + ": " + c("None") + " if the file is missing or unreadable, and invalid UTF-8 is replaced, not rejected. Collectors call it on " + c("roots.src") + " (" + rif("Sources and runners") + ")"],
        ["Parsing is separate from reading", "Each file has a " + c("parse_*()") + " function that takes text. The tests "
         "feed it strings, so no file is needed"],
        ["Rates need two samples", "A rate is " + c("(now − before) / dt") + ". The first sample has no “before”, so "
         "its rates are zero. Counters that go backwards (a reset) are clamped with " + c("saturating_sub") + ", never negative"],
    ], "«TAB» — The rules every collector follows") + \
    table(["Data", "Read from", "Collector", "Needs privileges?"], [
        ["CPU", c("/proc/stat") + ", " + c("/proc/cpuinfo") + ", " + c("/sys/devices/system/cpu/cpuN/cpufreq/scaling_cur_freq"),
         c("cpu.rs"), "no"],
        ["CPU temperature", c("/sys/class/hwmon/*/") + ", " + c("/sys/class/thermal/"), c("sensors.rs"), "no"],
        ["Memory, swap", c("/proc/meminfo") + ", " + c("/sys/block/zram*/mm_stat"), c("mem.rs") + ", " + c("mod.rs"), "no"],
        ["Load, uptime", c("/proc/loadavg") + ", " + c("/proc/uptime"), c("load.rs"), "no"],
        ["Processes", c("/proc/[pid]/stat") + ", " + c("status") + ", " + c("cmdline"), c("process.rs"), "no"],
        ["Selected process", c("/proc/[pid]/") + " many files", c("procdetail.rs"), "partly (other users' "
         + c("io") + ", " + c("fd") + ", " + c("environ") + ")"],
        ["Network", c("/proc/net/dev") + ", " + c("route") + ", " + c("fib_trie") + ", " + c("if_inet6") + ", "
         + c("/sys/class/net/*/") + ", " + sysf("/etc/resolv.conf"), c("net.rs"), "no"],
        ["Connections", c("/proc/net/{tcp,tcp6,udp,udp6}") + ", " + c("/proc/[pid]/fd"), c("conn.rs"),
         "owners of other users' sockets need root"],
        ["Disks", c("/proc/diskstats") + ", " + c("/sys/block/*/"), c("disk.rs"), "no"],
        ["Filesystems", c("/proc/self/mountinfo") + ", " + c("statvfs"), c("disk.rs"), "no"],
        ["GPU", c("/sys/class/drm/card*/device/") + ", " + c("/proc/[pid]/fdinfo/*"), c("gpu.rs"),
         "other users' clients need root"],
        ["Services", "systemd and login1 on D-Bus, " + c("/sys/fs/cgroup/") + ", " + c("journalctl") + ", "
         + c("systemd-analyze"), c("services/"), "starting and stopping asks polkit"],
        ["Time zone", c("TZ") + ", " + sysf("/etc/localtime"), c("src/ui/tz.rs"), "no"],
    ], "«TAB» — What is read, from where")

S2 = p("The processor view is built from two files and one directory.", lead=True) + \
    table(["Quantity", "Formula", "Where"], [
        ["Fields of a " + c("cpu") + " line", c("user nice system idle iowait irq softirq steal") + ". The sum of the first "
         "eight is the <i>total</i>; <i>idle</i> is " + c("idle + iowait") + ". (" + c("guest") + " is already counted inside "
         + c("user") + ", so it is not added.)", c("parse_times()")],
        ["Usage between two samples", "Δtotal = total<sub>now</sub> − total<sub>before</sub>; Δidle likewise (capped at "
         "Δtotal). <b>usage % = (Δtotal − Δidle) × 100 / Δtotal</b>; zero if Δtotal is zero", c("usage_between()")],
        ["Total and per core", "The " + c("cpu") + " line gives the total, each " + c("cpuN") + " line one core. A core "
         "that goes offline simply stops appearing", c("parse_stat()")],
        ["Frequency", c("scaling_cur_freq") + " is in kHz; the value shown is kHz / 1000 MHz. Missing on some "
         "machines: “—”", c("read_freq_mhz()")],
        ["Model", "The first " + c("model name") + " line of " + c("/proc/cpuinfo"), c("parse_model()")],
        ["Logical and physical cores", "Logical = number of " + c("processor") + " blocks. Physical = number of "
         "distinct pairs (" + c("physical id") + ", " + c("core id") + "); unknown if the file has no such lines "
         "(virtual machines, some ARM boards)", c("parse_topology()")],
    ], "«TAB» — CPU formulas") + \
    p("<b>Temperature.</b> " + c("find_cpu_temp_path()") + " looks once, at the first refresh, for the file to read, "
      "and the answer (even “none”) is remembered. It scans " + c("/sys/class/hwmon/hwmon*") + " in name order and "
      "accepts the first sensor whose " + c("name") + " is known: " + c("coretemp") + " (label “Package id 0”), "
      + c("k10temp") + " (labels “Tctl” then “Tdie”), " + c("zenpower") + " (“Tdie” then “Tctl”) and "
      + c("cpu_thermal") + " / " + c("cpu-thermal") + ". If the wanted label is not found it falls back to "
      + c("temp1_input") + ". If no hwmon matches, it looks for a thermal zone of type " + c("x86_pkg_temp") + ". The "
      "value is in thousandths of a degree and is divided by 1000. The same " + c("read_temp()") + " reads disk and GPU "
      "temperatures.")

S3 = p("Memory comes from one file, " + c("/proc/meminfo") + ", whose values are in kB; the parser multiplies by 1024 "
       "(a value with no unit is taken as bytes). Missing keys count as zero.", lead=True) + \
    table(["Shown as", "Formula", "Note"], [
        ["Used", c("MemTotal − MemAvailable"), "Not “total − free”: cache the kernel can give back is not counted as used"],
        ["Available", c("MemAvailable"), "If the kernel is too old to have it: " + c("MemFree + Buffers + Cached") + ", at most the total"],
        ["Cache", c("Cached + SReclaimable"), "Page cache plus reclaimable kernel memory"],
        ["Buffers, Free", c("Buffers") + ", " + c("MemFree"), ""],
        ["Swap used", c("SwapTotal − SwapFree"), ""],
        ["zswap", c("Zswap") + " (compressed size) and " + c("Zswapped") + " (original size)", "Only if the kernel exposes them"],
        ["Dirty", c("Dirty"), "Pages waiting to be written"],
        ["ZRAM", "Sum, over " + c("/sys/block/zram*/mm_stat") + ", of the first two numbers: original data size and "
         "compressed size", "The ratio original / compressed is shown as “2.5×”. Not shown if there is no ZRAM or it is empty"],
    ], "«TAB» — Memory formulas") + \
    p("<b>Load and uptime.</b> " + c("/proc/loadavg") + ": the first three numbers are the 1, 5 and 15 minute averages; the "
      "fourth, “running/total”, gives the number of runnable and existing scheduling entities. " + c("/proc/uptime")
      + ": seconds since boot, and idle seconds summed over all cores. The tab bar shows uptime as " + c("3d 04:12") + " or "
      + c("04:12:05") + ".")

PROC_TABLE = table(["Field of " + c("stat"), "Index after the last “)”", "Used for"], [
    ["state", "0", "R, S, D, Z, T… shown in the detail"],
    ["ppid", "1", "the tree"],
    ["utime, stime", "11, 12", "CPU time (clock ticks)"],
    ["priority, nice", "15, 16", "priority and nice in the General tab; nice is what F7 / F8 change"],
    ["num_threads", "17", "thread count"],
    ["starttime", "19", "start (ticks since boot): the identity of a process, with its pid"],
    ["rss", "21", "resident pages; × the page size = bytes"],
], "«TAB» — The fields of /proc/[pid]/stat that ngtop uses")

S4 = p("Reading every process is the most expensive part of a refresh, so " + c("process::read_all()") + " is written "
       "to do as little as possible per process. For each directory in " + c("/proc") + " whose name is a number it "
       "reads " + c("stat") + " every time and the other files only when needed.", lead=True) + PROC_TABLE + \
    p("The name (" + c("comm") + ") is in parentheses and may contain spaces and even a “)”, so " + c("parse_stat()")
      + " takes the text between the <i>first</i> “(” and the <i>last</i> “)”, and counts fields after it.") + \
    table(["File", "When it is read", "What for"], [
        [c("stat"), "every cycle, every process", "everything in the table above"],
        [c("status"), "for a new process; again every 15th cycle, staggered by pid", "the owner (first number of the " + c("Uid:") + " line). "
         "If " + c("status") + " cannot be read, the owner of the " + c("/proc/[pid]") + " directory is used"],
        [c("cmdline"), "for a new process, or after the name or start time changed (an " + c("exec") + ")", "the full command; NUL "
         "bytes become spaces; an empty one (kernel threads) is shown as " + c("[name]")],
        [c("io"), "not in normal runs", "The code path exists behind the flag " + c("want_io") + ", which nothing turns on, so the process list has no I/O columns. The detail of the selected process reads its own " + c("io")],
        [sysf("/etc/passwd"), "once, at start", "user ids to names; first name wins for a duplicated id"],
    ], "«TAB» — What is read for each process, and when") + \
    p("<b>CPU % of a process</b> is a share of the <i>whole machine</i>, from 0 to 100:") + \
    p("<b>cpu% = Δ(utime + stime) ÷ hz ÷ dt ÷ ncores × 100</b>, clamped to 0–100", lead=False) + \
    p("where " + c("hz") + " is the number of clock ticks per second (" + c("clock_ticks_per_second()") + "), " + c("dt")
      + " is the real time between the two refreshes, and " + c("ncores") + " is the number of cores listed in "
      + c("/proc/stat") + ". A process that fully uses one core of eight shows 12.5. The previous sample counts only if "
      "it has the <i>same start time</i>: if a pid was reused by a new process, its first sample shows zero instead of "
      "a wrong number.") + \
    p("<b>Memory</b> is the resident set: " + c("rss pages × page size") + ", with the page size read once from the system.") + \
    p("<b>Tree order.</b> " + c("tree_order()") + " puts each process under its parent, children sorted by pid, with the "
      "prefixes " + c("├─ ") + ", " + c("└─ ") + " and " + c("│  ") + ". A process whose parent does not exist is a root; "
      "if a loop in the parent ids ever existed, the leftover processes are made roots instead of being lost or "
      "looping forever. <b>Sorting</b> (" + c("sort_processes()") + ") is always stable: ties are broken by pid.") + \
    p("<b>The selected process.</b> " + c("DetailCollector::refresh()") + " reads, only for the pid selected in the "
      "Processes section, " + c("exe") + " and " + c("cwd") + " (links), " + c("stat") + " (last core, scheduling policy), "
      + c("status") + " (CPU affinity, swap), " + c("cgroup") + ", " + c("io") + " and the number of open files. Heavy "
      "files are read only when their tab is shown: " + c("task/*/stat") + " (Threads, with per-thread CPU %), "
      + c("fd") + " and " + c("limits") + " (Files), the socket tables (Net), " + c("environ") + " (Env), "
      + c("limits") + " (Limits) and the cgroup files (cgroup). " + c("smaps_rollup") + " (PSS, shared, swap) can take "
      "tens of milliseconds on a large process, so it is read only on the General tab, when the process changes and "
      "then every 5th cycle. Lists are capped: 200 files (" + c("MAX_FILES") + ") and 500 environment variables ("
      + c("MAX_ENV") + ").")

S5 = p("Network data is split into what changes every cycle (the byte counters) and what hardly ever does (routes, "
       "addresses, link speed). The second group is reread every 5th cycle.", lead=True) + \
    table(["Quantity", "Source and formula"], [
        ["Bytes in / out", c("/proc/net/dev") + ": field 0 after the colon is received bytes, field 8 is sent bytes"],
        ["Rate", "<b>(bytes<sub>now</sub> − bytes<sub>before</sub>) ÷ dt</b>, in bytes per second. Zero for a new interface"],
        ["Session peak", "The highest rate seen since start (key " + c("z") + " in Network &amp; Disks resets it)"],
        ["Link up", c("/sys/class/net/X/operstate") + " is “up” or “unknown” (tunnels report unknown, and are active)"],
        ["Speed, duplex", c("/sys/class/net/X/speed") + " (Mbit/s, ignored if not positive) and " + c("duplex") + " (ignored if “unknown”)"],
        ["Default route and gateway", c("/proc/net/route") + ": the route with destination 0 and mask 0 and the lowest metric. "
         "Values are hexadecimal, in the byte order of the machine, and converted back to dotted form"],
        ["IPv4 address", c("/proc/net/fib_trie") + ": the “/32 host LOCAL” entries, not 127.x; an address belongs to the "
         "interface whose route has a network that contains it: <b>(address &amp; mask) = destination</b>"],
        ["IPv6 address", c("/proc/net/if_inet6") + ": the global one if there is one, otherwise the link-local"],
        ["DNS servers", "the " + c("nameserver") + " lines of " + sysf("/etc/resolv.conf")],
    ], "«TAB» — Network quantities") + \
    p("<b>The interface shown.</b> " + c("NetInfo::pick()") + " chooses the one the user selected, else the interface of "
      "the default route, else the first one that is not " + c("lo") + ". " + c("NetInfo::cycle()") + " moves to the next "
      "or previous one and skips " + c("lo") + " when others exist.")

CONN = fig(
    box(20, 30, 190, 56, "Tables of sockets", "/proc/net/tcp tcp6 udp udp6", "dark")
    + box(250, 30, 190, 56, "Live inodes", "set of socket inodes", "blue")
    + diamond(570, 58, 190, 70, "In the cache?", "amber")
    + box(700, 30, 180, 56, "Known pid", "from the cache", "navy")
    + box(250, 140, 190, 56, "Scan /proc/*/fd", "cached pids first", "blue")
    + box(500, 140, 190, 56, "socket:[N] found", "cache[N] = pid", "navy")
    + box(730, 140, 150, 56, "Not found", "retry in 5 cycles", "grey")
    + arrow(212, 58, 248, 58) + arrow(442, 58, 474, 58)
    + arrow(666, 58, 698, 58, "#16a34a", False, "yes") + arrow(570, 94, 570, 120, "#d97706", False, "no", 584, 112)
    + path([(570, 120), (345, 120), (345, 138)], "#d97706")
    + arrow(442, 168, 498, 168) + arrow(692, 168, 728, 168, "#475569", True, "gone", 710, 160),
    900, 220, "«FIG» — Finding the process of a connection without scanning every file descriptor every time")

S6 = p("A line of " + c("/proc/net/tcp") + " gives two addresses, the state, the queue sizes and an <i>inode</i>, but not "
       "the process. The process is found by looking for a descriptor that points to " + c("socket:[inode]")
       + ". Doing that for every connection on every cycle would open the fd directory of every process, so the "
       "result is kept.", lead=True) + \
    table(["Item", "How it is decoded"], [
        ["IPv4 address", "8 hex digits: the 32-bit number in the byte order of the machine (" + c("to_ne_bytes") + "), then dotted"],
        ["IPv6 address", "32 hex digits: four 32-bit words, each in machine order; shown compressed in brackets"],
        ["Port", "hexadecimal"],
        ["TCP state", "01 ESTAB, 02 SYN_SENT, 03 SYN_RECV, 04 FIN_WAIT1, 05 FIN_WAIT2, 06 TIME_WAIT, 07 CLOSE, 08 CLOSE_WAIT, "
         "09 LAST_ACK, 0A LISTEN, 0B CLOSING"],
        ["UDP state", "01 (connected) is ESTAB; everything else is “—”"],
        ["Remote “unset”", "address 0 and port 0 are shown as " + c("*")],
        ["Queues", c("tx_queue:rx_queue") + " in hex"],
    ], "«TAB» — Decoding /proc/net/tcp and its relatives") + CONN + \
    p("The algorithm of " + c("conn::refresh_with()") + ":") + \
    table(["Step", "What happens"], [
        ["1", "Read the four tables and parse them. A malformed line is skipped"],
        ["2", "Forget cache entries whose inode is no longer in any table"],
        ["3", "Collect the inodes that must be found: new ones, and the ones that failed earlier only when the retry "
         "counter (" + c("RETRY_EVERY") + " = 5 cycles) has run out"],
        ["4", "If there are any, " + c("scan_fds()") + " walks the processes: first the pids already in the cache, then the "
         "others; it stops as soon as every wanted inode has been found. Pid 2 (kernel threads) is skipped"],
        ["5", "A process whose " + c("fd") + " folder cannot be opened (permission denied, or gone) is remembered with its "
         "start time and not tried again while it is the same process"],
        ["6", "Attach to each connection the pid from the cache, or none"],
    ], "«TAB» — The inode to pid search") + \
    note("The connections are collected only while Network &amp; Disks is the visible section, and then only on every "
         "second refresh (" + rif("Performance") + "). Where the owner cannot be known (a socket of another user "
         "without root, a kernel socket) the process column shows “—”.", "Cost.")

S7 = p("The disk view combines a kernel counter file, a few static files and the mount table.", lead=True) + \
    table(["Quantity", "Formula (two samples of /proc/diskstats, interval dt)"], [
        ["Which devices", "Whole physical disks only: names starting with " + c("loop") + ", " + c("ram") + ", " + c("zram") + ", " + c("dm-") + ", "
         + c("md") + ", " + c("sr") + " or " + c("fd") + " are skipped; a name is a disk if " + c("/sys/block/NAME") + " exists (partitions live "
         "inside the disk's folder, not in " + c("/sys/block") + "). Without " + c("/sys/block") + " the name is judged by pattern (" + c("sda1") + ", "
         + c("nvme0n1p2") + ", " + c("mmcblk0p1") + " are partitions)"],
        ["Throughput", "sectors × 512; <b>rate = Δbytes ÷ dt</b>, separately for read and write"],
        ["IOPS", "<b>Δ(reads completed) ÷ dt</b> and <b>Δ(writes completed) ÷ dt</b>"],
        ["Busy time", "<b>util = Δio_ticks ÷ (dt × 1000)</b> (the field is milliseconds spent with I/O in progress), clamped to 0–1"],
        ["Latency", "<b>(Δread_ticks + Δwrite_ticks) ÷ (Δreads + Δwrites)</b> in ms: the average time per operation. No latency if no operation happened"],
        ["Queue", "the “I/Os currently in progress” field, as it is now"],
        ["Kind", c("nvme…") + " is NVMe; otherwise " + c("queue/rotational") + " = 1 is HDD, 0 is SSD, missing is unknown"],
        ["Model, size", c("device/model") + " with spaces squeezed; " + c("size") + " × 512. Read once per disk"],
        ["Disk temperature", c("device/hwmon/hwmon*/temp1_input") + " (NVMe, and SATA with " + c("drivetemp") + "); found once"],
    ], "«TAB» — Disk formulas") + \
    p("<b>Filesystems.</b> " + c("parse_mountinfo()") + " keeps only the mounts of real filesystem types (" + c("ext2/3/4") + ", "
      + c("btrfs") + ", " + c("xfs") + ", " + c("f2fs") + ", " + c("vfat") + ", " + c("exfat") + ", " + c("zfs") + ", " + c("nfs") + ", "
      + c("nfs4") + ", " + c("cifs") + ", " + c("smb3") + ", " + c("ntfs") + ", " + c("ntfs3") + ", " + c("fuseblk") + ", " + c("jfs") + ", "
      + c("reiserfs") + ", " + c("bcachefs") + ", " + c("hfsplus") + "), skips the mounts under Docker, Podman and snap folders, and "
      "drops duplicates (same mount point, or same device and same root: bind mounts of one subvolume). Octal "
      "escapes such as " + c("\\040") + " in paths are decoded. The disk of a mount is found from its source "
      "(" + c("disk_of_source()") + "): " + c("/dev/nvme0n1p2") + " is " + c("nvme0n1") + "; device-mapper and network sources have none.") + \
    table(["Quantity", "Formula (statvfs)"], [
        ["Total", c("f_blocks × f_frsize")],
        ["Used", c("(f_blocks − f_bfree) × f_frsize")],
        ["Available", c("f_bavail × f_frsize") + " (what an ordinary user may still write)"],
        ["Fraction used", "<b>used ÷ (used + available)</b>, the same as " + c("df") + ", so the bar fills to 100 % when "
         "an ordinary user can write no more, even if the root reserve is free"],
        ["Inodes", "<b>(files − ffree) ÷ files</b>, absent if the filesystem has no inode count"],
    ], "«TAB» — Filesystem formulas") + \
    p("<b>Why threads.</b> " + c("statvfs") + " on a network mount whose server has vanished can block for minutes, "
      "and nothing can interrupt it. " + c("FsMonitor") + " therefore starts one thread per mount point "
      + c("ngtop-statvfs") + ". Every 5th cycle the interface asks each one for a new measurement, but only those with "
      "no request in flight; it never queues a second. It collects answers that have already arrived without waiting "
      "(only the very first refresh waits, at most 300 ms, so the first screen is not empty). A mount with no answer "
      "for more than 5 seconds (" + c("FS_TIMEOUT") + ") is shown as unresponsive and the others are unaffected. A "
      "mount that disappears from the list has its thread closed.")

GPUFLOW = fig(
    box(20, 30, 200, 56, "Cards", "/sys/class/drm/cardN", "dark")
    + diamond(335, 58, 180, 70, "Driver known?", "amber")
    + diamond(570, 58, 190, 70, "Suspended?", "amber")
    + box(700, 30, 180, 56, "Read sysfs", "usage, VRAM, clocks, hwmon", "navy")
    + box(235, 150, 200, 56, "Unsupported", "unknown driver", "grey")
    + box(470, 150, 220, 56, "Discrete and asleep", "no metrics are read", "amber")
    + arrow(222, 58, 244, 58) + arrow(426, 58, 474, 58, "#16a34a", False, "yes")
    + arrow(666, 58, 698, 58, "#16a34a", False, "no")
    + arrow(335, 94, 335, 148, "#d97706", False, "no", 350, 128)
    + arrow(570, 94, 570, 148, "#d97706", False, "yes", 586, 128),
    900, 232, "«FIG» — What the GPU collector does with each card, before it reads anything (an awake NVIDIA card is read through nvidia-smi instead of sysfs, see below)")

S8 = p("GPU data comes from three places: the device's own files in sysfs; for who uses the GPU, the " + c("fdinfo") + " "
       "file of every open GPU descriptor of every process; and, for NVIDIA cards, the output of " + c("nvidia-smi") + " (described below).", lead=True) + GPUFLOW + \
    table(["State", "When", "What ngtop does"], [
        [c("Active"), "the card is awake", "reads everything"],
        [c("Idle"), "an <i>integrated</i> GPU whose runtime power management says “suspended” or “suspending”", "reads it normally: it shares the processor's "
         "package, so there is nothing to save and nothing to wake"],
        [c("Suspended"), "a <i>discrete</i> GPU with " + c("power/runtime_status") + " “suspended” or “suspending”, or " + c("power_state") + " “D3cold”",
         "reads <b>none of its usage, memory, clock, sensor or link files</b> (and never starts " + c("nvidia-smi") + " for it), so it is not woken; the screen says it is asleep and whether it is in D3cold"],
        [c("Unsupported"), "any other driver, such as nouveau", "shows the name; nothing else"],
        [c("NoTool"), "an NVIDIA card (driver " + c("nvidia") + ") and " + c("nvidia-smi") + " is not installed (exit status 127 from the " + c("sh") + " guard, locally or remotely; " + c("NotFound") + " only if the runner cannot start the command)",
         "shows the name and “detected — install nvidia-smi for metrics” (“install nvidia-smi” in the compact layout); checks again every 60 s (" + c("NOTOOL_RECHECK") + ")"],
        [c("NotResponding"), c("nvidia-smi") + " timed out (3 s), failed, returned no cards, or its latest sample is older than 10 s (" + c("MAX_AGE") + ")",
         "shows “nvidia-smi not responding — retrying” (“nvidia-smi not responding” in the compact layout); asks again after a pause that doubles from 2 s up to 30 s (" + c("BACKOFF_MAX") + ")"],
    ], "«TAB» — The six states of a GPU") + \
    p("<b>NVIDIA.</b> A card with the " + c("nvidia") + " driver that is not suspended is read through " + c("nvidia-smi") + ", not sysfs (and DRM " + c("fdinfo") + " clients are skipped for it so counters come only from " + c("nvidia-smi") + "). "
      "When the first awake NVIDIA card is seen, " + c("NvClient::spawn()") + " (" + c("src/collect/nvidia_worker.rs") + ") starts the " + c("ngtop-nvidia") + " thread, which runs every 2 s (" + c("POLL") + "). "
      "The interface thread never waits: " + c("apply_nvidia()") + " takes the latest ready result if it is at most 10 s old (" + c("MAX_AGE") + "), or treats it as " + c("NotResponding") + " if it is older. "
      "Only the awake cards are queried, passing " + c("-i <bus_id,...>") + " with the 8-digit PCI domain expected by " + c("nvidia-smi") + " (" + c("smi_bus_id()") + ", e.g. " + c("00000000:01:00.0") + ", matched back to " + c("/sys/class/drm") + " by " + c("norm_bus_id()") + " as " + c("0000:01:00.0") + "); when no card is awake, nothing is launched. "
      "Every command is wrapped by " + c("guarded()") + " in a small " + c("sh") + " guard (" + c("GUARD") + ") that runs on the observed machine (local or remote, through " + c("roots.run") + ") and, in the same shell that then " + c("exec") + "s " + c("nvidia-smi") + ", re-reads " + c("power/runtime_status") + " of each target card right before execution: if any card is " + c("suspended") + " or " + c("suspending") + " or its state cannot be read, the shell exits (code 97, " + c("GUARD_ASLEEP") + ") without starting " + c("nvidia-smi") + ", and the worker waits a full 2 s round (" + rif("GPUs that must not be woken") + "). "
      "On a remote machine each command goes through the persistent " + c("ssh") + " session and occupies it for its duration. "
      "In each round the worker runs the card query (" + c("--query-gpu=<fields> --format=csv,noheader,nounits") + ") and then " + c("pmon -c 1 -s um") + ". "
      "If the driver rejects a field in " + c("FIELDS_FULL") + " (" + c("is_unknown_field_error()") + "), the query is repeated with the basic list (" + c("FIELDS_BASE") + ") and the choice is remembered; " + c("[N/A]") + " and " + c("[Not Supported]") + " become “—”. "
      "If " + c("pmon") + " fails or is not supported, the worker falls back to " + c("--query-compute-apps=gpu_bus_id,pid,used_memory --format=csv,noheader,nounits") + ", which lists only CUDA processes (" + c("procs_compute_only") + ", shown in the title of the “GPU processes” table as “only CUDA processes are listed on this GPU” when that card is selected); " + c("pmon") + " is retried every 30 rounds (" + c("PMON_RETRY_EVERY") + "). "
      "If " + c("nvidia-smi") + " is missing (exit code 127 from the guard), the state is " + c("NoTool") + " (“install nvidia-smi”). "
      "Each command has a 3 s timeout (" + c("CMD_TIMEOUT") + "); on timeout or error the state is " + c("NotResponding") + " (“nvidia-smi not responding”) and the retry wait grows up to 30 s (" + c("BACKOFF_MAX") + "). "
      "Everything from " + c("nvidia-smi") + " (" + c("src/collect/nvidia.rs") + ") is treated as hostile: at most 256 KB (" + c("MAX_OUTPUT") + ") are read (" + c("lossy()") + ", " + c("limita()") + "), at most 64 cards (" + c("MAX_GPUS") + ") and 4096 processes (" + c("MAX_PROCS") + ") are kept, control characters are stripped from card names (capped at 96 chars, " + c("MAX_NAME") + "), and all numbers are bounded to finite ranges, never panicking. "
      "Known limits: on GeForce cards some fields are " + c("[Not Supported]") + ", graphics applications are not always listed, " + c("nouveau") + " is not supported, and the " + c("sh") + " guard does not check " + c("power_state=D3cold") + " (which is covered by the pre-check " + c("is_suspended()") + " on the data of the last refresh).") + \
    p("<b>Identity.</b> Read once per card (" + c("read_static()") + "): vendor and device from " + c("device/vendor") + " and "
      + c("device/device") + "; PCI slot and driver from " + c("device/uevent") + " (" + c("PCI_SLOT_NAME") + ", " + c("DRIVER")
      + "); the name from " + c("product_name") + " if the card has it, otherwise from the " + c("pci.ids") + " database "
      "(" + c("pci_name()") + ": the bracketed commercial name, with the maker in front), otherwise “AMD GPU [1002:xxxx]”. "
      "The PCI slot is the key of the device everywhere in the program.") + \
    table(["Driver", "Usage %", "Memory", "Clocks", "Other"], [
        [c("amdgpu"), c("gpu_busy_percent"), c("mem_info_vram_used / total") + ", " + c("mem_info_gtt_used / total"),
         c("pp_dpm_sclk") + " and " + c("pp_dpm_mclk") + ": the line with “*” is the current clock, the largest is the maximum",
         "integrated if the VRAM is 1 GiB or less (unified memory); hwmon temperatures, power, fan; PCIe link"],
        [c("i915"), "the busiest engine, from " + c("fdinfo"), "system memory resident for GPU clients (" + c("fdinfo") + ")",
         c("gt_act_freq_mhz") + " (else " + c("gt_cur_freq_mhz") + "), max " + c("gt_max_freq_mhz"),
         "integrated if the PCI slot starts with " + c("0000:00:") + "; otherwise a discrete Arc card, with PCIe link"],
        [c("xe"), "the busiest engine, from " + c("fdinfo"), "as i915", c("tile0/gt0/freq0/act_freq") + " (else " + c("cur_freq")
         + "), max " + c("max_freq"), "only tile 0, gt 0 is read"],
    ], "«TAB» — What is read per driver") + \
    p("The temperature shown is the hwmon “junction” sensor, else “edge”, else the first one. Power is "
      + c("power1_average") + " (else " + c("power1_input") + "), in microwatts divided by 10<sup>6</sup>; the fan is "
      + c("fan1_input") + " in RPM, or " + c("pwm1") + " ÷ 255 as a percentage. The PCIe text “4.0 ×16” is built from "
      + c("current_link_speed") + " in GT/s (2, 5, 8, 16, 32, 64 map to PCIe 1.0 to 6.0) and " + c("current_link_width") + ".")

S9 = p("Modern kernels (amdgpu, i915, xe) publish, for each open GPU descriptor, a file " + c("/proc/[pid]/fdinfo/N")
       + " with the <i>DRM client</i>'s counters. That is the only source of “who is using the GPU”, and it needs no "
       "special library.", lead=True) + \
    table(["Key in fdinfo", "Meaning", "Used as"], [
        [c("drm-driver"), c("drm-pdev"), "the card the client works on"],
        [c("drm-client-id"), "the client's number, unique on that card", "with " + c("drm-pdev") + ", the client's identity"],
        [c("drm-engine-NAME"), "nanoseconds of work done on engine NAME, cumulative (amdgpu, i915)", "engine busy %"],
        [c("drm-engine-capacity-NAME"), "how many identical engines NAME stands for", "divisor of the busy %"],
        [c("drm-cycles-NAME") + ", " + c("drm-total-cycles-NAME"), "cycles used and cycles elapsed (xe)", "engine busy %"],
        [c("drm-memory-vram") + ", " + c("drm-resident-vram*") + " / " + c("local*"), "video memory of the client",
         "VRAM; <i>resident</i> is preferred when present, and the two families are never added"],
        [c("drm-memory-gtt") + ", " + c("drm-resident-system*"), "system memory the GPU maps", "GTT; memory shown for integrated GPUs"],
    ], "«TAB» — The fdinfo keys ngtop reads") + \
    p("<b>Finding the files.</b> A process's GPU descriptors are the ones in " + c("/proc/[pid]/fd") + " that point to "
      + c("/dev/dri/…") + ". Listing them for every process every cycle would be slow, so the answer is cached per pid "
      "with the process's start time. A new pid is scanned at once; a process that has GPU descriptors is rescanned every 5th "
      "cycle, one without every 20th (both staggered by pid so not all happen together); a process whose fd folder was "
      "denied is not scanned again until it is a different process. Kernel threads are never scanned. Then only the "
      "known descriptors are read, each cycle.") + \
    p("<b>One client, many processes.</b> A DRM client can be reachable from several processes at once (a parent and its "
      "forked children share the descriptor). If each were counted, one workload would appear many times. The collector "
      "walks pids in ascending order and keeps the first process that shows a given (card, client id); the others are "
      "ignored. The client is attributed to the <b>lowest pid</b> that holds it.") + \
    table(["Quantity", "Formula"], [
        ["Engine busy % of a client", "Time counters: <b>Δns ÷ (dt × 10<sup>9</sup>) ÷ capacity × 100</b>. Cycle counters: "
         "<b>Δused ÷ Δtotal × 100</b>. Clamped to 0–100. A client seen for the first time has no “before”: zero"],
        ["Engine kind", "By the name prefix: " + c("enc") + " encode; " + c("dec") + ", " + c("vcn") + ", " + c("jpeg") + " decode; "
         + c("video") + ", " + c("vcs") + ", " + c("vecs") + " video; " + c("compute") + ", " + c("ccs") + " compute; " + c("copy") + ", "
         + c("dma") + ", " + c("bcs") + ", " + c("sdma") + " copy; anything else (gfx, render) is graphics"],
        ["GPU % of a process", "The busiest of its graphics, compute and copy engines, after adding up all its clients "
         "on each engine, at most 100. It is the engine that limits the work, so sums of different engines are not used"],
        ["Encode and decode % of a process", "The sums of its encode engines, and of its decode and video engines, at most 100"],
        ["GPU memory of a process", "VRAM if the client has any, otherwise its resident system memory"],
        ["GPU % shown in the process table", "The sum over the cards the process uses, at most 100"],
        ["Usage of a card", "amdgpu: the file " + c("gpu_busy_percent") + ". If it is missing, and for Intel always, the busiest engine over all clients"],
        ["Kind of work", "Graphics if any graphics engine has ever worked, else compute, video, copy; none if all are zero"],
    ], "«TAB» — GPU formulas")

S10 = p("The Services section needs systemd. It is available only if " + c("/run/systemd/system") + " exists "
        "(" + c("systemd_present()") + "); otherwise the section says so. All the following happens in the services "
        "thread (" + rif("Threads") + ").", lead=True) + \
    table(["Data", "How", "Notes"], [
        ["List of units", c("ListUnits") + " on the systemd manager, system or session bus", "sorted by name, case-insensitive"],
        ["Enabled / disabled", c("ListUnitFiles") + ": the unit file state of each name", "slow; cached for 60 s ("
         + c("UNIT_FILES_TTL") + "), refreshed in the background; instances of a template (" + c("foo@bar.service")
         + ") take the state of " + c("foo@.service")],
        ["Main pid, cgroup", c("Get") + " of " + c("MainPID") + " and " + c("ControlGroup") + ", only for active services",
         "two small calls instead of " + c("GetAll") + ", which is costly for systemd; the cgroup is kept while the unit lives, the "
         "pid is asked again when the sub-state changes or after 10 s"],
        ["Memory and CPU of a unit", "straight from the cgroup: " + c("memory.current") + ", " + c("memory.peak") + ", " + c("cpu.stat") + " (" + c("usage_usec") + ")",
         "no D-Bus call per row. <b>CPU % = Δusage_usec ÷ 10<sup>6</sup> ÷ dt ÷ ncores × 100</b>; shown after the second reading"],
        ["Detail of a unit", c("GetAll") + " on the Unit interface and on the Service / Socket / Mount / Swap / Scope interface",
         "description, states, since when, restarts, memory, tasks, dependencies"],
        ["Sessions", "login1 " + c("ListSessions") + " and " + c("GetAll") + " per session", "user, seat, tty, type, host, since"],
        ["Boot times", "the manager's " + c("Firmware/Loader/InitRD/Userspace/Finish") + c("TimestampMonotonic"),
         c("boot_phases()") + ": firmware = firmware − loader; loader; kernel = initrd (or userspace); initrd = userspace − initrd; services = finish − userspace"],
        ["Slowest units", c("systemd-analyze blame") + " as a subprocess", c("parse_blame()") + " reads “1min 2.5s”, “696ms”"],
        ["Journal", c("journalctl -u NAME -n 50 -o json") + " as a subprocess (" + c("--user-unit") + " for the user scope)",
         "15 s limit; " + c("parse_journal_json()") + " skips bad lines, decodes a byte-array message, and sorts by time"],
    ], "«TAB» — How the Services data is obtained") + \
    p("<b>Bus and timeouts.</b> The system scope uses the system bus, the user scope the session bus. Queries wait at most 10 "
      "seconds (" + c("QUERY_TIMEOUT") + "); actions use a separate connection that waits up to 120 seconds ("
      + c("ACTION_TIMEOUT") + ") because polkit may be waiting for a password. Subprocesses are run by "
      + c("run_with_timeout()") + ", which polls every 20 ms and kills the child at the limit.") + \
    warn("In the Services section, <b>journalctl</b> and <b>systemd-analyze</b> are the only external programs ngtop runs. If one is missing, the "
         "message “journalctl not found” appears in the journal panel; the rest keeps working.", "Subprocesses.")

CHAPTER = ("Data sources", [
    ("How the collectors read", S1),
    ("CPU, temperature", S2),
    ("Memory, swap, load", S3),
    ("Processes", S4),
    ("Network", S5),
    ("Connections: inode to pid", S6),
    ("Disks and filesystems", S7),
    ("GPU: devices", S8),
    ("GPU: processes and the fdinfo files", S9),
    ("Services, sessions, boot", S10),
])
