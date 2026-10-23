# Copyright (c) 2026 Nicola Fiorillo
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
"""User Manual, chapter 7 — the Network & Disks section.
Sources: src/ui/sections/retedischi.rs (panels, ProtoFilter, ConnSort, on_key, filesystems), src/collect/conn.rs,
src/collect/disk.rs (REAL_FS, FS_TIMEOUT, is_physical), src/collect/net.rs, src/ui/app.rs (jump_to_pid)."""
from build import c, key, keys_table, note, p, rif, snap, steps, table, tip, ui, ul, warn

S_SCREEN = p("This section answers three questions: how much is the network carrying, <b>who</b> is talking, and how are the "
             "disks doing.", lead=True) + \
    snap("network", "«FIG» — The Network & Disks section", "150x42") + \
    table(["Panel", "What it shows"], [
        [ui("interface"), "The speed of the selected interface over time."],
        [ui("interfaces"), "All the network interfaces, with address, speeds and state."],
        [ui("connections"), "The TCP and UDP connections, with the process that owns each one."],
        [ui("sda") + " (the disk names)", "One panel for each physical disk: speed, operations, load."],
        [ui("filesystems"), "The mounted file systems and how full they are."],
    ], "«TAB» — The panels of the section") + \
    p("When the terminal is not tall enough for everything, ngtop keeps the network first, then the connections, the interfaces, "
      "the disks and the file systems.")

S_IFACE = p("The panel <b>interface</b> is named after the selected interface, for example <code>interface eth0</code>, with its "
            "addresses on the right. Inside you see the graphs of the two directions and these numbers:", lead=True) + \
    table(["Item", "Meaning"], [
        [ui("▼"), "Download speed now."],
        [ui("▲"), "Upload speed now."],
        [ui("peak"), "The highest speed seen since ngtop started, or since you pressed " + key("z") + "."],
        [ui("total"), "The amount of data carried: since the interface was created (usually at boot), or since you pressed " + key("z") + "."],
    ], "«TAB» — Numbers of the interface panel") + \
    p("The lower border shows the scale of the graph (<code>scale 128KB/s</code>), which follows the traffic, and the reminder " +
      ui("i: switch") + ". Press " + key("i") + " to go to the next interface, " + key("I") + " for the previous one, or click one in the "
      "list. ngtop starts with the interface of the default route.") + \
    p("The panel <b>interfaces</b> lists them all: " + ui("Name") + ", " + ui("IPv4 address") + ", the speeds " + ui("▼") + " and " + ui("▲") +
      ", and the " + ui("State") + " (" + ui("● up") + " or " + ui("○ down") + "). Below there is a line with the DNS server and the "
      "gateway: <code>DNS 9.9.9.9 · gateway 192.168.1.1</code>.") + \
    tip("In this section " + key("i") + " selects the interface. In the other sections " + key("i") + " reverses the sort order. "
        "That is why the key bar here says " + ui("i Interface") + ".")

S_CONN = p("The panel <b>connections</b> lists the TCP and UDP sockets of the machine. This is where you find out which program is "
           "talking to which address.", lead=True) + \
    table(["Column", "Meaning"], [
        [ui("Proto"), "TCP or UDP."],
        [ui("Local") + ", " + ui("Remote"), "The two ends, as address:port."],
        [ui("State"), c("ESTAB") + " (a connection in use), " + c("LISTEN") + " (a server waiting), or another TCP state such as " + c("TIME_WAIT") + "."],
        [ui("PID") + ", " + ui("Process"), "The process that owns the socket. <b>—</b> if ngtop cannot tell (" + rif("What you see without administrator rights") + ")."],
        [ui("Queue rx/tx"), "Bytes waiting in the receive and send queues. A queue that stays large means the program or the network is slow."],
    ], "«TAB» — Columns of the connection table") + \
    p("The title on the right shows the current filters and the count, for example <code>TCP+UDP · LISTEN yes · 170 · sort state</code>. "
      "These keys change what you see:") + \
    table(["Key", "Action"], [
        [key("c"), "Cycle through " + ui("TCP+UDP") + ", " + ui("TCP") + " and " + ui("UDP") + "."],
        [key("l"), "Show or hide the servers that only wait (" + c("LISTEN") + ")."],
        [key("F6"), "Change the sort: state, proto, local, remote, pid, process, queue."],
        [key("↑") + " " + key("↓") + " " + key("Home") + " " + key("End"), "Select a connection."],
    ], "«TAB» — Keys of the connection table") + \
    p("If nothing matches, the table says <code>no connections match these filters</code>. ngtop reads the connections only while "
      "this section is on the screen, so the other sections stay lighter.")

S_JUMP = p("You see a connection and want to know more about the program behind it? Select it and press " + key("Enter") +
      ". ngtop jumps to the " + ui("Processes") + " section with that process selected, opening the folded branches that hide it, and "
      "clearing the filter if it would hide it.", lead=True) + \
    ul(["If the connection has no process (" + ui("—") + "), nothing happens.",
        "If the process ended in the meantime, the status line says <code>process N has exited</code>.",
        key("F9") + " instead opens the signal window for that process without leaving the section. This is the quick way to "
        "stop a program that is flooding the network (" + rif("Sending a signal") + ")."])

S_DISKS = p("Under the connections there is one panel for each <b>physical</b> disk, named after the device (<code>sda</code>, "
            "<code>nvme0n1</code>). Partitions, loop devices, RAM disks and device-mapper volumes are not listed: they would "
            "only repeat what the real disk shows.", lead=True) + \
    table(["Item", "Meaning"], [
        [ui("R") + ", " + ui("W"), "Reading and writing speed in MB/s, with small graphs."],
        [ui("IOPS"), "Read and write operations per second (<code>IOPS 120/35</code>)."],
        [ui("Use"), "How busy the disk is, in percent of time. A disk near 100% is the bottleneck."],
        [ui("Lat"), "The mean time a request takes, in milliseconds. <b>—</b> when it cannot be measured, for example when the disk is idle."],
        [ui("Queue"), "Requests waiting at that moment."],
    ], "«TAB» — Numbers of a disk panel") + \
    p("With many disks, the panel shows a page at a time and the border says <code>1-3 of 8 · PgUp/PgDn</code>. Turn the page with " +
      key("PgUp") + " and " + key("PgDn") + ".")

S_FS = p("The panel <b>filesystems</b> lists what is mounted, one line each.", lead=True) + \
    table(["Column", "Meaning"], [
        [ui("Mounted on"), "The folder where the file system is attached."],
        [ui("Device"), "The disk, partition or network share behind it."],
        [ui("Type"), "ext4, btrfs, xfs, vfat, nfs4, cifs…"],
        [ui("Use"), "A meter and the percentage of space used."],
        [ui("Used") + ", " + ui("Free") + ", " + ui("Total"), "The sizes. <b>Free</b> is what a normal user can still write."],
        [ui("Inode"), "The percentage of file entries used. A disk can be full of small files with space still free."],
        [ui("Options"), "The mount options (<code>rw,relatime</code>)."],
    ], "«TAB» — Columns of the file systems table") + \
    p("Only <b>real</b> file systems are listed: the ones kept on a disk or on the network (ext2/3/4, btrfs, xfs, f2fs, vfat, exfat, zfs, "
      "ntfs, nfs, cifs and a few more). System ones such as <code>tmpfs</code> or <code>proc</code>, and the storage of containers "
      "and snap packages, are left out. A column that does not fit the width is dropped.")

S_STUCK = p("A network disk (NFS, CIFS) can stop answering: the server is off, the network is down. Programs that touch it freeze. "
            "A monitor must not freeze with them.", lead=True) + \
    p("ngtop asks each file system for its free space from a separate thread. If one does not answer within <b>5 seconds</b>, the "
      "line stays in the table with " + ui("not responding") + " in <b>red</b> in the " + ui("Use") + " column, and the rest of the "
      "screen goes on working. When the mount answers again, the numbers return.") + \
    warn("Do not try to open the stuck folder in a shell: that shell would hang. Check the server and the network first, then, if "
         "needed, unmount it with <code>umount -l</code>.", "A mount that does not respond.")

S_Z = p("Peaks and totals grow as time passes. To start counting from now, press " + key("z") + ": it resets the peaks and sets "
        "the totals to zero for all interfaces. Nothing else is changed. This is useful to measure one download: press " +
        key("z") + ", start the download, and read " + ui("total") + " at the end.", lead=True)

S_KEYSNET = p("The keys of the Network & Disks section, as the program reports them.", lead=True) + keys_table("Network & Disks")

CHAPTER = ("Network & Disks", [
    ("The Network & Disks screen", S_SCREEN),
    ("Interfaces", S_IFACE),
    ("Connections", S_CONN),
    ("From a connection to its process", S_JUMP),
    ("Disks", S_DISKS),
    ("File systems", S_FS),
    ("A mount that does not respond", S_STUCK),
    ("Resetting peaks and totals", S_Z),
    ("Keys of the Network & Disks section", S_KEYSNET),
])
