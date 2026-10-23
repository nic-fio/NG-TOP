# Copyright (c) 2026 Nicola Fiorillo
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
"""User Manual, chapter 10 — permissions and running as root.
Sources: src/collect/procdetail.rs (cwd, exe, fd, environ, io, smaps_rollup), src/collect/conn.rs (socket owners from
/proc/*/fd), src/collect/gpu.rs (fdinfo), src/ui/sections/processi.rs (signals, nice), src/ui/sections/servizi/ (journal,
DENIED_TEXT), README.md (known limits), docs/decisions-and-history.md (D3)."""
from build import c, key, note, p, rif, table, term, tip, ui, ul, warn

S_PRINCIPLE = p("ngtop is made to work for a <b>normal user</b>. It reads the information that the kernel publishes for everybody "
                "(<code>/proc</code> and <code>/sys</code>) and needs no special rights for it. You get the CPU, memory, load, "
                "network speeds, disks, file systems, the list of all processes with their CPU and memory, and the services.", lead=True) + \
    p("A few details are private to their owner, or to root. When ngtop is not allowed to read a value it shows a dash, <b>—</b>, "
      "and goes on. It never shows a made-up number, and it never stops.") + \
    note("A dash does not mean zero and does not mean an error. It means: \"this value is not available to you\".")

S_DASH = p("This is where you meet a dash as a normal user, and the reason.", lead=True) + \
    table(["Where", "What is missing", "Why"], [
        [ui("Processes") + " detail, tab " + ui("General"), ui("Directory") + " and " + ui("Executable") + " of processes of other users",
         "The system allows only the owner (and root) to read them."],
        [ui("memory & I/O") + " panel", ui("PSS") + ", " + ui("Shared") + ", " + ui("Swap") + " of other users' processes",
         "The detailed memory accounting of a process is private."],
        [ui("memory & I/O") + " panel", ui("Read") + ", " + ui("Write") + ", " + ui("R/s") + ", " + ui("W/s") + " of other users' processes",
         "The input/output counters of a process are private."],
        [ui("Files") + ", " + ui("Env") + " tabs", "<code>— open files (permission denied?)</code>, <code>— environment (permission denied?)</code>",
         "The list of open files and the environment of other users' processes are private."],
        [ui("Connections") + " (Network &amp; Disks)", ui("PID") + " and " + ui("Process") + " are <b>—</b> for the sockets of other users or of the system",
         "To know who owns a socket, ngtop must look at the open files of the process, which is private."],
        [ui("GPU processes"), "The processes of other users do not appear, or have <b>—</b> in " + ui("VRAM") + " and " + ui("Type"),
         "The GPU statistics are in files private to each process."],
        [ui("journal") + " (Services)", "<code>(journal empty or unreadable…)</code>", "Reading the journal needs the group <code>systemd-journal</code>, or root."],
        [ui("Services") + " actions", ui("Permission denied: a polkit agent is needed, or run ngtop with sudo"), "Starting and stopping system units needs authorisation (" + rif("Passwords, polkit and sudo") + ")."],
    ], "«TAB» — What a normal user does not see") + \
    p("What you do <b>see</b> for every process, whoever owns it: the PID, the name, the command line, the user, the number of threads, "
      "the resident memory, the CPU, the state, the nice value and the open network sockets of the whole machine. And everything about your "
      "own processes.")

S_ACT = p("Viewing and acting are different. As a normal user you can:", lead=True) + \
    ul(["send signals to <b>your own</b> processes; for the others the system says <code>permission denied (EPERM)</code>;",
        "<b>raise</b> the nice value of your own processes (F8); lowering it needs root: <code>root is needed to lower the nice value</code>;",
        "act on <b>user</b> units without a password; on system units, only if the system authorises you."]) + \
    p("See " + rif("What ngtop protects you from") + " for the checks that ngtop adds on top of the permissions of the system.")

S_ROOT = p("To see everything, run ngtop as root:", lead=True) + \
    term("""$ sudo ngtop""") + \
    p("Now the dashes of the table above turn into numbers, the services can be started and stopped without questions, and every "
      "process can receive a signal.") + \
    warn("As root, a signal or a service action can stop <b>anything</b>, including the programs the machine needs. ngtop's protections (it "
         "never signals PID 1, and it asks twice for large subtrees) stay, but the responsibility is yours. Run ngtop as root only when you "
         "need to, and leave it as soon as you are done.", "Root can do much harm.") + \
    ul(["With " + c("sudo") + ", the configuration file may be a different one: depending on how sudo is set up, root may use its own home "
        "folder. To be sure which file is used, add " + c("--config FILE") + ".",
        "You do not need root just to look at the Overview, the Network or the processes of your own user.",
        "Some systems mount <code>/proc</code> so that users see only their own processes. On those, a normal user sees an almost empty "
        "list, and root sees everything."])

S_REMOTE = p("On a <b>remote machine</b> (" + rif("Monitoring another machine") + ") the rules are the same, but they apply to the <b>user you log in "
             "as on the server</b>, not to you on your PC. If you log in as a normal user, you see the dashes of the table above for the other users' processes. "
             "To see everything, log in as root: " + c("ngtop root@server") + " (if the server allows it). The journal of the server needs the group "
             "<code>systemd-journal</code>, or root.", lead=True)

S_SAFE = p("Is it safe to leave ngtop open as root? ngtop only reads files and, when you ask, sends a signal, changes a nice value, or "
           "asks systemd for an action. It does not open network ports, does not write anything outside the configuration file "
           "(only with " + key("W") + "), and does not run other programs, except <code>journalctl</code> and "
           "<code>systemd-analyze</code> to read the journal and the boot times.", lead=True)

CHAPTER = ("Permissions and running as root", [
    ("What a normal user can see", S_PRINCIPLE),
    ("What you see without administrator rights", S_DASH),
    ("What a normal user can do", S_ACT),
    ("Running as root", S_ROOT),
    ("Permissions on a remote machine", S_REMOTE),
    ("What ngtop does on your system", S_SAFE),
])
