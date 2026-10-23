// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Formattazione di numeri per la visualizzazione.

/// Byte in forma compatta: 512K, 3.9G…
pub fn bytes(b: u64) -> String {
    const U: [&str; 5] = ["B", "K", "M", "G", "T"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 || v >= 100.0 {
        format!("{:.0}{}", v, U[i])
    } else if v >= 10.0 {
        format!("{:.1}{}", v, U[i])
    } else {
        format!("{:.2}{}", v, U[i])
    }
}

/// Uptime: "3d 04:12" oppure "04:12:05" sotto il giorno.
pub fn uptime(secs: f64) -> String {
    let s = if secs.is_finite() && secs > 0.0 {
        secs as u64
    } else {
        0
    };
    if s >= 86400 {
        format!(
            "{}d {:02}:{:02}",
            s / 86400,
            s % 86400 / 3600,
            s % 3600 / 60
        )
    } else {
        format!("{:02}:{:02}:{:02}", s / 3600, s % 3600 / 60, s % 60)
    }
}

/// Ora UTC hh:mm:ss da un tempo Unix.
pub fn clock_utc(unix_secs: u64) -> String {
    let d = unix_secs % 86400;
    format!("{:02}:{:02}:{:02}", d / 3600, d % 3600 / 60, d % 60)
}

/// Velocità in byte/s: "512B/s", "1.20MB/s".
pub fn rate(v: f64) -> String {
    let b = bytes(if v.is_finite() && v > 0.0 {
        v as u64
    } else {
        0
    });
    if b.ends_with('B') {
        format!("{b}/s")
    } else {
        format!("{b}B/s")
    }
}

/// Potenza di due che contiene `v` (almeno `floor`): scala "tonda" dei grafici di velocità.
pub fn nice_max(v: f64, floor: f64) -> f64 {
    let v = if v.is_finite() { v.max(floor) } else { floor }.max(1.0);
    2f64.powf(v.log2().ceil())
}

static FAHRENHEIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Imposta l'unità di temperatura usata da `temp` (vale per tutta l'interfaccia).
pub fn set_fahrenheit(on: bool) {
    FAHRENHEIT.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// "71°C" oppure "160°F" nell'unità scelta.
pub fn temp(celsius: f64) -> String {
    temp_in(
        celsius,
        FAHRENHEIT.load(std::sync::atomic::Ordering::Relaxed),
    )
}

pub fn temp_in(celsius: f64, fahrenheit: bool) -> String {
    if fahrenheit {
        format!("{:.0}°F", celsius * 9.0 / 5.0 + 32.0)
    } else {
        format!("{celsius:.0}°C")
    }
}

/// Etichetta dell'unità ("°C" o "°F").
pub fn temp_unit() -> &'static str {
    if FAHRENHEIT.load(std::sync::atomic::Ordering::Relaxed) {
        "°F"
    } else {
        "°C"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formati() {
        assert_eq!(bytes(0), "0B");
        assert_eq!(bytes(1536), "1.50K");
        assert_eq!(bytes(5 * 1024 * 1024 * 1024), "5.00G");
        assert_eq!(bytes(300 * 1024 * 1024), "300M");
        assert_eq!(uptime(90061.0), "1d 01:01");
        assert_eq!(uptime(61.0), "00:01:01");
        assert_eq!(uptime(f64::NAN), "00:00:00");
        assert_eq!(clock_utc(86400 + 3661), "01:01:01");
        assert_eq!(temp_in(71.4, false), "71°C");
        assert_eq!(temp_in(100.0, true), "212°F");
        assert_eq!(temp_in(-40.0, true), "-40°F");
        assert_eq!(rate(512.0), "512B/s");
        assert_eq!(rate(1.5 * 1048576.0), "1.50MB/s");
        assert_eq!(rate(f64::NAN), "0B/s");
        assert_eq!(nice_max(3.0e6, 1e5), 4194304.0);
        assert_eq!(nice_max(0.0, 100.0 * 1024.0), 131072.0);
        assert_eq!(nice_max(4194304.0, 1.0), 4194304.0);
        assert_eq!(nice_max(f64::NAN, 1024.0), 1024.0);
    }
}
