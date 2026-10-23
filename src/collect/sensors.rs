// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Sensori: temperatura della CPU da hwmon (ripiego thermal_zone).

use std::path::{Path, PathBuf};

use super::read_text;

fn list_dirs(dir: &Path, prefix: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(prefix))
        .map(|e| e.path())
        .collect();
    v.sort();
    v
}

/// File `tempN_input` (o thermal_zone) da leggere per la temperatura della CPU.
pub fn find_cpu_temp_path(sys: &Path) -> Option<PathBuf> {
    for hw in list_dirs(&sys.join("class/hwmon"), "hwmon") {
        let name = read_text(&hw.join("name")).unwrap_or_default();
        let name = name.trim();
        let wanted: &[&str] = match name {
            "coretemp" => &["Package id 0"],
            "k10temp" => &["Tctl", "Tdie"],
            "zenpower" => &["Tdie", "Tctl"],
            "cpu_thermal" | "cpu-thermal" => &[],
            _ => continue,
        };
        for label in wanted {
            for n in 1..=16 {
                let l = read_text(&hw.join(format!("temp{n}_label")));
                if l.as_deref().map(str::trim) == Some(*label) {
                    return Some(hw.join(format!("temp{n}_input")));
                }
            }
        }
        let t1 = hw.join("temp1_input");
        if t1.exists() {
            return Some(t1);
        }
    }
    for z in list_dirs(&sys.join("class/thermal"), "thermal_zone") {
        if read_text(&z.join("type")).as_deref().map(str::trim) == Some("x86_pkg_temp") {
            return Some(z.join("temp"));
        }
    }
    None
}

/// Legge un file di temperatura (millesimi di grado) e restituisce °C.
pub fn read_temp(path: &Path) -> Option<f64> {
    let v: f64 = read_text(path)?.trim().parse().ok()?;
    Some(v / 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ngtop-sens-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn coretemp_e_ripiego() {
        let d = tmp("a");
        let hw = d.join("class/hwmon/hwmon3");
        fs::create_dir_all(&hw).unwrap_or(());
        fs::write(hw.join("name"), "coretemp\n").unwrap_or(());
        fs::write(hw.join("temp1_label"), "Core 0\n").unwrap_or(());
        fs::write(hw.join("temp2_label"), "Package id 0\n").unwrap_or(());
        fs::write(hw.join("temp2_input"), "62500\n").unwrap_or(());
        let p = find_cpu_temp_path(&d);
        assert_eq!(p, Some(hw.join("temp2_input")));
        assert_eq!(read_temp(&hw.join("temp2_input")), Some(62.5));
        assert_eq!(read_temp(&hw.join("manca")), None);
        let _ = fs::remove_dir_all(&d);

        let d = tmp("b");
        let z = d.join("class/thermal/thermal_zone0");
        fs::create_dir_all(&z).unwrap_or(());
        fs::write(z.join("type"), "x86_pkg_temp\n").unwrap_or(());
        fs::write(z.join("temp"), "50000\n").unwrap_or(());
        assert_eq!(find_cpu_temp_path(&d), Some(z.join("temp")));
        let _ = fs::remove_dir_all(&d);
        assert_eq!(find_cpu_temp_path(&tmp("c")), None);
    }
}
