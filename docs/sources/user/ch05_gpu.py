# Copyright (c) 2026 Nicola Fiorillo
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
"""User Manual, chapter 5 — the GPU section.
Sources: src/ui/sections/gpu.rs (draw, draw_device, cells, state_line, draw_history, draw_engines, draw_power),
src/collect/gpu.rs (GpuState, drivers, fdinfo, suspended handling), src/ui/app.rs (cycle_gpu, gpu_only)."""
from build import c, key, keys_table, note, p, rif, snap, table, tip, ui, ul, warn

S_WHAT = p("The GPU section is ngtop's answer to <code>nvtop</code>. It shows every graphics card of the machine, how busy "
           "each one is, and <b>which processes use it</b>.", lead=True) + \
    snap("gpu", "«FIG» — The GPU section on a workstation with a discrete card and an integrated one", "150x42") + \
    p("From top to bottom the screen has the panels of the cards, the history of the selected card with its engines and its "
      "power, and the table of the processes that use the GPU.")

S_SUPPORT = p("ngtop reads the graphics cards from the kernel, not from a vendor program. What it can show depends on the driver.", lead=True) + \
    table(["Vendor", "Driver", "Support"], [
        ["AMD", c("amdgpu"), "Full: use, memory, clocks, temperature, power, fan, engines, per-process use."],
        ["Intel", c("i915") + ", " + c("xe"), "Full for what the driver reports: use by engine, clocks, memory, per-process use. "
         "Power and some temperatures are often not available."],
        ["NVIDIA", "any", "<b>Detected only.</b> The card is listed with its name, and the panel says " +
         ui("support coming soon") + ". Full support is planned."],
    ], "«TAB» — Graphics cards and drivers") + \
    note("NVIDIA's own tools read the card through a library of the vendor. A single static file like ngtop cannot load it "
         "cleanly, so NVIDIA needs a different way, which is planned. If you use an NVIDIA card with the proprietary "
         "driver, ngtop shows the card but no numbers for it.") + \
    p("If ngtop finds no card at all, the section says " + ui("No GPU detected") + ".")

S_CARD = p("Each card has a panel. The title says <code>GPU 0 · Name of the card</code>; on the right it gives the kind, the "
           "memory size, the driver and the PCI address, for example <code>integrated · i915 · 0000:00:02.0</code>.", lead=True) + \
    p("Inside, ngtop shows the values the card reports, two to a line. Only what exists is shown.") + \
    table(["Item", "Meaning"], [
        [ui("GPU"), "How busy the card is, 0–100%. For Intel it is the busiest engine."],
        [ui("MEM"), "Graphics memory used over the total. For an integrated card, which has no memory of its own, it is the "
         "amount of ordinary memory the card is using."],
        [ui("ENC") + ", " + ui("DEC") + ", " + ui("VID"), "Use of the video encoder, decoder, and the video engine (for Intel)."],
        [ui("Temp"), "Temperatures reported by the card, with their names."],
        [ui("Power"), "Watts used, and the limit set on the card if there is one (<code>120/203 W</code>)."],
        [ui("Clock"), "Current and maximum speed of the graphics core in MHz."],
        [ui("Mem clk"), "Speed of the graphics memory."],
        [ui("Fan"), "Fan speed in percent and in RPM."],
        [ui("PCIe"), "Speed and width of the link with the motherboard."],
    ], "«TAB» — What a card panel can show")

S_KINDS = p("ngtop distinguishes two kinds of card, because they behave differently.", lead=True) + \
    ul(["An <b>integrated</b> GPU is part of the processor (or sits on the motherboard) and shares the main memory. The title "
        "says <code>integrated</code>. ngtop always reads it.",
        "A <b>discrete</b> GPU is a separate card with its own memory. It can go to sleep to save power."]) + \
    p("When a laptop or a desktop has both, you see two panels, <code>GPU 0</code> and <code>GPU 1</code>. If they do not "
      "fit side by side they are stacked.")

S_SLEEP = p("A discrete card in power saving is <b>asleep</b>. Reading its files would wake it up, which costs power and "
            "defeats the purpose of letting it sleep. So ngtop <b>does not read a sleeping discrete GPU</b>. Its panel says:", lead=True) + \
    table(["Message", "Meaning"], [
        [ui("suspended — not queried, to avoid waking it"), "The card is in runtime suspend. No numbers are shown."],
        [ui("suspended (D3cold) — not queried, to avoid waking it"), "The card is completely powered off (state D3cold)."],
    ], "«TAB» — Messages of a sleeping card") + \
    p("As soon as some program wakes the card, ngtop shows its numbers again. In the compact layout the short word " +
      ui("suspended") + " is written instead.") + \
    p("Integrated cards are different: they switch on and off all the time while the machine works, so there is nothing "
      "to save, and ngtop reads them always. When the card is idle at that moment, the title shows " + ui("idle") + ".") + \
    tip("Do you see a card suspended while you expect it to work? That is the proof that ngtop is leaving it alone. Start "
        "a program that uses the card and look again.")

S_HIST = p("Under the cards, when the window is tall enough, there are three more panels about the <b>selected</b> card "
           "(the first one, or the one you chose with " + key("d") + ").", lead=True) + \
    ul([ui("GPU 0 history") + ": a graph of use, memory and temperature over time (<code>use · VRAM · temp</code>), with a scale "
        "0, 50 and 100%.",
        ui("engines · GPU 0") + ": the use of each engine of the card, as meters. The names are: " + ui("3D / gfx") + ", " +
        ui("Compute") + ", " + ui("Copy (DMA)") + ", " + ui("Video") + ", " + ui("Video enh.") + ", " + ui("Encode") + ", " +
        ui("Decode") + ". If the processes cannot be read the panel says " + ui("no data (clients not readable)") + ".",
        ui("power") + ": a graph of the watts. It says " + ui("not available") + " when the driver does not report power."]) + \
    p("The engines panel and the power panel appear only if the window is at least 100 columns wide.")

S_PROCS = p("The bottom of the section is the table " + ui("GPU processes") + ": only the processes that are using a graphics "
            "card. Its title shows the sort column, the number of processes and the sum of their graphics memory.", lead=True) + \
    table(["Column", "Meaning"], [
        [ui("PID") + ", " + ui("User"), "The process and its owner."],
        [ui("DEV"), "The number of the card the process uses (0, 1…)."],
        [ui("Type"), "What the process does with the card: " + ui("Graphics") + ", " + ui("Compute") + ", " + ui("Video") + " or " + ui("Copy") + "; " + ui("—") + " if unknown."],
        [ui("GPU%"), "Use of the card by the process, with a small bar."],
        [ui("ENC%") + ", " + ui("DEC%"), "Use of the encoder and the decoder."],
        [ui("VRAM"), "Graphics memory of the process; " + ui("—") + " when it is not reported."],
        [ui("CPU%") + ", " + ui("Mem"), "The same numbers as in the process list."],
        [ui("Name") + ", " + ui("Command"), "The program."],
    ], "«TAB» — The columns of the GPU table") + \
    p("The table works like the process list. " + key("↑") + " and " + key("↓") + " select, " + key("F6") + " changes the "
      "sort column (the list includes GPU and VRAM), " + key("/") + " filters, and " + key("F9") + " sends a signal to the "
      "selected process (" + rif("Sending a signal") + ").") + \
    warn("The GPU use of a process comes from files that the kernel keeps for each program using the card. You can read "
         "those of your own programs. For the programs of <b>other users</b> you need administrator rights, otherwise they "
         "do not appear, or appear with " + ui("—") + " (" + rif("What you see without administrator rights") + ").",
         "Other users' processes.")

S_DEVICE = p("With more than one card, press " + key("d") + " to select the next one. The marker " + ui("●") + " in the title "
             "shows the selected card. The history, engines and power panels follow the selection. So does the " +
             ui("gpu0") + " panel of the Overview.", lead=True) + \
    p("The process table lists the processes of all cards together; the " + ui("DEV") + " column tells which card each "
      "process uses.")

S_KEYSGPU = p("The keys of the GPU section, as the program reports them.", lead=True) + keys_table("GPU")

CHAPTER = ("GPU", [
    ("What the GPU section shows", S_WHAT),
    ("Supported cards", S_SUPPORT),
    ("The panel of a card", S_CARD),
    ("Integrated and discrete cards", S_KINDS),
    ("Sleeping cards", S_SLEEP),
    ("History, engines and power", S_HIST),
    ("Processes using the GPU", S_PROCS),
    ("Switching the card", S_DEVICE),
    ("Keys of the GPU section", S_KEYSGPU),
])
