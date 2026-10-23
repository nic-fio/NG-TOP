// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Tabelle dei tasti: barre in basso e aiuto F1. Sono l'unica fonte, usata anche da `--doc-json`.

pub use super::sections::processi::KEYS_PROC;

pub const KEYS_BIG: &[(&str, &str)] = &[
    ("F1", "Help"),
    ("F3", "Search"),
    ("F4", "Filter"),
    ("F5", "Tree"),
    ("F6", "Sort"),
    ("F9", "Signal"),
    ("+/-", "Interval"),
    ("1-5", "Sections"),
    ("q", "Quit"),
];
pub const KEYS_SMALL: &[(&str, &str)] = &[
    ("F1", "Help"),
    ("/", "Filter"),
    ("F5", "Tree"),
    ("F6", "Sort"),
    ("F9", "Sig"),
    ("q", "Quit"),
];
pub const KEYS_GPU: &[(&str, &str)] = &[
    ("d", "Device"),
    ("F6", "Sort"),
    ("F9", "Signal"),
    ("+/-", "Interval"),
    ("1-5", "Sections"),
    ("q", "Quit"),
];
pub const KEYS_RETE: &[(&str, &str)] = &[
    ("i", "Interface"),
    ("c", "TCP/UDP"),
    ("l", "LISTEN"),
    ("Enter", "Go to process"),
    ("F6", "Sort"),
    ("z", "Reset peaks/totals"),
    ("1-5", "Sections"),
    ("q", "Quit"),
];
pub const KEYS_SERVIZI: &[(&str, &str)] = &[
    ("/", "Search"),
    ("f", "Filter"),
    ("u", "System/User"),
    ("s", "Start"),
    ("t", "Stop"),
    ("r", "Restart"),
    ("e", "Enable/Disable"),
    ("j", "Journal"),
    ("1-5", "Sections"),
    ("q", "Quit"),
];
pub const KEYS_FILTER: &[(&str, &str)] = &[("Enter", "Confirm"), ("Esc", "Cancel and clear")];

/// Tasti validi in ogni sezione (aiuto F1, riga "Everywhere").
pub const GLOBAL_KEYS: &[(&str, &str)] = &[
    ("1-5 Tab Shift-Tab", "switch section"),
    ("+/-", "refresh interval"),
    ("W", "save configuration"),
    ("q F10 Ctrl-C", "quit"),
    ("Mouse", "click tabs, rows, headers; wheel scrolls"),
];

/// Tasti per sezione nell'aiuto F1: (titolo della sezione, voci). Stesso ordine di `Section::ALL`.
pub const SECTION_HELP: &[(&str, &[(&str, &str)])] = &[
    (
        "Overview",
        &[
            ("Up Down PgUp PgDn", "select"),
            ("F6 < >", "sort"),
            ("i", "invert"),
            ("F5", "list/tree"),
            ("/ F3 F4", "filter"),
            ("F9", "signal"),
            ("n", "switch network interface"),
            ("d", "switch GPU"),
        ],
    ),
    (
        "GPU",
        &[
            ("d", "switch GPU"),
            ("F6", "sort"),
            ("/", "filter"),
            ("F9", "signal"),
            ("NVIDIA", "proprietary driver, read through nvidia-smi"),
        ],
    ),
    (
        "Processes",
        &[
            ("Space", "fold branch"),
            ("F5", "list/tree"),
            ("\u{2190} \u{2192}", "tabs"),
            ("[ ]", "scroll"),
            ("F7 F8", "nice -1/+1 (main thread only, like htop)"),
            ("F9 k", "signal: Up/Down choose, Space subtree, Enter send"),
        ],
    ),
    (
        "Network & Disks",
        &[
            ("i", "interface"),
            ("c", "TCP/UDP"),
            ("l", "LISTEN"),
            ("F6", "sort"),
            ("z", "reset"),
            ("Enter", "go to process"),
            ("F9", "signal to the connection's process"),
        ],
    ),
    (
        "Services",
        &[
            ("/", "search"),
            ("f", "filter"),
            ("u", "system/user (this machine only)"),
            (
                "s t r",
                "start/stop/restart (remote: needs root, see --sudo)",
            ),
            ("e", "enable/disable"),
            ("j", "full-screen journal"),
        ],
    ),
];

/// Barra dei tasti di ciascuna sezione (grande), nell'ordine di `Section::ALL`.
pub fn section_bars() -> [&'static [(&'static str, &'static str)]; 5] {
    [KEYS_BIG, KEYS_GPU, KEYS_PROC, KEYS_RETE, KEYS_SERVIZI]
}

fn join(entries: &[(&str, &str)], limit: usize) -> Vec<String> {
    // Voci separate da " · ", righe lunghe al massimo `limit` colonne.
    let mut lines = Vec::new();
    let mut cur = String::new();
    for (k, d) in entries {
        let item = format!("{k} {d}");
        if !cur.is_empty() && cur.chars().count() + 3 + item.chars().count() > limit {
            lines.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push_str(" \u{b7} ");
        }
        cur.push_str(&item);
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// Righe dell'aiuto F1, generate dalle tabelle.
/// `cols` è la larghezza dello schermo: le righe vanno a capo per stare nella finestra.
pub fn help_lines(cols: usize) -> Vec<String> {
    let room = cols.saturating_sub(4).clamp(20, 100);
    let mut out = Vec::new();
    for (i, l) in join(GLOBAL_KEYS, room.saturating_sub(13))
        .into_iter()
        .enumerate()
    {
        out.push(if i == 0 {
            format!("Everywhere: {l}")
        } else {
            format!("            {l}")
        });
    }
    for (name, entries) in SECTION_HELP {
        out.push((*name).to_string());
        out.extend(
            join(entries, room.saturating_sub(2))
                .into_iter()
                .map(|l| format!("  {l}")),
        );
    }
    out
}
