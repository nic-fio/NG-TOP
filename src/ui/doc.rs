// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! `ngtop --doc-json` (nascosto): tasti e opzioni in JSON, generati dalle stesse tabelle
//! e dallo stesso testo `USAGE` che usa il programma, per i manuali.

use serde_json::{Value, json};

use super::app::Section;
use super::keys::{GLOBAL_KEYS, KEYS_FILTER, KEYS_SMALL, SECTION_HELP, section_bars};
use crate::config::USAGE;

fn pairs(t: &[(&str, &str)]) -> Value {
    Value::Array(t.iter().map(|(k, d)| json!([k, d])).collect())
}

/// Opzioni lette da `USAGE`: righe indentate che iniziano con `-`, aiuto separato da 2+ spazi;
/// le righe di continuazione (molto indentate) si accodano all'aiuto precedente.
pub fn options() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in USAGE.lines() {
        let t = line.trim_start();
        let indent = line.len() - t.len();
        if indent > 0 && t.starts_with('-') {
            if let Some(i) = t.find("  ") {
                out.push((t[..i].to_string(), t[i..].trim().to_string()));
                continue;
            }
        } else if indent > 10
            && !t.is_empty()
            && let Some(last) = out.last_mut()
        {
            last.1.push(' ');
            last.1.push_str(t);
        }
    }
    out
}

pub fn doc_json() -> String {
    let bars = section_bars();
    let sections: Vec<Value> = Section::ALL
        .iter()
        .enumerate()
        .map(|(i, s)| {
            json!({
                "name": s.title(),
                "keys": pairs(bars[i]),
                "help": pairs(SECTION_HELP[i].1),
            })
        })
        .collect();
    let options: Vec<Value> = options()
        .into_iter()
        .map(|(f, h)| json!({"flag": f, "help": h}))
        .collect();
    let v = json!({
        "version": env!("CARGO_PKG_VERSION"),
        "sections": sections,
        "compact_keys": pairs(KEYS_SMALL),
        "filter_keys": pairs(KEYS_FILTER),
        "global_keys": pairs(GLOBAL_KEYS),
        "options": options,
    });
    serde_json::to_string_pretty(&v).unwrap_or_default() + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_valido_con_tutte_le_sezioni() {
        let v: Value = serde_json::from_str(&doc_json()).expect("JSON valido");
        let names: Vec<&str> = v["sections"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["name"].as_str().unwrap())
            .collect();
        let want: Vec<&str> = Section::ALL.iter().map(|s| s.title()).collect();
        assert_eq!(names, want);
        for s in v["sections"].as_array().unwrap() {
            assert!(!s["keys"].as_array().unwrap().is_empty());
            assert!(!s["help"].as_array().unwrap().is_empty());
        }
        assert!(!v["global_keys"].as_array().unwrap().is_empty());
        assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
        let flags: Vec<&str> = v["options"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| o["flag"].as_str().unwrap())
            .collect();
        for f in [
            "-h, --help",
            "-d, --delay MS",
            "--config FILE",
            "--host TARGET",
            "--local",
            "--dump [TARGET]",
        ] {
            assert!(flags.contains(&f), "{f} in {flags:?}");
        }
        assert!(
            v["options"][3]["help"]
                .as_str()
                .unwrap()
                .contains("Network & Disks")
        );
    }
}
