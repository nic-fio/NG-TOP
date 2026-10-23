# Copyright (c) 2026 Nicola Fiorillo
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
"""User Manual, chapter 12 — reference of the keys. The tables are generated from the program (doc.json, produced by
`ngtop --doc-json`); the keys of the windows are from src/ui/app.rs, processi.rs (dialog_key), servizi/mod.rs (on_key)."""
from build import key, keys_table, p, rif, table, ui

S_GLOBAL = p("The keys that work in every section. This table and the next ones are produced by the program itself.", lead=True) + \
    keys_table("global")

S_OV = p("The Overview (key " + key("1") + ").", lead=True) + keys_table("Overview")
S_GPU = p("The GPU section (key " + key("2") + ").", lead=True) + keys_table("GPU")
S_PROC = p("The Processes section (key " + key("3") + ").", lead=True) + keys_table("Processes")
S_NET = p("The Network & Disks section (key " + key("4") + ").", lead=True) + keys_table("Network & Disks")
S_SVC = p("The Services section (key " + key("5") + ").", lead=True) + keys_table("Services")

S_WIN = p("When a window or a box is open, it takes the keys. Everything else is ignored until you close it.", lead=True) + \
    table(["Window", "Key", "Action"], [
        [ui("Help"), key("Esc") + " " + key("Enter") + " " + key("F1") + " " + key("q"), "Close."],
        [ui("Send signal"), key("↑") + " " + key("↓"), "Choose the signal."],
        ["", key("Space"), "Send to the whole subtree, or only to the process."],
        ["", key("Enter"), "Send. Press it a second time when the window asks for confirmation."],
        ["", key("Esc"), "Cancel."],
        ["Confirmation of a service action", key("←") + " " + key("→") + " " + key("Tab") + " " + key("Shift", "Tab"), "Choose " + ui("Cancel") + " or " + ui("Confirm") + "."],
        ["", key("Enter"), "Do what the selected button says."],
        ["", key("Esc") + " " + key("n"), "Cancel."],
        ["Filter box (processes) and search box (services)", "any letter", "Add it to the text."],
        ["", key("Backspace"), "Delete the last letter."],
        ["", key("↑") + " " + key("↓"), "Move the selection while the box is open."],
        ["", key("Enter"), "Close the box and keep the text."],
        ["", key("Esc"), "Close the box and clear the text."],
        [ui("journal") + " full screen", key("j") + " " + key("Esc"), "Back to the list."],
    ], "«TAB» — Keys inside windows and boxes") + \
    p("The tables of the sections list every key. Where the key bar shows two labels for one key (for example " + ui("Search") + " and " +
      ui("Filter") + "), they are two names for the same action.")

CHAPTER = ("Key reference", [
    ("Keys valid in every section", S_GLOBAL),
    ("Overview keys", S_OV),
    ("GPU keys", S_GPU),
    ("Processes keys", S_PROC),
    ("Network & Disks keys", S_NET),
    ("Services keys", S_SVC),
    ("Keys inside windows and boxes", S_WIN),
])
