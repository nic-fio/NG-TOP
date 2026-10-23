# Copyright (c) 2026 Nicola Fiorillo
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
"""User Manual, chapter 4 — the Overview section.
Sources: src/ui/sections/panoramica.rs (draw_cpu, draw_mem, draw_gpu, draw_net, draw_disks, draw_processes),
src/ui/app.rs (keys, sorting, filter), src/config.rs (left_width), src/collect/mem.rs (used = total - available)."""
from build import c, key, keys_table, note, p, rif, snap, table, tip, ui, ul

S_GLANCE = p("The Overview is the first screen. It puts the whole machine on one page: how busy the CPU is, where the "
             "memory went, what the graphics card and the network are doing, how full the disks are, and which "
             "processes use the most.", lead=True) + \
    snap("overview", "«FIG» — The Overview of a workstation, on a large terminal", "182x52") + \
    p("The screen has two columns. The <b>left column</b> holds the small panels and the <b>right column</b> the process "
      "list. The CPU panel runs across the top. The left column takes 40% of the width by default; " +
      rif("Changing the width of the left column") + " tells how to change it.") + \
    table(["Panel", "What it shows"], [
        [ui("cpu"), "Total use and one meter for each core, with a history graph."],
        [ui("mem"), "Used, available, cache, free memory and swap."],
        [ui("gpu0"), "A short card of the graphics card: use, memory, clock."],
        [ui("net"), "Speed of one network interface, with a graph."],
        [ui("disks"), "Reading and writing speed, and the fullness of each mounted disk."],
        [ui("processes"), "The processes, as a list or a tree, with filter and sorting."],
    ], "«TAB» — The panels of the Overview")

S_CPU = p("The <b>" + "cpu" + "</b> panel has the name of the processor and the number of threads in its title.", lead=True) + \
    ul(["The first line is the <b>total</b>: " + ui("CPU") + ", a meter, the percentage, the temperature of the processor "
        "and the average clock speed (for example <code>14% 42°C 0.92GHz</code>).",
        "Under it come the <b>cores</b>: " + ui("C0") + ", " + ui("C1") + " and so on, each with its own meter and percentage. "
        "A hyper-threaded core counts as two threads, so a 12-core processor shows 24 meters.",
        "The big graph on the left is the history of the total use. Newest is on the right.",
        "If the panel is too low to show all the cores, the lower border says <code>+N cores not shown</code>. Make the "
        "terminal taller to see them."]) + \
    p("A value of 100% means every thread is fully busy. The temperature comes from the sensors of the kernel; if the "
      "machine has none that ngtop can use, it is simply not shown.") + \
    tip("Use " + c("temperature = \"F\"") + " in the configuration file to see degrees Fahrenheit (" +
        rif("The configuration file") + ").")

S_MEM = p("The <b>mem</b> panel has the total memory in its title, and one line for each kind.", lead=True) + \
    table(["Line", "Meaning"], [
        [ui("Used"), "The memory that programs are really using: the total minus what is <b>Available</b>."],
        [ui("Available"), "What programs could still obtain without swapping. It includes the cache that the system "
         "would give back if needed. This is the number that matters."],
        [ui("Cache"), "Memory that holds copies of files, to make the disk faster. It is not lost: it is released when needed."],
        [ui("Free"), "Memory that holds nothing at all. A small number here is normal on Linux."],
        [ui("Swap"), "Swap in use over the swap total. If it is large and growing, the machine is short of memory."],
    ], "«TAB» — The lines of the memory panel") + \
    p("Each line has a meter and a small history. Below the lines, when they exist, ngtop adds one line of notes: " +
      c("ZRAM") + " (compressed memory used as swap, with the compression ratio), " + c("zswap") + " (the compressed "
      "swap cache) and " + c("Dirty") + " (data waiting to be written to disk).")

S_GPUNET = p("Two small panels sit under the memory.", lead=True) + \
    ul(["<b>" + "gpu0" + "</b> is a card of the first graphics card: its name in the title, then " + ui("GPU") + " (use), " +
        ui("MEM") + " (graphics memory), " + ui("VID") + " (video engine) and the clock, as far as the card reports them. "
        "If you have more than one card, press " + key("d") + " to show the next one. All the details are in the " +
        "chapter " + rif("GPU") + ". If the card is asleep, the panel says so instead of waking it.",
        "<b>net</b> shows one network interface: the name and the IP address in the title, then the download (" + ui("▼") + ") "
        "and upload (" + ui("▲") + ") speeds, the highest speed seen since ngtop started (" + ui("peak") + ") and the "
        "total transferred (" + ui("total") + "). The graph has a scale, written on the lower border (<code>scale 128KB/s</code>), "
        "that adapts to the traffic. If there is more than one interface, the title shows <code>‹ ›</code> and " +
        key("n") + " and " + key("N") + " switch to the next and the previous one. ngtop starts with the interface "
        "that has the default route. An interface that is down is marked " + ui("(down)") + "."]) + \
    p("The chapter " + rif("Network & Disks") + " shows the same data in more depth, with the connections.")

S_DISKS = p("The <b>disks</b> panel shows, in its title, the total reading and writing speed of all physical disks "
            "(<code>R 0.0 W 0.0 MB/s</code>). Then there is one line for each mounted file system: where it is "
            "mounted, a meter of how full it is, the used and total sizes, and a small history of the disk activity.", lead=True) + \
    p("If there are more file systems than lines, the lower border says <code>+N more</code>. The "
      "chapter " + rif("Network & Disks") + " lists them all.")

S_PROC = p("The <b>processes</b> panel is a smaller version of the " + rif("Processes") + " section.", lead=True) + \
    ul(["The title shows the filter you typed, the column used for sorting (<code>‹ cpu ›</code>) and whether the view is a "
        "<code>list</code> or a <code>tree</code>.",
        "The lower border counts the processes and threads of the whole machine and the position of the selection "
        "(<code>247 processes · 822 threads · sel. 1/247</code>).",
        "The columns are " + ui("PID") + ", " + ui("Name") + ", " + ui("Command") + ", " + ui("User") + ", " + ui("Thr") +
        " (threads), " + ui("Mem") + " and " + ui("CPU%") + ". If the machine has a GPU in use, two columns " + ui("GPU%") +
        " and " + ui("VRAM") + " are added. A narrow panel hides the columns that do not fit.",
        "The percentage is a share of the <b>whole machine</b>: 100 means every core is busy. A process that fills one core of "
        "eight shows 12.5."]) + \
    p("Sorting starts on CPU, highest first. Press " + key("F6") + " (or " + key(">") + ") to move to the next column, " +
      key("<") + " to go back, " + key("i") + " to reverse the order, or click a column header. The columns in the cycle are " +
      "PID, name, user, memory, CPU, GPU and VRAM. Memory, CPU, GPU and VRAM start with the highest value on top; the "
      "others start in alphabetical or numerical order.") + \
    p("Press " + key("F5") + " to change between the flat list and the <b>tree</b>, where every process is shown under its "
      "parent. " + key("/") + " opens the filter box (the chapter " + rif("Processes") + " explains it). " + key("F9") +
      " opens the window to send a signal to the selected process. To see the details of a process, or to fold branches of "
      "the tree, go to the " + ui("Processes") + " section: " + key("3") + ".")

S_WIDTH = p("The left column takes 40% of the width. To change it, put a line in the configuration file:", lead=True) + \
    p("<code>left_width = 50</code>") + \
    p("The value is a percentage between 20 and 70. There is no key and no command-line option for it. See " +
      rif("The configuration file") + ".")

S_KEYSOV = p("These are the keys of the Overview, as the program reports them.", lead=True) + \
    keys_table("Overview") + \
    p("The keys that work everywhere (sections, help, interval, save, quit) are in " + rif("Key reference") + ".")

CHAPTER = ("Overview", [
    ("The Overview at a glance", S_GLANCE),
    ("CPU", S_CPU),
    ("Memory", S_MEM),
    ("GPU and network panels", S_GPUNET),
    ("Disks panel", S_DISKS),
    ("The process panel", S_PROC),
    ("Changing the width of the left column", S_WIDTH),
    ("Keys of the Overview", S_KEYSOV),
])
