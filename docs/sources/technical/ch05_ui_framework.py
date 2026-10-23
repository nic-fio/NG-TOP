from build import arrow, box, c, fig, flow, note, p, rif, table, text, tip, warn, zone

# ── Figure: the braille cell ─────────────────────────────────────────────
def _dot(cx, cy, lit, label):
    fill = "#0050C0" if lit else "#e2e8f0"
    stroke = "#003a90" if lit else "#cbd5e1"
    return (f'<circle cx="{cx}" cy="{cy}" r="13" fill="{fill}" stroke="{stroke}" stroke-width="1.5"/>'
            + text(cx, cy + 4, label, 10.5, "#ffffff" if lit else "#64748b", "700", "middle", False))


def _cell(x, y, bits):
    """A braille cell: 2 columns x 4 rows; bits = set of lit bit values."""
    left = [0x01, 0x02, 0x04, 0x40]
    right = [0x08, 0x10, 0x20, 0x80]
    s = f'<rect x="{x}" y="{y}" width="92" height="160" rx="10" fill="#ffffff" stroke="#d5deea"/>'
    for r in range(4):
        s += _dot(x + 28, y + 28 + r * 34, left[r] in bits, f"{left[r]:02X}")
        s += _dot(x + 64, y + 28 + r * 34, right[r] in bits, f"{right[r]:02X}")
    return s


BRAILLE = fig(
    _cell(30, 40, set())
    + text(76, 222, "dots and bits", 11.5, "#334155")
    + _cell(190, 40, {0x40, 0x04, 0x80, 0x20})
    + text(236, 222, "left 0.5, right 0.5", 11.5, "#334155")
    + text(236, 238, "0x44 + 0xA0 = 0xE4: U+28E4", 11.5, "#334155")
    + _cell(350, 40, {0x40, 0x04, 0x02, 0x01, 0x80, 0x20})
    + text(396, 222, "left 1.0, right 0.5", 11.5, "#334155")
    + text(396, 238, "0x47 + 0xA0 = 0xE7: U+28E7", 11.5, "#334155")
    + box(480, 56, 380, 56, "character = U+2800 + bits", "one terminal cell, two columns of the graph", "navy")
    + box(480, 130, 380, 56, "level = round(value × rows × 4)", "dot k of row r is lit if level > 4r + k", "blue"),
    900, 258, "«FIG» — A braille cell: two columns, four dots, each dot a bit (0x01 to 0x80)")

# ── Figure: the gradients ────────────────────────────────────────────────
GRADS = [
    ("CPU", [0x3fb950, 0xd29922, 0xf85149]),
    ("MEM", [0x1b7c83, 0x39c5cf, 0xa5f3fc]),
    ("USED", [0x56d364, 0xe3b341, 0xf85149]),
    ("GPU", [0x8957e5, 0xbc8cff, 0xf778ba]),
    ("NET", [0x1f6feb, 0x58a6ff, 0xa5d6ff]),
    ("UP", [0x8250df, 0xd2a8ff, 0xf0b6ff]),
    ("DISK", [0x9e6a03, 0xe3b341, 0xf0883e]),
    ("WR", [0xbd561d, 0xf0883e, 0xffa657]),
    ("TEMP", [0x58a6ff, 0x3fb950, 0xd29922, 0xf85149]),
    ("PROC", [0x9a4b10, 0xf0883e, 0xffc680]),
]


def _grad_defs():
    out = "<defs>"
    for name, stops in GRADS:
        out += f'<linearGradient id="tg-{name.lower()}" x1="0" x2="1" y1="0" y2="0">'
        n = len(stops) - 1
        for i, col in enumerate(stops):
            out += f'<stop offset="{i / n:.3f}" stop-color="#{col:06x}"/>'
        out += "</linearGradient>"
    return out + "</defs>"


def _swatches():
    s = _grad_defs()
    for i, (name, _) in enumerate(GRADS):
        col, row = i // 5, i % 5
        x, y = 30 + col * 440, 30 + row * 40
        s += text(x, y + 17, name, 12.5, "#003a90", "700", "start", False)
        s += f'<rect x="{x + 56}" y="{y}" width="340" height="24" rx="6" fill="url(#tg-{name.lower()})"/>'
    return s


GRADIENTS = fig(_swatches(), 900, 230, "«FIG» — The ten gradients of the theme, from the lowest value (left) to the highest (right)")

S1 = p("ngtop uses <b>ratatui</b> 0.30 for what it does well — a screen as a grid of cells, a layout engine, a test "
       "backend — and <b>crossterm</b> 0.29 for the terminal. It does <i>not</i> use ratatui's ready-made widgets. Every "
       "visible element is a small struct in " + c("src/ui/widgets/") + " with a " + c("render(area, buf)")
       + " method that writes cells straight into the ratatui " + c("Buffer") + ". This costs a little code and "
       "buys exact control over characters, colours and widths, the same on every terminal.", lead=True) + \
    flow([
        ("Layout", "split a Rect", "navy"),
        ("Widget.render", "writes cells", "blue"),
        ("Buffer", "the whole frame", "blue"),
        ("convert_buffer", "colour depth", "dark"),
        ("crossterm", "changed cells only", "grey"),
    ], "«FIG» — From a rectangle to the terminal: the only place where colours are converted is after drawing") + \
    p("Drawing is a pure function of " + c("App") + " and " + c("System") + ". It happens inside "
      + c("Terminal::draw") + ", and ratatui sends the terminal only the cells that changed since the last frame. The "
      "same function runs on a ratatui " + c("TestBackend") + " in " + c("src/ui/snapshot.rs") + ": "
      + c("render_app()") + " returns the buffer, " + c("to_text()") + " turns it into lines of text (empty meter cells "
      "become “·” so they can be told from full ones) and " + c("to_ansi()") + " into text with truecolor escape "
      "sequences. This is what " + c("ngtop --snapshot WxH") + " prints.")

S2 = p("Terminals have no exceptions, but the code that writes to a ratatui buffer does: setting a cell outside the "
       "buffer panics. A monitor must survive being resized to one row, or to an enormous size. So no widget calls "
       "ratatui's writing functions directly.", lead=True) + \
    table(["Function", "File", "Rule"], [
        [c("put(buf, x, y, text, style)"), c("widgets/mod.rs"), "Writes nothing if the position is outside the buffer; clips at the right edge"],
        [c("putn(…, n, …)"), c("widgets/mod.rs"), "The same, at most " + c("n") + " cells"],
        [c("clip(s, max)"), c("widgets/mod.rs"), "Cuts a string to " + c("max") + " <i>cells</i>, without splitting a wide character"],
        [c("width(s)"), c("widgets/mod.rs"), "Width in cells with " + c("unicode-width") + ", not the number of characters or bytes"],
        [c("style_area()"), c("widgets/mod.rs"), "Applies a style only to the part of a rectangle inside the buffer"],
        [c("column_spans()"), c("widgets/proctable.rs"), "Column positions from the widths; the flexible column gets the rest; nothing exceeds the area"],
    ], "«TAB» — The safe writing functions") + \
    p("<b>Unicode widths.</b> A character can occupy zero, one or two cells (CJK, many emoji; combining marks take "
      "none). Process names and command lines come from the system and can contain anything. Wherever a text is "
      "placed in a column (" + c("put_line()") + " in " + c("proctable.rs") + "), ngtop advances by the width of each character "
      "and skips zero-width ones; a wide character that does not fit whole in the column is not drawn, so it can never "
      "spill into the next. The characters ngtop draws itself — braille, “■”, block elements, box lines, “▼”, "
      "“▲” — are one cell wide.") + \
    p("<b>Arithmetic.</b> Sizes are " + c("u16") + ". Wherever a product could overflow on a huge terminal or a machine "
      "with thousands of cores, the code widens to " + c("u32") + " first (for example " + c("core_layout()") + ") or uses "
      + c("saturating_sub") + " and " + c("saturating_add") + ".") + \
    note("The test " + c("tutte_le_misure_e_sezioni_senza_panic") + " draws every section at 81 combinations of width "
         "(1 to 182) and height (1 to 52), including one-cell terminals, and renders each to ANSI. It is the guard "
         "of this chapter (" + rif("Testing") + ").", "Guard.")

S3 = p("All widgets are in " + c("src/ui/widgets/") + ". Each takes the data to show and a rectangle.", lead=True) + \
    table(["Widget", "Data", "What it draws"], [
        [c("Panel"), "title, right text, bottom text, colour", "A rounded border (╭ ╮ ╰ ╯ ─ │) in the given colour. The title is bold on the top edge, "
         "the right text dim at the right of the top edge, the bottom text dim on the bottom edge; all clipped to leave the corners. "
         "Returns the inner rectangle; a rectangle under 2 × 2 returns an empty one"],
        [c("BrailleGraph"), "values 0–1, gradient, direction", "A line graph in braille, the newest value at the right (below)"],
        [c("Meter"), "fraction 0–1, gradient", "A bar of “■” cells. The first " + c("round(fraction × width)") + " cells are coloured along the gradient; "
         "the rest use the grey " + c("EMPTY") + ". NaN and values out of range are corrected"],
        [c("Sparkline"), "values 0–1, gradient", "One row of ▁▂▃▄▅▆▇█, one per value; the level is " + c("floor(value × 8)") + ", at most 7; "
         "each block takes the gradient colour of its own value. Right-aligned; missing values are blank"],
        [c("ProcTable"), "columns, visible rows, selection, sort", "A header (dim; the sort column bold with ▼ for descending or ▲ for ascending), then the rows. "
         "Cells are ratatui " + c("Line") + " values so a cell can have several colours"],
        [c("TabBar"), "tab names, active index, right text", "“ngtop 1.5 │” then the five tabs as “ 1 Overview ”, and at the right the host, uptime, load, "
         "interval and clock if there is room. In remote mode a lead text “⇄ host” and the coloured latency are kept even when the rest does not fit, and the tabs leave room for them (" + c("tabs_end()") + "). " + c("tab_ranges()") + " gives the click areas"],
        [c("KeyBar"), "pairs (key, label)", "htop-style bar: the key on a cyan background, the label on a dark one. Items that do not fit are left out whole; the last one (q Quit) is kept when possible"],
        [c("Dialog"), "title, lines, buttons, selected", "A centred red-bordered window; it clears its background first. "
         "Width is the longest line (at least 30) plus 4; height is the lines plus 4"],
    ], "«TAB» — The widgets") + \
    p("The widgets keep no state. All state (selection, scroll offset, histories) is in " + c("App") + ".")

S4 = p("Every graph in ngtop — the CPU history, the network rate, the GPU load — is a braille graph. A braille "
       "character (U+2800 to U+28FF) has a grid of 2 × 4 dots, so one terminal cell carries <b>two columns and four "
       "levels</b> of the graph: eight times the resolution of the cell, with a single character.", lead=True) + BRAILLE + \
    p("The algorithm is " + c("braille_rows(vals, w, h, down)") + " in " + c("src/ui/widgets/braille.rs") + ". The graph is "
      + c("w") + " cells wide and " + c("h") + " cells high, so it has " + c("2w") + " columns of " + c("4h") + " levels.") + \
    table(["Step", "Rule"], [
        ["1. Which values", "The last " + c("2w") + " values of the series. If there are fewer, the missing ones are zero and sit at the "
         "<i>left</i>: the graph fills from the right as time passes"],
        ["2. Level of a value", "<b>level = round(v × 4h)</b>, with " + c("v") + " clamped to 0–1 (NaN is 0). Thus " + c("level") + " dots must be lit from the base"],
        ["3. Which cell", "Text column " + c("c") + " takes values " + c("2c") + " (left dots) and " + c("2c + 1") + " (right dots)"],
        ["4. Which dots", "Rows are counted from the base: row " + c("rb") + " is the one " + c("rb") + " cells above it. In that row, dot " + c("k") + " (0 = nearest the base, "
         "to 3) is lit if <b>level &gt; 4·rb + k</b>"],
        ["5. The bits", "Left column, from the base up: 0x40, 0x04, 0x02, 0x01. Right column: 0x80, 0x20, 0x10, 0x08 (mirrored for " + c("down") + "). "
         "The sum of the lit bits is added to U+2800"],
        ["6. Colour", "A whole row has one colour: the gradient at <b>(rb + 0.5) ÷ h</b>. A tall peak therefore turns from green toward red as it climbs"],
    ], "«TAB» — The braille algorithm") + \
    p("With " + c("down") + " the graph hangs from the top edge and the bit tables are swapped, which gives the mirror "
      "image used under a graph that goes up (network received and sent, GPU history). The rate graphs are given values "
      "already divided by a “round” maximum: " + c("nice_max(v, floor)") + " in " + c("fmt.rs") + " returns the smallest power of "
      "two that is at least the largest recent value (and at least the floor), so the scale does not jump at every cycle.") + \
    tip("For a graph one cell high, " + c("braille_rows(&[1.0, 0.0], 1, 1, false)") + " is U+2847 (the whole left column) and "
        + c("&[0.5, 0.5]") + " is U+28E4 (two dots from the base on both sides). The unit test " + c("caratteri_attesi")
        + " checks exactly these.", "Check it by hand.")

S5 = p("Colours are defined once, in " + c("src/ui/theme.rs") + ", as 24-bit values. They are ten <b>gradients</b>: lists of "
       "two to four colour stops between which the program interpolates. They were taken from the mock-up "
       "(" + c("mockup/tui.js") + ").", lead=True) + GRADIENTS + \
    p(c("grad(stops, t)") + " takes " + c("t") + " from 0 to 1 (outside the range or NaN is brought back to 0–1), scales it to the "
      "number of segments, finds the segment and mixes the two colours channel by channel: <b>colour = a + (b − a) × f</b>, rounded. "
      "One stop gives a flat colour; none gives the default text colour.") + \
    table(["Constant", "Value", "Use"], [
        [c("FG") + ", " + c("DIM") + ", " + c("WHITE"), "#c9d1d9, #6e7681, #f0f6fc", "text, secondary text, emphasis"],
        [c("DARK") + ", " + c("EMPTY"), "#0b0f14, #21262d", "text on a bright background; the unfilled part of a meter"],
        [c("RED") + ", " + c("BLUE") + ", " + c("CYAN"), "#f85149, #58a6ff, #39c5cf", "errors and dialogs; the brand; the key bar"],
        [c("SEL_BG") + ", " + c("KEY_LABEL_BG"), "#13356a, #1c2733", "the selected row; the key bar labels"],
    ], "«TAB» — The other colours") + \
    p("Each panel has a colour taken from the gradient of its content (CPU green, memory cyan, network blue, disks orange, "
      "processes orange, GPU purple). Percentages are coloured with " + c("pct_color()") + ", temperatures by "
      + c("grad(TEMP, (t − 30) ÷ 60)") + ".")

S6 = p("Most terminals can show 24-bit colour, but not all: the Linux console has 16 colours, and some terminals only "
       "256. ngtop draws everything in truecolor and converts at the very end.", lead=True) + \
    table(["Case", "Mode"], [
        ["configuration or " + c("--256") + " says 256 colours", "256"],
        ["configuration says " + c("truecolor"), "truecolor"],
        [c("COLORTERM") + " contains " + c("truecolor") + " or " + c("24bit"), "truecolor"],
        [c("TERM") + " is " + c("linux"), "16 colours"],
        [c("TERM") + " contains " + c("truecolor") + " or " + c("direct"), "truecolor"],
        ["anything else", "256 colours"],
    ], "«TAB» — How the colour mode is chosen (ColorMode::detect, tried in this order)") + \
    p("In the event loop, after " + c("view::draw()") + " and before the buffer is sent, " + c("convert_buffer()") + " visits "
      "every cell once and converts its foreground and background. In truecolor it does nothing. It is the only "
      "place where depth is considered, so no widget needs to know about it.") + \
    table(["Mode", "Conversion"], [
        ["256", c("xterm256(r, g, b)") + ": the xterm palette is a 6 × 6 × 6 cube (levels 0, 95, 135, 175, 215, 255, indices 16 to 231) and a 24-step grey ramp "
         "(indices 232 to 255, levels 8, 18, … 238). The colour is matched to the nearest cube point, channel by channel, and to the nearest grey "
         "(average of the channels). The one with the smaller squared distance " + c("(Δr)² + (Δg)² + (Δb)²") + " wins"],
        ["16", c("ansi16(r, g, b, only8)") + ": the nearest of the 16 base colours by the same squared distance, using the usual xterm values "
         "(black 0,0,0 … white 255,255,255). <b>Backgrounds use only the first eight</b>, because the Linux console cannot show bright backgrounds"],
        ["Other colours", "Terminal-default colours (" + c("Color::Reset") + ") and the 16 named colours are left alone"],
    ], "«TAB» — Colour conversion") + \
    note("A gradient is meant to be read, but on 16 colours it can only be red, green, yellow and so on. The program stays "
         "usable because the numbers are printed next to every bar, never only coloured.", "Colour is never the only signal.")

S7 = p("The interface has two layouts. " + c("LayoutMode::for_size()") + " picks the compact one when the terminal is "
       "narrower than 100 columns or lower than 30 rows. It is recomputed at every resize. The other four sections "
       "adapt by hiding what does not fit; only the Overview is redrawn with a compact version.", lead=True) + \
    table(["Quantity", "Rule", "Where"], [
        ["Left column of the Overview", "<b>width × left_width ÷ 100</b> (default 40 %), at least 60 and at most width − 40", c("draw_big()")],
        ["Height of the CPU panel", "grid rows + header + gap + borders, at least 8, at most one third of the area (up to one half with more than 16 cores); never so tall "
         "that less than 10 rows remain", c("cpu_panel_height()")],
        ["Grid of cores", "Columns needed = cores ÷ available rows. First with a bar and a percentage in each cell; if they do not all fit, only label and "
         "percentage; if still not all, the rest is announced as “+N cores not shown”", c("core_layout()")],
        ["Heights in Network &amp; Disks", "Every block has a minimum and a wanted height; minimums are given by priority (network, connections, interfaces, disks, "
         "filesystems) while they fit, then blocks grow toward the wanted height, and any spare rows go to the highest priority one", c("allocate()")],
        ["GPU panels", "As many columns as fit in 70 cells each; panel height is the one of the richest device; a history, engines and power row only if at least 14 rows are left below the device panels", c("gpu::draw()")],
        ["Table columns", "Columns appear only if the width allows: for example the User column from 50 cells, GPU% from 60 in Processes", c("table_columns()")],
    ], "«TAB» — Layout rules") + \
    p("The compact Overview is a short header — CPU (meter, percentage, temperature, clock, and one sparkline "
      "cell per core), MEM, a GPU line if there is a GPU, NET and DSK — over a short process table "
      "(" + c("small_columns()") + ": PID, User, CPU%, Mem, Command).")

CHAPTER = ("The UI framework", [
    ("ratatui and the frame", S1),
    ("Safe writing and widths", S2),
    ("The widgets", S3),
    ("The braille graph", S4),
    ("Theme and gradients", S5),
    ("Colour depth", S6),
    ("Layout", S7),
])
