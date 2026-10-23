from build import arrow, box, c, code, fig, note, p, path, rif, seq, sysf, table, term, text, tip, warn, zone

ABSTRACTION = fig(
    box(20, 40, 200, 60, "Collectors", "src/collect/, ui/tz.rs", "navy")
    + box(290, 40, 200, 60, "Roots", "proc, sys, etc + src + run", "blue")
    + arrow(222, 70, 288, 70) + text(255, 60, "read", 11)
    + zone(560, 12, 320, 118, "This machine")
    + box(580, 40, 135, 56, "LocalSource", "std::fs, statvfs", "dark")
    + box(730, 40, 135, 56, "LocalRunner", "Command + timeout", "dark")
    + zone(560, 150, 320, 118, "Another machine, through ssh")
    + box(580, 178, 135, 56, "RemoteSource", "cache + manifest", "light")
    + box(730, 178, 135, 56, "RemoteRunner", "x requests", "light")
    + arrow(492, 62, 558, 62) + arrow(492, 82, 578, 200)
    + box(20, 150, 200, 60, "Tests", "fixtures, in-memory source", "grey")
    + path([(120, 150), (120, 128), (390, 128), (390, 102)], "#475569", True),
    900, 290, "«FIG» — Every read goes through Roots; behind it can be the local machine, a fixture, or a remote machine")

ROUND = seq([
    ("ngtop", "refresh thread", "navy"),
    ("ssh", "one process, kept open", "grey"),
    ("sh", "agent.sh, in memory", "dark"),
    ("Kernel", "/proc, /sys", "dark"),
], [
    ("sep", "begin_refresh(): the manifest in one request"),
    (0, 1, "p, t …, T …, G …, M …, z N"),
    (1, 2, "stdin"),
    (2, 3, "read the files"),
    (3, 2, "contents", True),
    (2, 1, "records, then Z N", True),
    (1, 0, "stdout", True),
    ("note", 0, "the collectors read from the cache"),
], "«FIG» — One refresh: a single round trip")

S1 = p("Version 1.5 lets ngtop show another machine exactly as it shows this one: " + c("ngtop user@host") + ". Nothing is installed "
       "on the server. The only requirements are an ssh server and a POSIX shell with " + c("cat") + ", which any Linux has, busybox included. "
       "Inside ngtop only the <i>place where bytes are read</i> changes; the formulas, the graphs and the sections are the same code.", lead=True) + ABSTRACTION + \
    table(["Item", "Where", "Role"], [
        [c("Source"), c("src/source/mod.rs"), "The trait for every read of the observed system: " + c("read()") + ", " + c("read_into()") + ", " + c("read_dir()") + ", " + c("read_link()") + ", " + c("owner_uid()")
         + ", " + c("statvfs()") + ", " + c("exists()") + ", " + c("read_text()") + ". An implementation never panics: what is missing or forbidden is an " + c("Err")],
        [c("begin_refresh()"), c("Source"), "Called once at the start of every refresh (first line of " + c("System::refresh()") + "). The local source does nothing; the remote one collects here, in one round trip, the files that the collectors are about to read"],
        [c("remote_status()") + ", " + c("page_size()") + ", " + c("clock_ticks()"), c("Source"), "Connection state and latency for the tab bar; page size and clock ticks of the <i>observed</i> machine (a remote one may differ from this one)"],
        [c("LocalSource"), c("src/source/mod.rs"), "The machine ngtop runs on, with ordinary system calls; " + c("read_into()") + " reuses the caller's buffer"],
        [c("CommandRunner"), c("src/source/mod.rs"), c("run(argv, timeout)") + ": an external command on the observed system. No shell: the arguments are passed as they are"],
        [c("LocalRunner"), c("src/source/mod.rs"), "Runs the command here and reads stdout and stderr in threads, so a command with more than 64 KB of output does not stall until the time limit"],
        [c("Roots"), c("src/collect/mod.rs"), "The three folders " + c("proc") + ", " + c("sys") + ", " + c("etc") + " plus the two handles " + c("src") + " and " + c("run") + ". " + c("Roots::local()") + " is the default; "
         + c("Roots::with_dirs()") + " points at a fixture folder with a local source; " + c("Roots::remote(src, run)") + " keeps the paths " + c("/proc") + ", " + c("/sys") + ", " + c("/etc") + " but reads them on the other machine"],
        [c("RemoteStatus"), c("src/source/mod.rs"), "Target, host name, latency in milliseconds, bytes of the last update, and a " + c("RemoteState") + ": " + c("Connected") + ", " + c("Reconnecting(message)") + " or " + c("Failed(message)")],
    ], "«TAB» — The abstraction") + \
    p("The path of a file is always the path on the observed system (" + c("/proc/1/stat") + "), already including the fixture folder when a test uses one. "
      "The time zone (" + c("Tz::load(&roots)") + ") and the user table (" + sysf("/etc/passwd") + ") are read through the same source, so a remote machine is shown with <i>its</i> clock and <i>its</i> user names.") + \
    note("The rule is checked by a test. " + c("nessuna_lettura_diretta_fuori_da_source") + " in " + c("tests/source_only.rs") + " scans the production code of " + c("src/collect/*.rs") + " and "
         + c("src/ui/tz.rs") + " and fails if it finds " + c("std::fs::") + ", " + c("File::open") + " or a free call to " + c("read_dir") + ", " + c("read_link") + ", " + c("metadata") + " or " + c("statvfs")
         + ". Another test, " + c("cpu_e_memoria_da_sorgente_in_memoria") + ", runs a whole " + c("System") + " on an in-memory source. A collector that opens a file directly would work locally and silently show nothing "
         "remotely: the test makes that impossible (" + rif("Add a collector") + ").", "Enforced.")

S2 = p("The connection is made <i>before</i> the interface starts (" + c("connect_remote()") + " in " + c("src/main.rs") + "), because ssh may need to ask for a password or the confirmation "
       "of a host key on the terminal, and it can only do that while the terminal is still normal. Then one ssh process is kept for the whole run.", lead=True) + \
    table(["Step", "What happens"], [
        ["Command", c("ssh -T -o ServerAliveInterval=15 -o ServerAliveCountMax=3 [-C] [-p PORT] DEST env TERM=dumb sh") + ". " + c("-T") + ": no terminal. " + c("-C") + ": ssh compression, on by default. "
         + "A dead link is noticed after 45 s. " + c("env TERM=dumb") + " keeps remote profiles that call " + c("tput") + " quiet. The program is " + c("ssh") + " from the " + c("PATH") + ", or the words of the variable "
         + c("NGTOP_SSH") + " (for example " + c("ssh -F /tmp/x/ssh_config") + "); " + c("NGTOP_SSH=local-sh") + " runs a local " + c("sh") + " instead of ssh, which is how the protocol is tested without a network"],
        ["Target", c("Target::parse()") + " accepts " + c("host") + ", " + c("user@host") + ", " + c("ssh://user@host:2222") + " and IPv6 in brackets. It refuses a leading “-” (which ssh would read as an option), spaces, control characters and unusual characters in user and host"],
        ["The agent", "The client writes " + c("NG='<nonce>'") + " and then the text of " + c("src/remote/agent.sh") + " (included in the binary with " + c("include_str!") + ") to the standard input of the remote " + c("sh") + ". The script is executed "
         "as it arrives: <b>it is never written to a file on the server</b>"],
        ["The nonce", "128 random bits from " + sysf("/dev/urandom") + " (a hash of the clock as a fallback), shown as “ngtop-” and 32 hex digits, chosen by the client for each session. It travels only on the standard input of the remote shell, never on a command line, so "
         + c("ps") + " on the server does not show it"],
        ["Capabilities", "The agent tests what the server has: " + c("timeout") + " (also with " + c("-k") + "), " + c("ls -q") + ", " + c("stat -L -c") + " and " + c("stat -f -c") + " (GNU and busybox forms), and asks " + c("getconf PAGESIZE") + " and " + c("getconf CLK_TCK")
         + " (fallbacks 4096 and 100 if the answer is not a number)"],
        ["READY", "The agent answers with one line: " + c("<NG> READY 111 4096 100") + ": three flags (" + c("stat -c") + " usable, " + c("stat -f") + " usable, " + c("timeout") + " present), page size, ticks per second. "
         "The client looks for that marker; any text before it (a banner, a login message) is ignored, but more than 1 MiB of it is an error. Waiting may last 300 s, because a password may be asked"],
        ["stderr", "Until READY the ssh messages go to the terminal (password prompts, host key questions). Afterwards the last 4 KiB are kept to explain a failure"],
    ], "«TAB» — Starting the session") + \
    p("Three small threads serve the process: one reads its standard error, one reads its standard output and sends the chunks on a channel, one writes the requests. The refresh thread only exchanges messages "
      "with them. A <b>round trip</b> is " + c("Session::exchange()") + ": it sends the request lines wrapped in a " + c("p") + " (ping) and a " + c("z N") + " (end, with a sequence number), then reads records until it sees " + c("Z N") + ". "
      "The time between sending and the " + c("P") + " record is the latency shown in the tab bar.") + ROUND

S3 = p("The reply is a stream of <b>records</b> on the standard output of the agent. The request is a text line per item, evaluated by the shell (" + c("eval") + "), in which the client has quoted every argument.", lead=True) + \
    term("""p
t '/proc/loadavg'
f '/proc/nonexistent'
l '/proc/self/exe'
u '/etc/hostname'
z 1""", "Request: one line per item") + \
    term("""ngtop-1c2f… READY 111 4096 100
ngtop-1c2f… P
ngtop-1c2f… F /proc/loadavg
0.56 1.10 1.10 2/823 660834

ngtop-1c2f… ! 0
ngtop-1c2f… F /proc/nonexistent

ngtop-1c2f… ! 2
ngtop-1c2f… L /proc/self/exe
/usr/bin/readlink

ngtop-1c2f… ! 0
ngtop-1c2f… U /etc/hostname
0

ngtop-1c2f… ! 0
ngtop-1c2f… Z 1""", "The reply, shortened: the nonce is really 128 bits") + \
    p("A record is a header line <b>" + c("<NG> TYPE ARGUMENT") + "</b>, a body of raw bytes, and a trailer <b>" + c("\\n<NG> ! RC") + "</b>. " + c("RC") + " is 0 or an errno (2 not found, 13 permission denied, 21 is a directory, 22 invalid, 110 timed out), which the client turns into an ordinary " + c("io::Error") + ". "
      "The shell always adds a newline before the trailer and the client removes it, so a file without a final newline, or with NUL bytes, arrives exact (the blank line above is that added newline after the file's own).") + \
    p("<b>How the body ends.</b> The body stops at the first <b>" + c("\\n<NG> ") + "</b>. There is no length to announce (" + c("/proc") + " files say size 0) and no escaping: because the nonce is random, secret and 128 bits long, no file or process on the "
      "server can contain it by chance or on purpose. If the stream does not start with the marker where a record should begin, the session is declared out of sync (" + c("remote protocol out of sync") + ") and is dropped. "
      + c("parse_record()") + " resumes its search where it stopped, so a body arriving in pieces is not rescanned from the start.") + \
    table(["Request (client)", "Record", "What it is", "Body"], [
        [c("t PATH") + ", " + c("T GLOB"), c("F"), "A text file read with the shell's own " + c("read") + " loop: no " + c("fork") + ", no " + c("exec"), "the file, text only (no NUL)"],
        [c("f PATH") + ", " + c("F GLOB"), c("F"), "A file read with " + c("cat"), "the file"],
        [c("G ARG…"), c("G"), "Many text files with <b>one</b> " + c("grep -H ''") + "", "lines " + c("path:line")],
        [c("B GLOB"), c("B"), c("C"), "Many files that may contain NUL (" + c("cmdline") + ", " + c("environ") + ") with one " + c("wc -c") + " and one " + c("cat") + ", " + "then " + c("wc -c") + " again as record " + c("Q")],
        [c("d PATH") + ", " + c("D GLOB"), c("D"), "Names of a folder", "names, each ended by NUL"],
        [c("l PATH"), c("L"), "Target of a link", "the target"],
        [c("K PARENT LEAF"), c("N"), c("K"), "Links of a folder-and-leaf pattern, with one " + c("readlink") + " per folder: paths (NUL) and targets (one per line)"],
        [c("M ARG…"), c("M"), "The links of many " + c("fd") + " folders with one " + c("ls -l"), "the " + c("ls") + " output"],
        [c("W PATH…"), c("W"), "Many links with one " + c("ls -ld"), "lines “… path -> target”"],
        [c("u PATH") + ", " + c("U ARG…"), c("U") + ", " + c("V"), "Owner uid; many with one " + c("stat -c '%u %n'"), "uid, or lines “uid path”"],
        [c("s PATH") + ", " + c("H PATH…"), c("S") + ", " + c("H"), c("statvfs") + ": frsize, blocks, available, inodes, free inodes, free blocks (with " + c("df -P -k") + " if " + c("stat -f") + " is missing)", "numbers, one line"],
        [c("h PATH"), c("S"), "The same for <i>one</i> network or FUSE filesystem, with a 2 s limit", "numbers; rc 110 if it timed out"],
        [c("x SECONDS ARGV…"), c("X") + ", " + c("Y"), "A command, with " + c("timeout") + " if present", "X: its stdout, rc = its exit status; Y: its stderr"],
        [c("p") + ", " + c("z N"), c("P") + ", " + c("Z N"), "Ping and end of reply (header only)", "none"],
    ], "«TAB» — The records") + \
    p("<b>Quoting.</b> " + c("sh_quote()") + " puts every argument in single quotes, writes " + c("'\\''") + " for a quote and " + c("'\"$NL\"'") + " for a newline; an argument with a NUL byte is refused (the call fails with “NUL in argument”). "
      "Arguments are quoted one by one and never concatenated into a command. Before building a path request, " + c("fetch()") + " also refuses any path that does not begin with “/” or contains NUL or newline.")

S4 = p(c("RemoteSource") + " (" + c("src/remote/source.rs") + ") sits between the collectors and the session. Collectors call " + c("read()") + " for single files, as always. The trick is that ngtop does not ask for them one by one.", lead=True) + \
    table(["Idea", "How it works"], [
        ["Learn the manifest", "Everything read between two " + c("begin_refresh()") + " calls is remembered (" + c("touched") + "). The next " + c("begin_refresh()") + " asks for all of it at once, in one request, so the collectors of the new refresh find their files already in the cache. "
         "The first refresh starts from a reasonable initial manifest (" + c("initial_manifest()") + "); in the first two laps it is added to what has been learned"],
        ["Patterns instead of paths", "When at least " + c("PATTERN_MIN") + " = 3 pids read the same file, the manifest keeps " + c("/proc/[0-9]*/stat") + " instead of three paths: processes born later are included without a new request. "
         "A path covered by a pattern that is missing from the reply does not exist, and no request is made for it"],
        ["Exact paths only if repeated", "A fixed path enters the manifest only if it was read in two laps in a row (after the first four laps); paths outside " + c("/proc") + " read recently stay for " + c("RECENT_LAPS") + " = 8 laps, so rare reads such as the detail of a selected process do not cost a lap each time"],
        ["Forget what ceased to matter", "A path read once and never again is not asked for again: it leaves the manifest by itself"],
        ["Slow-changing data less often", "Links, uids and folders outside " + c("/proc") + " (for example " + c("/sys/block") + ") are asked again every " + c("STICKY_LAPS") + " = 10 laps"],
        ["Misses are grouped", "A read that is not in the cache (a new process, a tab opened) is sent at once, but the request also carries the same file for all new pids, and, at the second miss of the same pattern in a lap, for all pids: a group of similar reads costs one round trip. "
         "Descriptors and files that need one " + c("cat") + " each are excluded from this bulk loading"],
    ], "«TAB» — How the manifest is learned") + \
    p("<b>Reading in bulk.</b> The cost on the server is the number of processes started, not the bytes, so the manifest is turned into the fewest processes possible (" + c("build_request()") + "):") + \
    table(["Data", "Mode", "Cost"], [
        ["Text files of the kernel: " + c("stat") + ", " + c("status") + ", " + c("meminfo") + ", …", c("t") + " / " + c("T") + ": the shell's " + c("read") + " loop", "no process; about one system call per byte, but no " + c("fork") + " + " + c("exec") + " (4 ms each in the measurements)"],
        ["Text files at exact paths of many pids", c("G") + ": one " + c("grep -H ''") + " for all", "one process. A file that is empty or unreadable gives no line and becomes " + c("NotFound"), ],
        [c("cmdline") + ", " + c("environ") + " (NUL inside)", c("B") + ": one " + c("wc -c") + ", one " + c("cat") + ", one " + c("wc -c") + " again", "three processes for all"],
        ["Files that need a binary-safe read", c("f") + " / " + c("F") + ": " + c("cat"), "one process per request"],
        ["Links of all descriptors", c("M") + ": one " + c("ls -l") + " for all " + c("fd") + " folders", "one process; also gives the list of folders"],
        ["Other links", c("K") + " (a " + c("readlink") + " per folder), " + c("W") + " (one " + c("ls -ld") + ")", "one process per folder / for all"],
        ["Owners", c("U") + ": one " + c("stat") + " for all", "one process"],
        ["Space of filesystems", c("H") + ": one " + c("stat -f") + " for all the local ones", "one process"],
    ], "«TAB» — Bulk loaders") + \
    p("<b>Double measure.</b> Reading many " + c("cmdline") + " files with one " + c("cat") + " loses the boundaries between them, so the lengths come from " + c("wc -c") + ", measured <i>before</i> and <i>after</i> the " + c("cat") + ". The second measure is compared with the first, ignoring the “total” line. The block is accepted only if every line of the second "
      "measure is identical to a line of the first (only processes that ended after the " + c("cat") + " may be missing) and the sum of the first measure equals the bytes of the " + c("cat") + ". Otherwise (a process changed its command line in between) the block is asked again, at most twice, then given up. A single " + c("cat") + " per file would cost seconds with thousands of processes.") + \
    p("<b>Network mounts.</b> " + c("statvfs") + " on a mount whose server is gone blocks for minutes. On the server, an agent that blocks blocks everything. So the filesystems are split: the types of the mounts come from " + c("/proc/self/mountinfo") + " (the field right after the “-”): "
      + c("nfs*") + ", " + c("cifs") + ", " + c("smb3") + ", " + c("smbfs") + ", " + c("ncpfs") + ", " + c("afs") + ", " + c("ceph") + ", " + c("glusterfs") + ", " + c("9p") + ", " + c("lustre") + ", " + c("davfs") + " and the FUSE types (except " + c("fusectl") + ") are excluded from the group request. "
      "Each is asked alone with " + c("h") + " and a 2-second limit on the server (" + c("statvfs_net()") + "); if it times out, or the server has no " + c("timeout") + ", it is not tried again for 30 laps. The local " + c("FsMonitor") + " threads (" + rif("Disks and filesystems") + ") apply the same idea on the interface side.") + \
    p("<b>Waiting for the lock.</b> Only one request travels at a time (the session is behind a " + c("Mutex") + "). A slow command (" + c("journalctl") + " for the Services section can take seconds) holds it. A refresh does not wait for it for more than " + c("SOURCE_WAIT") + " = 200 ms: "
      "it gives up that lap, keeps the old data and the screen simply does not update once. Requests have a limit of 10 s (" + c("REQUEST_TIMEOUT") + ").")

S5 = p("A link over the internet breaks. ngtop treats this as a normal event, not an error.", lead=True) + \
    table(["Moment", "What happens"], [
        ["A request fails", "The session is discarded, the state becomes " + c("Reconnecting(message)") + " and a background thread tries again. The cache is cleared, so the next refresh starts from a clean state, and the first lap after a reconnection treats all pids as new"],
        ["The retries", "The first after 0.5 s, then the wait doubles up to 10 s. Retries use " + c("-o BatchMode=yes") + ": ssh must not ask for a password in the middle of the interface. A new session re-reads the page size, the ticks and the host name"],
        ["A final answer", "If ssh says " + c("Permission denied") + ", " + c("Host key verification failed") + " or " + c("HOST IDENTIFICATION HAS CHANGED") + ", retrying is useless and may be unsafe: the state becomes " + c("Failed(message)") + " and the attempts stop"],
        ["On screen", "A fixed line, for as long as it lasts: “connection lost — reconnecting… (reason)” or “connection failed: reason — press q to quit”. The data on screen stays that of the last good refresh"],
        ["The interval", "If an update takes longer than the interval three times in a row (a single slow one, after a reconnection for instance, does not count), the interval widens to the next step (250, 500, 1000, 2000, 5000 ms) and the status line says “remote update takes N ms: interval widened to …” (" + c("adapt_interval()") + ")"],
    ], "«TAB» — When the connection fails")

S6 = p("The user sees the other machine as the local one, with a few differences that all say what they are.", lead=True) + \
    table(["Where", "What changes"], [
        ["Command line", c("ngtop [options] [TARGET]") + ", " + c("--host TARGET") + " (the same), " + c("--local") + " (ignore the " + c("host") + " key of the file), and the configuration key " + c("host") + ". " + c("--dump") + " and " + c("--snapshot") + " also accept a TARGET"],
        ["Tab bar", "At the right, " + c("⇄ host") + " (the remote host name if known, else the target) and the latency in ms, coloured green under 50, yellow under 200, red above (" + c("latency_color()") + "). The title of the help window adds the machine name"],
        ["Data", "All five sections work: GPU with the remote " + c("fdinfo") + ", the detail of the selected process, the connections only while the section is visible, Services without D-Bus (" + rif("Services without D-Bus") + ")"],
        ["Actions", "Signals (" + c("F9") + ", " + c("k") + "), nice (" + c("F7") + ", " + c("F8") + ") and service actions are <b>disabled</b>: the key shows the line “Not available on remote machines in this version”. In the filter box, " + c("k") + " is still text"],
        ["Time", "Clock and process start times use the time zone of the remote machine"],
    ], "«TAB» — The remote mode of the interface") + \
    note("The refresh of the local machine does not change: the cost of the local collectors, about 5 ms per refresh, is the same when the data comes from the network, because the network wait happens in one round trip at the start. See " + rif("Remote measurements") + ".", "Same code, same cost.")

S7 = p("A remote machine has no D-Bus the way " + c("DbusBackend") + " uses it. " + c("RemoteBackend") + " (" + c("src/collect/services/remote.rs") + ") implements the same " + c("Backend") + " trait with commands run through " + c("CommandRunner")
       + " and files read through " + c("Source") + ". It is chosen automatically when " + c("remote_status()") + " is not empty (" + c("services_for()") + " in " + c("src/ui/mod.rs") + ").", lead=True) + \
    table(["Data", "Command", "Fallback for an old systemd"], [
        ["Units", c("systemctl list-units --output=json"), "plain text output, parsed (" + c("parse_units_plain()") + ")"],
        ["Enabled / disabled", c("systemctl list-unit-files --output=json"), c("--plain --no-legend") + "; cached 60 s"],
        ["Main pid, cgroup, detail", c("systemctl show -p … --no-pager -- UNIT…") + ", 40 units per command", "—"],
        ["Memory and CPU", "none: the cgroup files read through " + c("Source"), "—"],
        ["Sessions", c("loginctl list-sessions --json=short") + " and one " + c("loginctl show-session"), c("--no-legend") + " parsed as plain text"],
        ["Boot phases", c("env LC_ALL=C systemd-analyze time"), "the kernel start time from " + c("btime") + " in " + c("/proc/stat")],
        ["Slowest units", c("systemd-analyze blame"), "—"],
        ["Journal", c("journalctl --unit=NAME -n 50 -o json --no-pager -q"), "—"],
    ], "«TAB» — How remote Services data is obtained") + \
    p("A unit name or a session id is checked against a strict list of characters before it is used (" + c("valid_unit_name()") + ", " + c("valid_session_id()") + "). User units are not reachable (the session bus is not): the section says "
      "“User units are not available on remote machines”. Actions return “Not available on remote machines in this version” from the backend itself, and the interface does not even offer them. If a command is missing, its part of the screen shows “—” or an explanation, like a local machine without systemd.")

S8 = p("The remote machine is not trusted. It may be misconfigured or compromised, and a user on it can choose the names of their files and processes. The question the design answers is: <i>what can a hostile server, or a hostile user on it, do to the machine "
       "that runs ngtop, and to the server?</i>", lead=True) + \
    table(["Threat", "Defence", "Where"], [
        ["A host name that becomes an ssh option (" + c("-oProxyCommand=…") + ")", "A target may not start with “-”, contain spaces or control characters; user and host are validated; the destination is one argument of " + c("Command") + ", never a shell string", c("Target::parse()")],
        ["Command injection on the server through a path or a unit name", "Every argument is single-quoted by " + c("sh_quote()") + "; NUL is refused; paths must be absolute with no newline; unit names and session ids are checked; " + c("argv") + " of a command is never joined", c("sh_quote()") + ", " + c("fetch()") + ", " + c("valid_unit_name()")],
        ["Data that forges a record or a line", "The end of a record is a secret 128-bit nonce that never appears on a command line. File names with newlines are skipped by the agent; " + c("ls -q") + " turns unprintable characters into “?”; " + c("grep -H") + " prefixes every line itself", c("agent.sh") + ", " + c("parse_record()")],
        ["A file name that forges the " + c("fd") + " folder of another process", "The " + c("ls -l") + " output is accepted only if each header is exactly " + c("/proc/<digits>/fd:") + ", unique, and the names are numeric; if anything is off, nothing is inserted and the folders are re-read one by one", c("absorb_ls()")],
        ["A command line that changes during the read, so one process gets another's", "The double measure of " + c("B") + " / " + c("Q"), c("absorb()")],
        ["A reply that never ends or is enormous", "1 MiB at most before READY; 128 MiB at most per reply (" + c("MAX_RESPONSE") + "); 10 s per request; 4 KiB of stderr kept; lists re-requested after a rejection are capped (200 links, 500 folders)", c("session.rs") + ", " + c("source.rs")],
        ["Numbers or lengths that lie", "Every slice and sum from the server is checked; a record that does not fit is dropped or the session is closed; no " + c("unwrap()") + " on them. The protocol parsers have tests with truncated and invented input", c("src/remote/tests.rs")],
        ["Writing on the server", "The agent only reads: " + c("cat") + ", " + c("grep") + ", " + c("ls") + ", " + c("stat") + ", " + c("readlink") + ", " + c("wc") + ", " + c("getconf") + ". It creates no file, listens on no port, and leaves nothing after the session. The commands of Services only read; the actions are disabled", c("agent.sh")],
        ["The agent hanging on a mount", "Network and FUSE mounts are asked alone with a 2-second limit, and a failed one is skipped for 30 laps; " + c("x") + " commands run under " + c("timeout") + " when the server has it", c("statvfs_net()")],
        ["The credentials", "ngtop never sees them. Passwords and keys are handled by the user's own ssh, with their " + c("ssh_config") + ", agent and " + c("known_hosts") + ". A reconnection uses " + c("BatchMode") + " and stops on a refused login or a changed host key", c("Transport::command()")],
    ], "«TAB» — What protects ngtop and the server") + \
    p("<b>What the design does not do.</b> The code documents these residual risks, and so does this manual:") + \
    table(["Risk", "Why it is accepted"], [
        ["A hostile server can show <i>false</i> data", "ngtop shows what the server says. The data are not signed: you must trust the machine you are monitoring, as you do when you read its output with " + c("ssh host cat /proc/…") + ". What it cannot do is run code on this machine through the protocol"],
        ["Two processes swapping command-line bytes with equal lengths", "The double measure compares lengths, not contents. A swap that keeps both lengths equal before and after the " + c("cat") + " but differs during it would not be detected. Effect: one command line shown wrongly, once"],
        ["Text files read by " + c("grep") + " differ in small ways", "An empty or unreadable file becomes “not found”, a final newline is always present, a file with NUL may be cut. This applies only to kernel text files chosen by " + c("file_mode()") + ". And a huge argument list (tens of thousands of paths, beyond " + c("ARG_MAX") + ") is not split: those files would appear absent"],
        ["Old busybox without " + c("timeout"), "Then there is no time limit on the server for network mounts and commands; ngtop still has its own limits on the answer"],
        ["No identification of a process across the network", "Another reason why signals and nice are disabled in this version: the " + c("pidfd") + " protection of " + rif("Signals: pidfd and start time") + " does not exist across ssh"],
    ], "«TAB» — Residual risks")

S9 = p("Measured on a VPS over the internet, and on containers with sshd (Alpine, Debian, Debian with systemd) with " + c("scripts/test-remote.sh --measure") + " (alias " + c("scripts/measure-remote.sh") + "), at a 1 s interval. "
       "These are measurements of one setup, not guarantees.", lead=True) + \
    table(["Quantity", "Value", "Note"], [
        ["Bytes per update, steady state", "about 34 KB raw", "the protocol bytes, before ssh compression"],
        ["Bytes per update, compressed", "about 3 KB", "with " + c("ssh -C") + " (default)"],
        ["Opening a new connection", "about 0.6 s", "why the session stays open"],
        ["One-shot summary " + c("ngtop --dump TARGET"), "1 to 1.7 s", "connection, the first two refreshes, the output"],
        ["CPU on the server", "about 1 %", "at 1 s; the agent uses " + c("cat") + ", " + c("grep") + ", " + c("ls") + ", with no process per file"],
        ["Local refresh", "about 5 ms", "unchanged: the local part of a remote refresh is as cheap as a local one"],
    ], "«TAB» — Remote measurements") + \
    p("<b>Why it is cheap.</b> The first design measured about 60 KB (about 17 KB compressed) per update. What brought it down is in this chapter: one round trip per refresh; patterns instead of paths; the caches of the local collectors (command line, owner, "
      "descriptors every 5th or 20th lap) which also reduce what is asked of the server; no process per file on the server (the shell's " + c("read") + " loop for text files, one " + c("grep") + ", one " + c("ls") + ", one " + c("stat") + " for many); and compression. "
      "On a slow link, if an update does not fit in the interval, the interval widens by itself (" + rif("Reconnection and the interval") + ").")

S10 = p("The choice was made at the start and recorded in " + c("decisions-and-history.md") + ": ngtop runs the <b>system's</b> " + c("ssh") + " command.", lead=True) + \
    table(["Reason", "Detail"], [
        ["The libraries need C", "The Rust libraries for ssh need cryptographic code written in C. That would end the one rule of the static build (" + rif("Dependencies: Rust only") + "): no C in the build"],
        ["The user keeps everything", "Keys, the agent, " + c("~/.ssh/config") + ", " + c("ProxyJump") + ", " + c("known_hosts") + ", certificates, hardware keys, jump hosts, the passwords asked on the terminal: all of it works as it does for the user's own " + c("ssh") + ", with nothing to configure in ngtop"],
        ["Nothing to maintain", "Security fixes to the protocol come with the system's ssh, not with a new version of ngtop"],
        ["The cost", "A process and three threads, and a dependence on an " + c("ssh") + " in the " + c("PATH") + ". It is the same dependence a user of " + c("scp") + " or " + c("rsync") + " already accepts"],
    ], "«TAB» — Why the ssh command and not a library")

CHAPTER = ("Remote monitoring", [
    ("Sources and runners", S1),
    ("The ssh session", S2),
    ("The record protocol", S3),
    ("RemoteSource: manifest and bulk reads", S4),
    ("Reconnection and the interval", S5),
    ("The remote mode of the interface", S6),
    ("Services without D-Bus", S7),
    ("Remote security", S8),
    ("Remote measurements", S9),
    ("Why the ssh command", S10),
])
