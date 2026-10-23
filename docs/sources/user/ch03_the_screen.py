# Copyright (c) 2026 Nicola Fiorillo
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
"""User Manual, chapter 3 — the parts of the screen, keys and mouse, help, compact layout, colours, interval.
Sources: src/ui/view.rs (draw, draw_tabs, HELP, key bars), src/ui/app.rs (on_key, on_mouse, LayoutMode,
INTERVALS_MS), src/ui/widgets/ (panel, meter, braille, keybar, tabbar), src/ui/theme.rs (ColorMode)."""
from build import c, key, note, p, rif, snap, table, term, tip, ui, ul, warn

S_PARTS = p("Every screen of ngtop has the same three parts, from top to bottom.", lead=True) + \
    snap("overview", "«FIG» — The Overview screen: the tab bar at the top, the panels in the middle, the key bar at the bottom", "120x36") + \
    table(["Part", "What it shows"], [
        [ui("Tab bar"), "The program name and version, the five sections, and on the right some facts about the machine."],
        ["Panels", "The body of the screen. What is in it depends on the section; every section is a set of framed panels."],
        [ui("Key bar"), "The last line: the keys that matter in the section you are in, each with its label."],
    ], "«TAB» — The three parts of the screen")

S_TABS = p("The tab bar is the first line. On the left are the five sections; the one you are in is highlighted. Click a tab, "
           "or press " + key("1") + " to " + key("5") + ", or use " + key("Tab") + " and " + key("Shift", "Tab") +
           " to go to the next and the previous section.", lead=True) + \
    p("On the right, ngtop shows five facts, separated by <code>│</code>:") + \
    term("""myhost │ up 3d 06:09 │ load 0.77 1.16 1.04 │ every 1s │ 13:40:04""") + \
    table(["Fact", "Meaning"], [
        ["Host name", "The name of the machine."],
        [c("up 3d 06:09"), "How long the machine has been running: 3 days, 6 hours, 9 minutes."],
        [c("load 0.77 1.16 1.04"), "The load average over 1, 5 and 15 minutes: the mean number of processes that wanted a CPU."],
        [c("every 1s"), "The refresh interval (" + rif("The refresh interval") + ")."],
        ["The clock", "The local time of the machine, from its time zone."],
    ], "«TAB» — The right side of the tab bar") + \
    p("On a narrow terminal the right side is cut first; the tabs always stay.")

S_PANELS = p("Each panel is a frame with a <b>title on the top left</b> and, often, a short piece of information on the "
             "<b>top right</b>: the name of the CPU, the interface, how many processes there are. The lower border "
             "sometimes carries a note, such as the scale of a graph.", lead=True) + \
    p("Numbers are drawn in three ways.") + \
    table(["Drawing", "Looks like", "How to read it"], [
        ["Meters", c("■■■■■■········"), "A bar of squares: the filled part is the share used. The number follows, "
         "such as <code>14%</code> or <code>4.05G</code>."],
        ["Graphs", "tiny dots made of braille characters", "A history, newest on the right. Each character holds a small "
         "grid of dots, so the line is finer than the text."],
        ["Small bars", c("▁▂▄█"), "A very short history in one line, such as the one for each disk in the Overview."],
    ], "«TAB» — How ngtop draws numbers") + \
    p("Colours follow the value. The usual gradient goes from <b>green</b> (low) through <b>yellow</b> to <b>red</b> (high). "
      "CPU, memory, network and disks each have their own hue, so you can tell the panels apart at a glance.") + \
    note("Where ngtop cannot read a value it writes <b>—</b>. A dash is never a zero: it means \"not available here\" "
         "(" + rif("What you see without administrator rights") + ").")

S_KEYBAR = p("The last line of the screen is the <b>key bar</b>. It shows the main keys of the section you are in, each "
             "with its action. It changes when you change section. While you type a filter it shows " +
             ui("Enter") + " (" + ui("Confirm") + ") and " + ui("Esc") + " (" + ui("Cancel and clear") + ").", lead=True) + \
    p("When something happens, the result replaces the key bar for about five seconds: in <b>green</b> if it went well, "
      "in <b>red</b> if it failed. This is the <b>status line</b>. Examples:") + \
    term("""SIGTERM sent to 4121 firefox
nice of 4121 firefox (main thread): 0 → 1
configuration saved to /home/alice/.config/ngtop/config.toml
SIGTERM to 4121 firefox: permission denied (EPERM)""", "Status line") + \
    p("The Services section writes its results on the lower border of the list instead (" +
      rif("Starting and stopping") + ").") + \
    p("The key bar is a reminder, not the full list. The full list is in the help window (" + rif("The help window") +
      ") and in the chapter " + rif("Key reference") + ".")

S_KEYS = p("Everything in ngtop can be done with the keyboard. The mouse is a convenience.", lead=True) + \
    table(["Key", "Action"], [
        [key("1") + " … " + key("5") + ", " + key("Tab") + ", " + key("Shift", "Tab"), "Go to a section, to the next one, to the previous one."],
        [key("↑") + " " + key("↓") + " " + key("PgUp") + " " + key("PgDn") + " " + key("Home") + " " + key("End"), "Move the selection in the main table of the section."],
        [key("F1"), "Open the help window."],
        [key("+") + " " + key("-"), "Lengthen or shorten the refresh interval."],
        [key("W"), "Save the interval and the section in the configuration file."],
        [key("q") + " " + key("F10") + " " + key("Ctrl", "C"), "Quit."],
    ], "«TAB» — The keys that work in every section") + \
    p("Every section adds its own keys, listed in its chapter and, all together, in the chapter " + rif("Key reference") + ". "
      "Letters are case-sensitive: " + key("W") + " saves the configuration, " + key("w") + " does nothing.")

S_MOUSE = p("If your terminal reports the mouse, ngtop uses it.", lead=True) + \
    table(["Mouse", "Effect"], [
        ["Click a tab", "Go to that section."],
        ["Click a row", "Select it."],
        ["Click a column header", "Sort by that column. Click again to reverse the order. Only the columns that can be sorted react."],
        ["Mouse wheel", "Move the selection by three rows. Over the detail panel of a process, scroll the panel."],
        ["Click a tab of the detail panel", "Show that tab (" + ui("Processes") + " section)."],
        ["Click an interface", "Show that interface (" + ui("Network & Disks") + " section)."],
    ], "«TAB» — Mouse actions") + \
    p("To turn the mouse off, start with " + c("--no-mouse") + " or set " + c("mouse = false") + " in the configuration file. "
      "Then your terminal handles the mouse itself, and you can select and copy text on the screen as usual.") + \
    tip("In most terminals you can still select text while the mouse is on, by holding " + key("Shift") + " while you drag.")

S_HELP = p("Press " + key("F1") + " in any section. A window lists the keys that work everywhere and the keys of each "
           "section.", lead=True) + \
    snap("overview", "«FIG» — The help window, opened over the Overview", "120x36", overlay="help") + \
    p("Close it with " + key("Esc") + ", " + key("Enter") + ", " + key("F1") + " or " + key("q") + ". While the window is open "
      "the other keys do nothing, and the screen behind it keeps updating.")

S_COMPACT = p("ngtop adapts to the size of the terminal. With <b>100 columns or more and 30 rows or more</b> it uses the "
              "full layout. Below that, it switches by itself to the <b>compact layout</b>, and returns to the full one "
              "when you enlarge the window.", lead=True) + \
    snap("compact", "«FIG» — The compact layout (Overview) in a small terminal") + \
    p("In the compact layout the Overview becomes a column of single lines (CPU, memory, GPU, network, disks) above the "
      "process list. The key bar is shorter too: " + ui("Filter") + " is on " + key("/") + " and " + ui("Sig") + " stands for "
      "the signal. The other sections squeeze their panels, and hide the columns that do not fit.") + \
    note("Below 10 columns by 4 rows nothing can be drawn and the screen stays empty. Enlarge the window.")

S_COLOURS = p("ngtop uses the best colours your terminal can show and chooses by itself:", lead=True) + \
    table(["Mode", "When ngtop picks it"], [
        ["Truecolor (24-bit)", "The variable <code>COLORTERM</code> contains <code>truecolor</code> or <code>24bit</code>; or <code>TERM</code> contains <code>truecolor</code> or <code>direct</code>."],
        ["16 colours", "<code>TERM</code> is <code>linux</code>: the text console, outside a graphical session."],
        ["256 colours", "Everything else. This is the safe choice."],
    ], "«TAB» — How the colour mode is chosen") + \
    p("If the choice is wrong for your terminal, force it: " + c("--256") + " on the command line, or " +
      c('colors = "truecolor"') + " or " + c('colors = "256"') + " in the configuration file (" +
      rif("The configuration file") + "). " + rif("Colours look wrong") + " has the symptoms and the cures.")

S_INTERVAL = p("ngtop reads the system once for each refresh interval. The default is <b>one second</b>. The interval is "
               "always shown in the tab bar (" + ui("every 1s") + ").", lead=True) + \
    p("Press " + key("+") + " to make the interval longer and " + key("-") + " to make it shorter. It moves on five steps:") + \
    table(["Step", "Interval", "Good for"], [
        ["1", "250 ms", "Watching a fast change; uses more CPU."],
        ["2", "500 ms", "—"],
        ["3", "1 s", "The default."],
        ["4", "2 s", "A busy or slow machine."],
        ["5", "5 s", "A slow SSH link, or a machine that must not be disturbed."],
    ], "«TAB» — The refresh intervals") + \
    p("The change takes effect at once. It is lost when you quit, unless you press " + key("W") + ", which saves it with the "
      "current section. To start with another interval, use " + c("--delay") + " (" + rif("Command-line options") + ").")

CHAPTER = ("The screen", [
    ("The parts of the screen", S_PARTS),
    ("The tab bar", S_TABS),
    ("Panels, meters and graphs", S_PANELS),
    ("The key bar and the status line", S_KEYBAR),
    ("Keyboard", S_KEYS),
    ("Mouse", S_MOUSE),
    ("The help window", S_HELP),
    ("The compact layout", S_COMPACT),
    ("Colours", S_COLOURS),
    ("The refresh interval", S_INTERVAL),
])
