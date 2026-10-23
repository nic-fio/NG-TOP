from build import c, fig, flow, key, note, p, rif, seq, sysf, table, ui, warn

SIGNAL_SEQ = seq([
    ("SignalDialog", "src/ui/sections/processi.rs", "navy"),
    ("send_dialog()", "after Enter", "blue"),
    ("/proc/PID/stat", "start time", "dark"),
    ("Kernel", "pidfd, signal", "grey"),
], [
    ("sep", "when the dialog opens"),
    ("note", 0, "remember (pid, start) of all"),
    ("sep", "Enter (twice for a large subtree)"),
    (0, 1, "targets, tree_starts"),
    (1, 2, "start of the root"),
    (2, 1, "same as remembered?", True),
    (1, 1, "verified_subtree()"),
    ("sep", "for each target"),
    (1, 3, "pidfd_open(pid)"),
    (3, 1, "a descriptor tied to that process", True),
    (1, 2, "start time again"),
    (2, 1, "same as remembered?", True),
    (1, 3, "pidfd_send_signal(fd)"),
], "«FIG» — How a signal reaches only the process the user chose")

REMOTE_NOTE = note("On a remote machine there is no pidfd. The same identity (pid and start time) is checked by the script on the server in the same command that sends the signal; "
                   "the small window that remains is described in " + rif("Actions on the server") + ".", "Remote.")

S1 = p("A signal sent to a pid is dangerous for one reason: <b>pids are reused</b>. Between the moment the user sees "
       "a process in the list and the moment they press Enter, that process may have ended and the kernel may have given "
       "its number to another one. A naive “kill the pid” would then kill an innocent. ngtop closes this window in three "
       "ways.", lead=True) + \
    table(["Defence", "How"], [
        ["Identify by pid <i>and</i> start time", "A process is the pair (pid, " + c("starttime") + "), the field 22 of " + c("/proc/PID/stat") + ", in clock ticks since boot. "
         "Two processes with the same pid never have the same start time"],
        ["Remember, then check", "The pairs are remembered when the dialog opens; at the moment of sending, the root is checked first, each target again, and each once more inside " + c("send_signal()")],
        ["Hold the process, not the number", "Linux 5.3 and later give a <b>pidfd</b>: a descriptor that refers to one process for good. After " + c("pidfd_open(pid)") + ", "
         "the start time is read again; if it is the expected one, the descriptor is the process the user chose, and the signal goes through " + c("pidfd_send_signal") + ", "
         "which cannot reach another process even if the pid is reused a microsecond later"],
    ], "«TAB» — Three defences against pid reuse") + SIGNAL_SEQ + \
    p(c("send_signal()") + " is the only function that sends a signal. Its order is deliberate: <b>open first, check after</b>. If it checked "
      "and then opened, the pid could change in between; opened first, the descriptor and the check refer to the same process.") + \
    table(["Situation", "What " + c("send_signal()") + " does"], [
        [c("pidfd_open") + " works, start time equals the expected one", c("pidfd_send_signal") + " on the descriptor"],
        [c("pidfd_open") + " works, start time differs", "Nothing is sent: “the pid now belongs to another process (reused)”"],
        [c("pidfd_open") + " fails with ESRCH", "“the process no longer exists (ESRCH)”"],
        [c("pidfd_open") + " fails for another reason (old kernel, or the syscall is not allowed)", "Checks the start time, then falls back to " + c("kill()") + ". It is less strong, "
         "but the check precedes it immediately"],
        [c("pidfd_send_signal") + " fails with something other than ESRCH", "Falls back to " + c("kill()") + " for this pid, still right after the check"],
        ["The kernel refuses (EPERM, EACCES)", "The error is shown as text, e.g. “permission denied (EPERM)”"],
    ], "«TAB» — The outcomes of send_signal") + \
    p("No C library is involved: the calls are in " + c("rustix") + " (" + c("pidfd_open") + ", " + c("pidfd_send_signal") + ", " + c("kill_process") + ", "
      + c("setpriority_process") + "), which makes the raw system calls.") + \
    REMOTE_NOTE

S2 = p("The dialog (" + c("SignalDialog") + ", key " + key("F9") + " or " + key("k") + ") lets the user choose a signal and whether it also goes to the descendants.", lead=True) + \
    table(["Signal", "Name", "Description in the dialog"], [
        ["15", "SIGTERM", "ask to quit (recommended) — first in the list, so " + key("Enter") + " alone is the polite choice"],
        ["9", "SIGKILL", "stop now, unsaved work lost"],
        ["19", "SIGSTOP", "pause"],
        ["18", "SIGCONT", "resume"],
        ["1", "SIGHUP", "reload configuration"],
        ["2", "SIGINT", "interrupt (like Ctrl-C)"],
        ["10", "SIGUSR1", "user signal 1"],
        ["12", "SIGUSR2", "user signal 2"],
    ], "«TAB» — The eight signals of the dialog") + \
    p("Up and Down choose the signal, " + key("Space") + " toggles “whole subtree”, " + key("Enter") + " sends, " + key("Esc") + " cancels. These are the rules:") + \
    table(["Rule", "Detail", "Function"], [
        ["PID 1 is never touched", "Not alone, not as part of a subtree. Pressing Enter on it sends nothing and says so", c("protected()") + ", " + c("targets()")],
        ["ngtop itself", "Allowed alone, with the warning “you are signalling ngtop itself”; skipped when inside a subtree", c("protected()")],
        ["Subtree of PID 1 or 2", "Forbidden: it would be the whole system. The box cannot be ticked", c("subtree_allowed()")],
        ["Subtree: only still-valid nodes", "The set is rebuilt at send time: a node is included if its start time is the remembered one; or, for a process that was not "
         "in the remembered tree (a new child), if its parent is included and it started no earlier than the parent. A node whose pid was reused is dropped together with all its descendants",
         c("verified_subtree()")],
        ["The root first", "If the root has changed, nothing at all is sent: “process N has exited, no signal sent”", c("send_dialog()")],
        ["Second confirmation", "For a subtree of more than 50 processes (" + c("BIG_SUBTREE") + "), or containing processes of other users: " + key("Enter") + " must be pressed twice. "
         "Changing the signal or the box cancels the first press", c("needs_confirm()")],
        ["Visible warnings", "Other users' processes, protected pids and the size of the subtree are listed in the dialog", c("warnings()") + ", " + c("lines()")],
    ], "«TAB» — The rules of the signal dialog") + \
    p("The result appears for five seconds in the bottom line instead of the key bar: “SIGTERM sent to 1234 firefox”, “SIGTERM sent to 7 processes (subtree of …)”, "
      "“SIGKILL to 1234 x: permission denied (EPERM)”, or “…: 5 sent, 2 errors (first: …)”. If some targets have vanished the line adds “(N no longer exist)”.") + \
    warn("The dialog is open for as long as the user takes. A process list that changes under it does not matter: the pairs (pid, start time) were "
         "fixed when it opened. A process that ended and a new one with the same pid will make the dialog send nothing.", "Time passes.")

S3 = p("Keys " + key("F7") + " and " + key("F8") + " lower and raise the nice value of the selected process by one.", lead=True) + \
    table(["Rule", "Detail"], [
        ["Range", "−20 to 19. At the limit: “nice of PID NAME already at the limit (N)”"],
        ["Which thread", "Only the main thread, as htop does: " + c("setpriority") + " on a pid acts on the thread whose tid equals it. The message says “(main thread)”"],
        ["Lowering needs root", "Raising the value (a lower priority) is allowed to the owner. Lowering it fails with EACCES or EPERM for an ordinary user; "
         "the message says “root is needed to lower the nice value”"],
        ["Identification", "Unlike signals, nice acts on the pid selected in the list with no start-time check"],
    ], "«TAB» — Nice")

S4 = p("Starting and stopping a service is another action that cannot be undone by ngtop, so it has its own care.", lead=True) + \
    table(["Care", "How"], [
        ["Always ask", "Every action opens a dialog with the unit, the scope (system or user), and “If needed, polkit will ask for the password.”. The default button is " + ui("Cancel") + "; "
         + key("Left") + " / " + key("Right") + " moves to " + ui("Confirm") + "; " + key("Esc") + " or " + key("n") + " cancels"],
        ["Use the system's authority", "ngtop never asks for a password and never runs sudo. The D-Bus call carries the flag " + c("AllowInteractiveAuth") + ", so polkit may open "
         "its own agent. Without an agent, the answer is a refusal and the line says: “Permission denied: a polkit agent is needed, or run ngtop with sudo”"],
        ["Time limit", "The action's connection waits up to 120 seconds, as long as a person may need to type a password. The status line says “waiting for authorization…” meanwhile"],
        ["One at a time", "While an action is outstanding, the keys " + key("s") + " " + key("t") + " " + key("r") + " " + key("e") + " do nothing, and a second request that reaches the thread is "
         "refused: “another action is in progress”"],
        ["Not blocking", "The action runs in its own thread, so the screen stays alive while polkit waits"],
        ["Read errors", "Texts like " + c("AccessDenied") + ", " + c("InteractiveAuthorizationRequired") + " or “not authorized” become “denied”; everything else is shown as received (" + c("classify_error()") + ")"],
        ["Fresh state", "After enable or disable, the cache of unit file states is aged so the next list shows the change"],
    ], "«TAB» — How service actions are protected") + \
    p("What ngtop does <i>not</i> do: it never edits unit files, never masks, never runs a service command through a shell. Its two external programs in this section, "
      + c("journalctl") + " and " + c("systemd-analyze") + ", are read-only and get a fixed list of arguments, with the unit name as a separate argument and never through a shell.")

S5 = p("A discrete GPU in D3cold or runtime-suspended costs almost nothing; reading its files or running " + c("nvidia-smi") + " on it wakes it, which can take a second and a few watts. A monitor "
       "that does this every second keeps the card on forever. The rule <b>“Never wake a sleeping GPU”</b> is therefore strict.", lead=True) + \
    table(["Rule", "Detail"], [
        ["Metrics are not read", "For a discrete card whose " + c("power/runtime_status") + " is “suspended” or “suspending”, or whose " + c("power_state") + " is “D3cold” (" + c("is_suspended()") + "), no usage, memory, clock, sensor or "
         "link file is opened. The panel shows “suspended” or “D3cold”. Tests: " + c("gpu_sospesa_non_viene_interrogata") + ", " + c("nvidia_suspending_conta_come_sospesa")],
        ["Integrated GPUs are read anyway", "Intel on PCI bus 0, and an AMD APU once recognised: they live in the processor's package, there is nothing to save. Their state is " + ui("Idle")
         + ". Test: " + c("integrata_con_runtime_pm_si_legge_comunque")],
        ["Unknown AMD cards are treated as discrete", "The first time, before the memory size says it is an APU, the stricter rule applies"],
        ["Clients are still counted (DRM)", "For AMD and Intel, the processes are scanned for DRM descriptors and their " + c("fdinfo") + " files as usual: per-process counters do not come from the device's sysfs folder (for NVIDIA cards " + c("fdinfo") + " is skipped)"],
        ["NVIDIA: awake cards only, with -i", "Only awake NVIDIA cards are passed to " + c("nvidia-smi") + ", via " + c("-i <bus id>") + "; when no card is awake, the " + c("ngtop-nvidia") + " thread launches nothing. Tests: " + c("nvidia_sospesa_non_lancia_nvidia_smi") + ", " + c("nvidia_due_schede_una_sospesa_interroga_solo_la_sveglia") + ", " + c("nvidia_scheda_che_si_sospende_non_viene_piu_interrogata")],
        ["NVIDIA: the sh guard before every launch", "Because a card can go to sleep between the last refresh and the moment the worker runs " + c("nvidia-smi") + " (especially over " + c("ssh") + ", where the collector's check uses the snapshot of the last refresh), every launch (" + c("--query-gpu") + ", " + c("pmon") + ", " + c("--query-compute-apps") + ") is wrapped by " + c("guarded()") + " in a small " + c("sh") + " guard (" + c("GUARD") + " in " + c("src/collect/nvidia_worker.rs") + "). On the observed machine (local or remote) and in the same shell that then " + c("exec") + "s " + c("nvidia-smi") + ", the guard re-reads " + c("/sys/class/drm/cardN/device/power/runtime_status") + " for each target card right before starting " + c("nvidia-smi") + ": if any card is " + c("suspended") + " or " + c("suspending") + ", or its status cannot be read, the shell exits (code 97, " + c("GUARD_ASLEEP") + ") without launching anything. When the guard finds a card asleep (" + c("Run::Asleep") + "), it is not treated as an error and the worker waits a full 2 s round (" + c("POLL") + "). Tests: " + c("guardia_lancia_solo_con_schede_sveglie") + ", " + c("nvidia_guardia_addormentata_non_e_un_errore_e_non_fa_raffiche")],
        ["Known limit of the guard", "The " + c("sh") + " guard checks " + c("power/runtime_status") + " (" + c("suspended") + " / " + c("suspending") + "), not " + c("power_state=D3cold") + "; " + c("D3cold") + " is covered by the pre-check " + c("is_suspended()") + " on the data of the last refresh"],
    ], "«TAB» — Not waking the GPU")

S6 = p("A terminal program that leaves the terminal in raw mode on the alternate screen, with the cursor hidden and the mouse on, ruins the user's shell. "
       "ngtop gives the terminal back in every way it can end.", lead=True) + \
    table(["How it ends", "What happens"], [
        ["Normal quit (" + key("q") + ", " + key("F10") + ", " + key("Ctrl-C") + ")", "The loop ends and the guard is dropped"],
        ["Error in the loop (" + c("?") + ")", "The function returns; the guard is dropped on the way out"],
        ["Panic", "A hook installed first calls " + c("restore_terminal()") + " <i>before</i> the panic message is printed, so the message is readable and not swallowed by the alternate screen"],
        ["SIGTERM, SIGINT, SIGHUP", c("signal_hook") + " sets a flag; the loop looks at it at least every 250 ms and ends in the normal way. A <b>second</b> signal while the flag is set "
         "ends the process at once with status 1, in case the loop is stuck"],
        ["Closed terminal window", "SIGHUP: the same as above"],
        ["Not a terminal", "If standard input or output is not a terminal, ngtop refuses to start with a clear message (exit 2) and suggests " + c("--dump"), ],
    ], "«TAB» — Giving the terminal back") + \
    p(c("TerminalGuard") + " is a value whose " + c("Drop") + " calls " + c("restore_terminal()") + ": it leaves the alternate screen, turns off the mouse, "
      "leaves raw mode and shows the cursor. Every step ignores errors and the function can be called several times, so the guard, the panic hook and "
      "the signal path can all run it. Everything that can fail is done <i>before</i> the terminal is touched: the first sample, the time zone, the configuration "
      "file.") + \
    note("The script " + c("scripts/test-distro.sh") + " runs ngtop under a pseudo-terminal for three seconds, sends it SIGTERM and checks that the output contains the "
         "sequences that leave the alternate screen (" + c("ESC[?1049l") + ") and show the cursor (" + c("ESC[?25h") + "), that nothing says “panicked”, and that the exit "
         "status is 0, 124 or 143 (" + rif("Testing") + ").", "Tested on eight distributions.") + \
    p("Output to a closed pipe (for example " + c("ngtop --dump | head -1") + ") is also safe: " + c("write_stdout()") + " ignores write errors, so there is no panic "
      "for a broken pipe.")

S7 = p("To show the clock, ngtop must know the local time zone offset. The usual way is a C library function, which a static musl binary has in a poorer form, "
       "so " + c("src/ui/tz.rs") + " does it in Rust. The data are untrusted — a file anyone may have written — so the parser is cautious.", lead=True) + \
    table(["Step", "Rule"], [
        ["Which zone", "The " + c("TZ") + " variable (a leading “:” is removed). “UTC”, “UTC0” and “GMT” are UTC. An absolute path is read as a TZif file. A name is looked up in "
         + sysf("/usr/share/zoneinfo") + ", " + sysf("/usr/lib/zoneinfo") + " and " + sysf("/usr/share/lib/zoneinfo") + ", but never if it contains “..”. Otherwise it is read as a POSIX rule "
         "such as " + c("CET-1CEST,M3.5.0,M10.5.0/3") + ". Without " + c("TZ") + ": " + sysf("/etc/localtime")],
        ["Failure", "Any error leads to UTC. Never a panic"],
        ["TZif header", "The first four bytes must be “TZif”. The six counts are read (as 32-bit numbers)"],
        ["Size check before any allocation", "The expected size of the data is computed from the counts with checked arithmetic (" + c("checked_mul") + ", " + c("checked_add") + ") "
         "and must fit in the file; otherwise the file is rejected. A hostile file cannot make ngtop reserve gigabytes"],
        ["Version 2 and 3", "The second block, with 64-bit times, is used when present, followed by the footer: a POSIX rule that says what to do after the last listed transition"],
        ["Finding the offset", "Binary search (" + c("partition_point") + ") for the last transition not after the instant. Past the end the footer rule decides, which is how a zone keeps "
         "working in the years beyond the table"],
        ["POSIX rule", "Standard name, offset, optional daylight name, offset and the two switching rules " + c("Mm.w.d/time") + " (month, week with 5 = last, weekday, time). "
         "The day is computed from the calendar (Hinnant's algorithm for days from civil date), including leap years"],
    ], "«TAB» — How the time zone is read") + \
    p("Tests: " + c("nessun_panic_su_prefissi_e_file_troncati") + " feeds the parser every prefix of a POSIX rule, and of a real zone file; " + c("tzif_inventato_e_rotto") + " builds files with "
      "impossible counts; " + c("posix_semplici_e_non_validi") + " tries malformed rules.")

CHAPTER = ("Safety", [
    ("Signals: pidfd and start time", S1),
    ("The signal dialog", S2),
    ("Nice", S3),
    ("Service actions", S4),
    ("GPUs that must not be woken", S5),
    ("Terminal restore", S6),
    ("Time zone without trust", S7),
])
