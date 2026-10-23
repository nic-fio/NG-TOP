# Copyright (c) 2026 Nicola Fiorillo
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
"""User Manual, chapter 9 — command-line options, the configuration file, the W key.
Sources: src/config.rs (Config, parse_full, to_toml, update_text, save, load, parse_args, merge, USAGE),
src/ui/app.rs (save_config), src/main.rs (interactive: exit codes)."""
from build import c, code, key, note, options_table, p, rif, table, term, tip, ui, ul, warn

S_CLI = p("You can start ngtop with options that change how it starts. They are all optional.", lead=True) + \
    options_table() + \
    p("An option with a value can be written with a space or with <code>=</code>: " + c("--delay 500") + " and " + c("--delay=500") +
      " are the same. The short forms " + c("-d") + " and " + c("-s") + " take the value after a space.") + \
    p("Examples:") + \
    term("""$ ngtop -d 500 -s 3        # refresh every 500 ms, start on Processes
$ ngtop --no-mouse        # leave the mouse to the terminal
$ ngtop --256             # force 256 colours
$ ngtop --config myconfig
$ ngtop --dump            # print a text summary and exit""") + \
    ul(["The interval of " + c("--delay") + " is between 250 and 5000 ms. A value in between is rounded to the nearest of the five "
        "steps (250, 500, 1000, 2000, 5000).",
        "A wrong option or value prints a message and <code>Try 'ngtop --help'.</code>, and ngtop exits with code 2 without opening the screen.",
        c("--help") + " and " + c("--version") + " print and exit; they work without a terminal."])

S_FILE = p("ngtop can remember your preferences in a small text file. You do not need one: without it, ngtop uses the defaults.", lead=True) + \
    p("The file is " + c("config.toml") + " in the folder <code>ngtop</code> of your configuration folder:") + \
    table(["If", "The file is"], [
        [c("XDG_CONFIG_HOME") + " is set", c("${XDG_CONFIG_HOME}/ngtop/config.toml")],
        ["otherwise", c("~/.config/ngtop/config.toml")],
    ], "«TAB» — Where the file is") + \
    p("To use another file, start with " + c("--config FILE") + ". That file <b>must exist</b>; the default one may be missing.") + \
    p("The format is simple: one setting on each line, written <code>name = value</code>. Text values go in double quotes, "
      "numbers are plain, and yes/no values are <code>true</code> or <code>false</code>. Everything after a <code>#</code> is a "
      "comment. Empty lines are ignored.") + \
    code("""# ngtop configuration
# refresh interval in ms (250–5000)
interval = 1000
# start section: 1 Overview, 2 GPU, 3 Processes, 4 Network & Disks, 5 Services
section = 1
# mouse enabled
mouse = true
# colors: "auto", "truecolor" or "256"
colors = "auto"
# temperature unit: "C" or "F"
temperature = "C"
# width of the left column of the Overview, in % (20–70)
left_width = 40
# remote machine to monitor over ssh (like the TARGET argument)
# host = "user@server"
""", "toml", "config.toml — the file ngtop writes when it creates it")

S_KEYS = p("These are all the settings. A missing line means the default.", lead=True) + \
    table(["Setting", "Default", "Allowed", "What it does"], [
        [c("interval"), c("1000"), "250–5000", "Refresh interval in milliseconds. A value in between is rounded to one of 250, 500, 1000, 2000, 5000. Same as " + c("--delay") + "."],
        [c("section"), c("1"), "1–5", "The section shown at start: 1 Overview, 2 GPU, 3 Processes, 4 Network &amp; Disks, 5 Services. Same as " + c("--section") + "."],
        [c("mouse"), c("true"), c("true") + " or " + c("false"), "Use the mouse. With " + c("false") + " the terminal keeps the mouse. " + c("--no-mouse") + " turns it off for one run."],
        [c("colors"), c('"auto"'), c('"auto"') + ", " + c('"truecolor"') + ", " + c('"256"'), "How many colours to use. " + c('"auto"') + " decides from the terminal (" + rif("Colours") + "). " + c("--256") + " forces 256."],
        [c("temperature"), c('"C"'), c('"C"') + " or " + c('"F"'), "Degrees Celsius or Fahrenheit, for the CPU and the GPU. Only in the file."],
        [c("left_width"), c("40"), "20–70", "Width of the left column of the Overview, in percent. Only in the file (" + rif("Changing the width of the left column") + ")."],
        [c("host"), "not set", "a quoted target", "A remote machine to monitor over ssh, written as in " + rif("Monitoring another machine") + ": " + c('host = "alex@server"') + ". Same as giving the target on the command line. " + c("--local") + " ignores it for one run."],
    ], "«TAB» — The settings of the configuration file") + \
    note("Settings that are not here are ignored with a warning (see " + rif("Mistakes in the file") + ").")

S_PRIORITY = p("Three places can say the same thing. ngtop uses them in this order, from the strongest:", lead=True) + \
    ul(["the <b>command line</b> (" + c("--delay") + ", " + c("--section") + ", " + c("--no-mouse") + ", " + c("--256") + ", " + c("--host") + " or a target, " + c("--local") + "),",
        "the <b>configuration file</b>,",
        "the <b>defaults</b> of the program."]) + \
    p("So " + c("ngtop -d 250") + " beats a file with " + c("interval = 2000") + ". The interval can also be changed while ngtop runs, with " +
      key("+") + " and " + key("-") + ".")

S_SAVE = p("You do not have to edit the file. Arrange ngtop as you like (the interval with " + key("+") + " and " + key("-") + ", the section you "
           "want to start on) and press " + key("W") + " (capital). ngtop writes the <b>current interval</b> and the <b>current section</b> "
           "to the file and says so on the status line:", lead=True) + \
    term("""configuration saved to /home/alice/.config/ngtop/config.toml""", "Status line") + \
    ul(["If the file does not exist, ngtop creates it, with the folder if needed, with all the settings and the comments shown above.",
        "If it exists, ngtop changes <b>only the two lines</b> <code>interval</code> and <code>section</code>. Your comments, the order and "
        "the other settings are left as they are. If a line was missing, it is added at the end.",
        "The write is safe: ngtop writes a temporary file and then renames it, so a failure never leaves a half-written configuration.",
        "The other settings (mouse, colours, temperature, width) are never changed by " + key("W") + ": edit them by hand."]) + \
    p("If something goes wrong the status line says why, in red: " + ui("configuration not saved (PATH): …") + ", or " +
      ui("no configuration path (HOME is not set)") + " when ngtop has no folder to use.")

S_ERRORS = p("ngtop reads the file at start-up. If it finds a mistake it does not guess: it prints a message with the line number and "
             "exits with code 2, before changing the screen.", lead=True) + \
    term("""$ ngtop
ngtop: configuration /home/alice/.config/ngtop/config.toml: line 3: 'interval' must be between 250 and 5000 (found 10)""") + \
    table(["Mistake", "Message (the end of it)"], [
        ["A number out of range", c("'section' must be between 1 and 5 (found 9)")],
        ["A text value without quotes", c("invalid value: blu")],
        ["A quoted text where a number is expected", c("'interval' must be a number")],
        ["A host that is not text, or is not a valid target", c("'host' must be a quoted string") + ", " + c("invalid TARGET 'x': …")],
        ["A number where text or yes/no is expected", c("'colors' must be a quoted string") + ", " + c("'mouse' must be true or false")],
        ["A wrong text value", c("'colors' must be \"auto\", \"truecolor\" or \"256\"")],
        ["A line without equals sign", c("missing '=' (expected key = value)")],
        ["A section in brackets", c("tables [..] are not supported")],
        ["A file that is not UTF-8, or larger than 64 KiB", c("the file is not valid UTF-8") + " / " + c("file too large")],
    ], "«TAB» — Common mistakes") + \
    p("A setting ngtop does not know is <b>not</b> an error. It is reported and ignored, and ngtop starts normally:") + \
    term("""ngtop: configuration /home/alice/.config/ngtop/config.toml: warning, line 7: unknown key 'colour', ignored""") + \
    p("Exit codes of ngtop: <b>0</b> normal; <b>2</b> a wrong option, a wrong configuration file, or no interactive terminal; <b>1</b> a "
      "terminal error while running.")

CHAPTER = ("Configuration", [
    ("Command-line options", S_CLI),
    ("The configuration file", S_FILE),
    ("Every setting", S_KEYS),
    ("Which setting wins", S_PRIORITY),
    ("Saving with W", S_SAVE),
    ("Mistakes in the file", S_ERRORS),
])
