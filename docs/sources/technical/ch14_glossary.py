from build import c, dl, p, rif, sysf


def v(*titles):
    return " See " + ", ".join(rif(t) for t in titles) + "."


NAMES = [
    ("Collector", "A function or module that reads kernel files and computes numbers; it knows nothing about the screen (" + c("src/collect/") + ")." + v("Collect and draw")),
    ("Roots", "The three folders (" + c("proc") + ", " + c("sys") + ", " + c("etc") + ") every collector reads from. Real on the machine, fake in tests." + v("How the collectors read")),
    ("System", "The structure that holds the latest data of the whole machine and the previous samples for rates (" + c("src/collect/mod.rs") + ")." + v("One refresh, step by step")),
    ("App", "The state of the interface: histories, selection, filter, dialog, state of each section (" + c("src/ui/app.rs") + ")." + v("State and drawing")),
    ("Section", "One of the five main screens, selected with " + c("1") + " to " + c("5") + ": Overview, GPU, Processes, Network &amp; Disks, Services. In the code, the modules and variants have Italian names: "
     + c("panoramica") + " / " + c("Panoramica") + " is the Overview, " + c("processi") + " / " + c("Processi") + " is Processes, " + c("retedischi") + " / " + c("ReteDischi") + " is Network &amp; Disks, "
     + c("servizi") + " / " + c("Servizi") + " is Services; " + c("gpu") + " is the same in both languages." + v("The sections")),
    ("Large layout, compact layout", "The two forms of the Overview: compact under 100 columns or 30 rows." + v("Layout")),
    ("Tick", "One cycle of the event loop that refreshes the data and redraws; its length is the refresh interval (250 ms to 5 s)." + v("The event loop")),
    ("dt", "The real time, in seconds, between two refreshes. A rate is a difference of counters divided by " + c("dt") + "; it is zero for the first sample." + v("One refresh, step by step")),
    ("Frame, buffer", "One whole screen as a grid of cells (ratatui " + c("Buffer") + "), drawn from scratch each time and sent to the terminal as the cells that changed." + v("ratatui and the frame")),
    ("Widget", "A small struct that draws one kind of element into a buffer (" + c("Panel") + ", " + c("Meter") + "…)." + v("The widgets")),
    ("Panel", "A bordered rectangle with a title on its border; the unit of every section." + v("The widgets")),
    ("Braille cell", "One character U+2800 to U+28FF: a 2 × 4 grid of dots, used to draw graphs at eight times the cell resolution." + v("The braille graph")),
    ("Gradient", "A list of colour stops between which the program interpolates; every bar, graph and percentage takes its colour from one." + v("Theme and gradients")),
    ("Truecolor, 256, 16", "The three colour depths of a terminal; ngtop draws in truecolor and converts the frame at the end." + v("Colour depth")),
    ("Alternate screen", "A second terminal screen that a full-screen program draws on and then leaves, so the shell's text comes back intact." + v("Terminal restore")),
    ("Raw mode", "Terminal mode in which keys reach the program without being edited or echoed." + v("Terminal restore")),
    ("Fixture", "A fake " + c("/proc") + ", " + c("/sys") + ", " + c("/etc") + " tree in " + c("tests/fixtures/") + " used by tests and by documentation screenshots." + v("Fixtures and two samples")),
    ("Snapshot", "One frame drawn on a test backend and printed as text or ANSI: " + c("ngtop --snapshot WxH") + "." + v("Seeing the screen: snapshot, dump, sweep")),
    ("Sweep", "A test that draws every section at many sizes, including absurd ones, to prove there is no panic." + v("Seeing the screen: snapshot, dump, sweep")),
    ("Backend (services)", "The trait through which the Services section talks to systemd. The real one uses D-Bus; the fake one gives demo data." + v("Threads", "Services, sessions, boot")),
    ("Worker thread", "The thread that executes the slow requests of the Services section while the interface keeps running." + v("Threads")),
]

KERNEL = [
    ("amdgpu, i915, xe", "The Linux kernel drivers of AMD, Intel (older) and Intel (newer) graphics. They publish counters in sysfs and in " + c("fdinfo") + "." + v("GPU: devices")),
    ("cgroup", "The kernel's grouping of processes with its accounting. systemd puts each service in one; ngtop reads memory and CPU of a service from its cgroup files." + v("Services, sessions, boot")),
    ("Clock tick, " + c("hz"), "The unit of CPU times in " + c("/proc") + ": usually 100 per second." + v("CPU, temperature", "Processes")),
    ("D3cold", "The deepest power state of a PCI device. A GPU in D3cold is not read." + v("GPUs that must not be woken")),
    ("DRM client", "One user of a GPU, with its own counters, identified on a card by its client id. A process can show it, and so can its children." + v("GPU: processes and the fdinfo files")),
    ("D-Bus, polkit", "The message bus of Linux and the service that decides if an action may be done, asking for a password if necessary." + v("Service actions")),
    ("Engine", "A part of a GPU that works on one kind of task: graphics, compute, copy, video decode or encode." + v("GPU: processes and the fdinfo files")),
    ("fdinfo", "The file " + c("/proc/[pid]/fdinfo/N") + ", with the counters of a DRM client for one open GPU descriptor." + v("GPU: processes and the fdinfo files")),
    ("hwmon, thermal zone", "The kernel's sensor interfaces under " + c("/sys/class") + ". ngtop reads temperatures, power and fans from them." + v("CPU, temperature")),
    ("Inode (socket)", "The number in " + c("/proc/net/tcp") + " that identifies a socket; the same number appears in " + c("socket:[N]") + " in a process's file descriptors." + v("Connections: inode to pid")),
    ("IOPS, latency, queue", "Operations per second, average time per operation, and requests in flight, of a disk." + v("Disks and filesystems")),
    ("musl", "A small C library that can be linked into an executable completely; with it ngtop is one static file." + v("musl and build.sh")),
    ("NSS", "The name service switch of the C library, which lets programs ask directories (LDAP, sssd) for user names. A static musl program has none." + v("What static takes away")),
    ("NVML", "NVIDIA's management library; shared and closed. The reason NVIDIA cards are only detected." + v("Why not NVML")),
    ("pidfd", "A file descriptor that refers to one process for ever, immune to reuse of the pid number." + v("Signals: pidfd and start time")),
    ("Runtime PM", "Power management that suspends a device when idle. A discrete GPU suspended this way is not woken." + v("GPUs that must not be woken")),
    ("Start time (" + c("starttime") + ")", "The moment a process started, in clock ticks since boot; with the pid, the identity of a process." + v("Signals: pidfd and start time")),
    ("statvfs", "The call that gives the size and free space of a filesystem; on a stuck network mount it can block, so it runs in a thread." + v("Disks and filesystems")),
    ("TZif", "The binary format of time zone files in " + c("/usr/share/zoneinfo") + ", read by ngtop's own parser." + v("Time zone without trust")),
    ("zswap, ZRAM", "Two ways of compressing memory instead of swapping it out to disk; the Overview shows the saving." + v("Memory, swap, load")),
]

CHAPTER = ("Glossary", [
    ("The project's own words", p("Words ngtop uses as terms, in the code and in this manual, with the section that explains them. Several source files keep their Italian names; "
                                   "the first terms tell which is which.", lead=True) + dl(NAMES, "gloss")),
    ("Linux and system terms", p("Terms of the system that ngtop reads, and what they mean for it.", lead=True) + dl(KERNEL, "gloss")),
])
