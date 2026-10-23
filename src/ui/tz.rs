// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Fuso orario locale in puro Rust: TZ (se impostata) oppure /etc/localtime (TZif v1/v2/v3).
//! Serve solo a trovare l'offset corrente dall'UTC; nessuna dipendenza esterna.

use std::path::Path;

use crate::collect::Roots;
use crate::source::Source;

/// Regola POSIX "std offset dst [offset],Mm.w.d[/h],Mm.w.d[/h]".
#[derive(Debug, Clone, PartialEq)]
pub struct PosixTz {
    /// Secondi a est di UTC.
    std_off: i64,
    dst: Option<DstRule>,
}

#[derive(Debug, Clone, PartialEq)]
struct DstRule {
    dst_off: i64,
    start: Rule,
    end: Rule,
}

/// Mese, settimana (5 = ultima), giorno della settimana (0 = domenica), secondi dalla mezzanotte.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Rule {
    month: i64,
    week: i64,
    wday: i64,
    time: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Tz {
    Utc,
    Posix(PosixTz),
    File {
        transitions: Vec<i64>,
        /// Indice del tipo per ogni transizione.
        indices: Vec<u8>,
        /// Offset (secondi a est) per ogni tipo.
        types: Vec<i64>,
        footer: Option<PosixTz>,
    },
}

/// Giorni dal 1970-01-01 per una data civile (algoritmo di H. Hinnant).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn year_of(days: i64) -> i64 {
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    yoe + era * 400 + i64::from(m <= 2)
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        _ => 28,
    }
}

impl Rule {
    /// Secondi Unix (come se fosse UTC) del momento locale in cui scatta la regola nell'anno `y`.
    fn local_secs(&self, y: i64) -> i64 {
        let first = days_from_civil(y, self.month, 1);
        let wd_first = (first + 4).rem_euclid(7); // 1970-01-01 era giovedì
        let mut day = 1 + (self.wday - wd_first).rem_euclid(7) + (self.week - 1) * 7;
        while day > days_in_month(y, self.month) {
            day -= 7;
        }
        (first + day - 1) * 86400 + self.time
    }
}

impl PosixTz {
    pub fn offset_at(&self, unix: i64) -> i64 {
        let Some(d) = &self.dst else {
            return self.std_off;
        };
        let y = year_of((unix + self.std_off).div_euclid(86400));
        let start = d.start.local_secs(y) - self.std_off;
        let end = d.end.local_secs(y) - d.dst_off;
        let in_dst = if start < end {
            unix >= start && unix < end
        } else {
            unix >= start || unix < end
        };
        if in_dst { d.dst_off } else { self.std_off }
    }
}

struct Cursor<'a> {
    s: &'a [u8],
    i: usize,
}

impl Cursor<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn name(&mut self) -> Option<()> {
        if self.peek() == Some(b'<') {
            while self.peek()? != b'>' {
                self.i += 1;
            }
            self.i += 1;
        } else {
            let st = self.i;
            while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
                self.i += 1;
            }
            if self.i - st < 3 {
                return None;
            }
        }
        Some(())
    }

    fn num(&mut self) -> Option<i64> {
        let st = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.i += 1;
        }
        if self.i == st {
            return None;
        }
        std::str::from_utf8(self.s.get(st..self.i)?)
            .ok()?
            .parse()
            .ok()
    }

    /// Salta il carattere `c` obbligatorio.
    fn expect(&mut self, c: u8) -> Option<()> {
        if self.peek()? != c {
            return None;
        }
        self.i += 1;
        Some(())
    }

    /// [+-]hh[:mm[:ss]] in secondi (con segno).
    fn hms(&mut self) -> Option<i64> {
        let sign = match self.peek() {
            Some(b'-') => {
                self.i += 1;
                -1
            }
            Some(b'+') => {
                self.i += 1;
                1
            }
            _ => 1,
        };
        // Ore fino a 167 (limite POSIX esteso), minuti e secondi < 100: niente overflow.
        let hours = self.num()?;
        if hours > 167 {
            return None;
        }
        let mut secs = hours * 3600;
        for mult in [60, 1] {
            if self.peek() == Some(b':') {
                self.i += 1;
                let v = self.num()?;
                if v > 99 {
                    return None;
                }
                secs += v * mult;
            }
        }
        Some(sign * secs)
    }

    fn rule(&mut self) -> Option<Rule> {
        if self.peek()? != b'M' {
            return None; // forme Jn / n non supportate
        }
        self.i += 1;
        let month = self.num()?;
        self.expect(b'.')?;
        let week = self.num()?;
        self.expect(b'.')?;
        let wday = self.num()?;
        if !(1..=12).contains(&month) || !(1..=5).contains(&week) || !(0..=6).contains(&wday) {
            return None;
        }
        let time = if self.peek() == Some(b'/') {
            self.i += 1;
            self.hms()?
        } else {
            7200
        };
        Some(Rule {
            month,
            week,
            wday,
            time,
        })
    }
}

/// Interpreta una stringa POSIX TZ; `None` se non valida.
pub fn parse_posix(s: &str) -> Option<PosixTz> {
    let mut c = Cursor {
        s: s.trim().as_bytes(),
        i: 0,
    };
    c.name()?;
    let std_off = -c.hms()?;
    if c.peek().is_none() {
        return Some(PosixTz { std_off, dst: None });
    }
    c.name()?;
    let dst_off = if matches!(c.peek(), Some(b',') | None) {
        std_off + 3600
    } else {
        -c.hms()?
    };
    if c.peek() != Some(b',') {
        // DST senza regole: non gestito, vale lo standard.
        return Some(PosixTz { std_off, dst: None });
    }
    c.i += 1;
    let start = c.rule()?;
    if c.peek()? != b',' {
        return None;
    }
    c.i += 1;
    let end = c.rule()?;
    Some(PosixTz {
        std_off,
        dst: Some(DstRule {
            dst_off,
            start,
            end,
        }),
    })
}

fn be32(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(i..i + 4)?.try_into().ok()?))
}

fn be64(b: &[u8], i: usize) -> Option<i64> {
    Some(i64::from_be_bytes(b.get(i..i + 8)?.try_into().ok()?))
}

/// Interpreta un file TZif (usa il blocco a 64 bit se presente). Conteggi incoerenti con la
/// lunghezza del file → `None`, prima di allocare qualsiasi cosa.
pub fn parse_tzif(b: &[u8]) -> Option<Tz> {
    if b.get(0..4)? != b"TZif" {
        return None;
    }
    let version = *b.get(4)?;
    // ordine: isutcnt, isstdcnt, leapcnt, timecnt, typecnt, charcnt
    let counts = |h: usize| -> Option<[usize; 6]> {
        let mut c = [0usize; 6];
        for (k, v) in c.iter_mut().enumerate() {
            *v = be32(b, h.checked_add(20 + k * 4)?)? as usize;
        }
        Some(c)
    };
    // Dimensione del blocco dati per transizioni di `t` byte (None se overflow).
    let size = |c: &[usize; 6], t: usize| -> Option<usize> {
        let mut total = c[3].checked_mul(t + 1)?;
        total = total.checked_add(c[4].checked_mul(6)?)?;
        total = total.checked_add(c[5])?;
        total = total.checked_add(c[2].checked_mul(t + 4)?)?;
        total.checked_add(c[1])?.checked_add(c[0])
    };
    let c1 = counts(0)?;
    let (hdr, tsize, c) = if version >= b'2' {
        let h2 = 44usize.checked_add(size(&c1, 4)?)?;
        (h2, 8, counts(h2)?)
    } else {
        (0, 4, c1)
    };
    let data_start = hdr.checked_add(44)?;
    let data_end = data_start.checked_add(size(&c, tsize)?)?;
    if data_end > b.len() || c[4] == 0 {
        return None;
    }
    let timecnt = c[3];
    let mut p = data_start;
    let mut transitions = Vec::with_capacity(timecnt);
    for k in 0..timecnt {
        transitions.push(if tsize == 8 {
            be64(b, p + k * 8)?
        } else {
            i64::from(be32(b, p + k * 4)? as i32)
        });
    }
    p += timecnt * tsize;
    let indices = b.get(p..p + timecnt)?.to_vec();
    p += timecnt;
    let mut types = Vec::with_capacity(c[4]);
    for k in 0..c[4] {
        types.push(i64::from(be32(b, p + k * 6)? as i32));
    }
    let footer = if version >= b'2' {
        b.get(data_end..).and_then(|f| {
            let f = std::str::from_utf8(f).ok()?.trim_start_matches('\n');
            let line = f.split('\n').next()?;
            if line.is_empty() {
                None
            } else {
                parse_posix(line)
            }
        })
    } else {
        None
    };
    Some(Tz::File {
        transitions,
        indices,
        types,
        footer,
    })
}

impl Tz {
    /// Carica il fuso: TZ se impostata (solo per la macchina locale), altrimenti
    /// `<etc>/localtime` letto dalla sorgente; in caso di errore UTC.
    pub fn load(roots: &Roots) -> Tz {
        let env = if roots.src.remote_status().is_none() {
            std::env::var("TZ").ok()
        } else {
            None
        };
        Self::load_with(env.as_deref(), &*roots.src, &roots.etc)
    }

    pub fn load_with(tz_env: Option<&str>, src: &dyn Source, etc: &Path) -> Tz {
        if let Some(v) = tz_env
            .map(|v| v.trim_start_matches(':'))
            .filter(|v| !v.is_empty())
        {
            if matches!(v, "UTC" | "UTC0" | "GMT") {
                return Tz::Utc;
            }
            // Percorso assoluto di un file TZif (es. TZ=:/etc/localtime).
            if v.starts_with('/') {
                return src
                    .read(Path::new(v))
                    .ok()
                    .and_then(|b| parse_tzif(&b))
                    .unwrap_or(Tz::Utc);
            }
            if !v.contains("..") {
                for dir in [
                    "/usr/share/zoneinfo",
                    "/usr/lib/zoneinfo",
                    "/usr/share/lib/zoneinfo",
                ] {
                    let path = Path::new(dir).join(v.trim_start_matches('/'));
                    if let Some(tz) = src.read(&path).ok().and_then(|b| parse_tzif(&b)) {
                        return tz;
                    }
                }
            }
            return parse_posix(v).map_or(Tz::Utc, Tz::Posix);
        }
        src.read(&etc.join("localtime"))
            .ok()
            .and_then(|b| parse_tzif(&b))
            .unwrap_or(Tz::Utc)
    }

    /// Secondi a est di UTC validi all'istante `unix`.
    pub fn offset_at(&self, unix: i64) -> i64 {
        match self {
            Tz::Utc => 0,
            Tz::Posix(p) => p.offset_at(unix),
            Tz::File {
                transitions,
                indices,
                types,
                footer,
            } => {
                let n = transitions.partition_point(|&t| t <= unix);
                // Oltre l'ultima transizione (o senza transizioni) vale la regola del footer.
                if n == transitions.len()
                    && let Some(f) = footer
                {
                    return f.offset_at(unix);
                }
                if n == 0 {
                    return types.first().copied().unwrap_or(0);
                }
                indices
                    .get(n - 1)
                    .and_then(|&i| types.get(i as usize))
                    .copied()
                    .unwrap_or(0)
            }
        }
    }

    /// "hh:mm:ss" locale per un istante Unix.
    pub fn clock(&self, unix: i64) -> String {
        let d = (unix + self.offset_at(unix)).rem_euclid(86400);
        format!("{:02}:{:02}:{:02}", d / 3600, d % 3600 / 60, d % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::LocalSource;

    #[test]
    fn posix_europa() {
        let p = parse_posix("CET-1CEST,M3.5.0,M10.5.0/3").unwrap();
        // 2024-01-15 12:00 UTC, 2024-07-01 12:00 UTC
        assert_eq!(p.offset_at(1705320000), 3600);
        assert_eq!(p.offset_at(1719835200), 7200);
        // 2024-03-31 00:59:59 UTC ancora CET, 01:00:00 UTC già CEST
        assert_eq!(p.offset_at(1711846799), 3600);
        assert_eq!(p.offset_at(1711846800), 7200);
        // 2024-10-27 00:59:59 UTC ancora CEST, 01:00:00 UTC CET
        assert_eq!(p.offset_at(1729990799), 7200);
        assert_eq!(p.offset_at(1729990800), 3600);
    }

    #[test]
    fn nessun_panic_su_prefissi_e_file_troncati() {
        let full = "CET-1CEST,M3.5.0,M10.5.0/3";
        for n in 0..=full.len() {
            let _ = parse_posix(&full[..n]);
        }
        for bad in [
            "CET-1CEST,M3",
            "CET-1CEST,M3.5.0,M10",
            "CET-1CEST,M3.5",
            "CET-1CEST,M13.5.0,M10.5.0",
            "CET-1CEST,M3.5.0,M10.5.0/",
        ] {
            assert!(parse_posix(bad).is_none(), "{bad}");
        }
        let b = std::fs::read("/usr/share/zoneinfo/Europe/Rome").unwrap_or_else(|_| tzif_minimo());
        for n in 0..=b.len() {
            let _ = parse_tzif(&b[..n]);
        }
        assert!(parse_tzif(&b).is_some());
    }

    fn tzif_minimo() -> Vec<u8> {
        let mut b = b"TZif2".to_vec();
        b.extend([0u8; 15]);
        for c in [0u32, 0, 0, 0, 1, 4] {
            b.extend(c.to_be_bytes());
        }
        b.extend(3600i32.to_be_bytes());
        b.extend([0u8, 0u8]);
        b.extend(b"AAA\0");
        // blocco v2 identico + footer
        b.extend(b"TZif2");
        b.extend([0u8; 15]);
        for c in [0u32, 0, 0, 0, 1, 4] {
            b.extend(c.to_be_bytes());
        }
        b.extend(3600i32.to_be_bytes());
        b.extend([0u8, 0u8]);
        b.extend(b"AAA\0");
        b.extend(b"\nCET-1CEST,M3.5.0,M10.5.0/3\n");
        b
    }

    #[test]
    fn posix_semplici_e_non_validi() {
        assert_eq!(parse_posix("JST-9").unwrap().offset_at(0), 32400);
        assert_eq!(parse_posix("<-03>3").unwrap().offset_at(0), -10800);
        // emisfero sud
        let p = parse_posix("AEST-10AEDT,M10.1.0,M4.1.0/3").unwrap();
        assert_eq!(p.offset_at(1705320000), 39600); // gennaio: estate
        assert_eq!(p.offset_at(1719835200), 36000);
        assert!(parse_posix("").is_none());
        assert!(parse_posix("x").is_none());
        assert!(parse_posix("CET-1CEST,J1,J2").is_none());
        assert!(parse_posix("XXX-99999999999999999999").is_none());
        assert!(parse_posix("XXX-200").is_none());
        assert!(parse_posix("XXX-1:100").is_none());
    }

    #[test]
    fn tzif_reale_se_presente() {
        let Ok(b) = std::fs::read("/usr/share/zoneinfo/Europe/Rome") else {
            return;
        };
        let tz = parse_tzif(&b).unwrap();
        assert_eq!(tz.offset_at(1705320000), 3600);
        assert_eq!(tz.offset_at(1719835200), 7200);
        assert_eq!(tz.offset_at(4_000_000_000 + 86400 * 182) % 3600, 0);
        assert_eq!(tz.clock(1705320000), "13:00:00");
    }

    #[test]
    fn tzif_inventato_e_rotto() {
        // v1 con una transizione: prima UTC+1 poi UTC+2
        let mut b = b"TZif".to_vec();
        b.push(0);
        b.extend([0u8; 15]);
        for c in [0u32, 0, 0, 1, 2, 4] {
            b.extend(c.to_be_bytes());
        }
        b.extend(1000i32.to_be_bytes());
        b.push(1);
        for off in [3600i32, 7200] {
            b.extend(off.to_be_bytes());
            b.extend([0u8, 0u8]);
        }
        b.extend(b"AAA\0");
        let tz = parse_tzif(&b).unwrap();
        assert_eq!(tz.offset_at(0), 3600);
        assert_eq!(tz.offset_at(2000), 7200);
        assert!(parse_tzif(b"TZif").is_none());
        // intestazione falsa con conteggi enormi e file troncato: nessuna allocazione, solo None
        let mut bad = b"TZif2".to_vec();
        bad.extend([0u8; 15]);
        for c in [0u32, 0, 0, 0xFFFF_FFFF, 0xFFFF_FFFF, 0xFFFF_FFFF] {
            bad.extend(c.to_be_bytes());
        }
        assert!(parse_tzif(&bad).is_none());
        let mut bad1 = b"TZif".to_vec();
        bad1.extend([0u8; 16]);
        for c in [0u32, 0, 0, 0xFFFF_FFFF, 1, 4] {
            bad1.extend(c.to_be_bytes());
        }
        assert!(parse_tzif(&bad1).is_none());
        assert!(parse_tzif(&b[..b.len() - 3]).is_none());
        assert!(parse_tzif(b"nope").is_none());
        assert!(parse_tzif(&b[..50]).is_none());
        assert_eq!(Tz::Utc.clock(3661), "01:01:01");
    }

    #[test]
    fn env_tz() {
        let etc = Path::new("/nonexistent-ngtop");
        assert_eq!(Tz::load_with(Some("UTC"), &LocalSource, etc), Tz::Utc);
        assert_eq!(Tz::load_with(None, &LocalSource, etc), Tz::Utc);
        assert_eq!(
            Tz::load_with(Some("JST-9"), &LocalSource, etc).offset_at(0),
            32400
        );
        assert_eq!(
            Tz::load_with(Some("../../etc/passwd"), &LocalSource, etc),
            Tz::Utc
        );
        // percorso assoluto, con o senza ':'
        let dir = std::env::temp_dir().join(format!("ngtop-tz-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let f = dir.join("zona");
        let mut b = b"TZif".to_vec();
        b.push(0);
        b.extend([0u8; 15]);
        for c in [0u32, 0, 0, 0, 1, 4] {
            b.extend(c.to_be_bytes());
        }
        b.extend(7200i32.to_be_bytes());
        b.extend([0u8, 0u8]);
        b.extend(b"AAA\0");
        let _ = std::fs::write(&f, &b);
        let p = f.to_string_lossy().to_string();
        assert_eq!(
            Tz::load_with(Some(&p), &LocalSource, etc).offset_at(0),
            7200
        );
        assert_eq!(
            Tz::load_with(Some(&format!(":{p}")), &LocalSource, etc).offset_at(0),
            7200
        );
        let _ = std::fs::remove_dir_all(&dir);
        // senza transizioni ma con footer POSIX vale il footer
        let tz = Tz::File {
            transitions: vec![],
            indices: vec![],
            types: vec![0],
            footer: parse_posix("JST-9"),
        };
        assert_eq!(tz.offset_at(5), 32400);
    }
}
