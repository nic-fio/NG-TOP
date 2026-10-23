// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Configurazione (`~/.config/ngtop/config.toml`) e opzioni da riga di comando.
//! Parser minimale di un sottoinsieme di TOML: `chiave = valore` con stringhe, interi,
//! booleani e commenti. Nessuna dipendenza.

use std::path::{Path, PathBuf};

use crate::ui::app::{INTERVALS_MS, Section};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorSetting {
    Auto,
    Truecolor,
    C256,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TempUnit {
    Celsius,
    Fahrenheit,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// Intervallo di aggiornamento in ms (250–5000, arrotondato ai passi esistenti).
    pub interval_ms: u64,
    /// Sezione iniziale, 1–5.
    pub section: usize,
    pub mouse: bool,
    pub colors: ColorSetting,
    pub temperature: TempUnit,
    /// Larghezza della colonna sinistra della Panoramica, in % (20–70).
    pub left_width: u16,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            interval_ms: 1000,
            section: 1,
            mouse: true,
            colors: ColorSetting::Auto,
            temperature: TempUnit::Celsius,
            left_width: 40,
        }
    }
}

/// Arrotonda un intervallo al passo esistente più vicino.
pub fn round_interval(ms: u64) -> u64 {
    INTERVALS_MS
        .iter()
        .copied()
        .min_by_key(|v| v.abs_diff(ms))
        .unwrap_or(1000)
}

pub fn section_from_number(n: usize) -> Section {
    Section::ALL[n.clamp(1, Section::ALL.len()) - 1]
}

#[derive(Debug, PartialEq)]
enum Value {
    Str(String),
    Int(i64),
    Bool(bool),
}

/// Toglie un commento finale (un `#` fuori dalle virgolette).
fn strip_comment(line: &str) -> &str {
    let mut in_str = false;
    let mut esc = false;
    for (i, c) in line.char_indices() {
        match c {
            '\\' if in_str => esc = !esc,
            '"' if !esc => in_str = !in_str,
            '#' if !in_str => return &line[..i],
            _ => esc = false,
        }
    }
    line
}

fn parse_value(v: &str) -> Result<Value, String> {
    let v = v.trim();
    if let Some(rest) = v.strip_prefix('"') {
        let body = rest
            .strip_suffix('"')
            .ok_or("string without a closing quote")?;
        let mut out = String::new();
        let mut it = body.chars();
        while let Some(c) = it.next() {
            if c == '\\' {
                match it.next() {
                    Some('"') => out.push('"'),
                    Some('\\') => out.push('\\'),
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    _ => return Err("invalid escape sequence in string".into()),
                }
            } else if c == '"' {
                return Err("unexpected quote inside string".into());
            } else {
                out.push(c);
            }
        }
        return Ok(Value::Str(out));
    }
    match v {
        "true" => return Ok(Value::Bool(true)),
        "false" => return Ok(Value::Bool(false)),
        "" => return Err("missing value".into()),
        _ => {}
    }
    v.replace('_', "")
        .parse::<i64>()
        .map(Value::Int)
        .map_err(|_| format!("invalid value: {v}"))
}

fn int_in(v: Value, key: &str, lo: i64, hi: i64) -> Result<i64, String> {
    match v {
        Value::Int(n) if (lo..=hi).contains(&n) => Ok(n),
        Value::Int(n) => Err(format!("'{key}' must be between {lo} and {hi} (found {n})")),
        _ => Err(format!("'{key}' must be a number")),
    }
}

/// Come `parse_full` ma scarta gli avvisi.
pub fn parse(text: &str) -> Result<Config, String> {
    parse_full(text).map(|(c, _)| c)
}

/// Interpreta il testo di un file di configurazione. Errore: messaggio con il numero di riga.
/// Le chiavi sconosciute non sono un errore: producono un avviso.
pub fn parse_full(text: &str) -> Result<(Config, Vec<String>), String> {
    let mut cfg = Config::default();
    let mut warnings = Vec::new();
    // Un BOM iniziale (file salvati da certi editor) va ignorato.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    for (n, raw) in text.lines().enumerate() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        let at = |e: String| format!("line {}: {e}", n + 1);
        if line.starts_with('[') {
            return Err(at("tables [..] are not supported".into()));
        }
        let (key, val) = line
            .split_once('=')
            .ok_or_else(|| at("missing '=' (expected key = value)".into()))?;
        let key = key.trim();
        let val = parse_value(val).map_err(at)?;
        match key {
            "interval" => {
                cfg.interval_ms = round_interval(int_in(val, key, 250, 5000).map_err(at)? as u64)
            }
            "section" => cfg.section = int_in(val, key, 1, 5).map_err(at)? as usize,
            "left_width" => cfg.left_width = int_in(val, key, 20, 70).map_err(at)? as u16,
            "mouse" => match val {
                Value::Bool(b) => cfg.mouse = b,
                _ => return Err(at("'mouse' must be true or false".into())),
            },
            "colors" => match val {
                Value::Str(s) => {
                    cfg.colors = match s.as_str() {
                        "auto" => ColorSetting::Auto,
                        "truecolor" => ColorSetting::Truecolor,
                        "256" => ColorSetting::C256,
                        _ => {
                            return Err(at(
                                "'colors' must be \"auto\", \"truecolor\" or \"256\"".into()
                            ));
                        }
                    }
                }
                _ => return Err(at("'colors' must be a quoted string".into())),
            },
            "temperature" => match val {
                Value::Str(s) => {
                    cfg.temperature = match s.as_str() {
                        "C" => TempUnit::Celsius,
                        "F" => TempUnit::Fahrenheit,
                        _ => return Err(at("'temperature' must be \"C\" or \"F\"".into())),
                    }
                }
                _ => {
                    return Err(at("'temperature' must be a quoted string".into()));
                }
            },
            other => warnings.push(at(format!("unknown key '{other}', ignored"))),
        }
    }
    Ok((cfg, warnings))
}

/// Testo TOML della configurazione.
pub fn to_toml(c: &Config) -> String {
    format!(
        "# ngtop configuration\n\
         # refresh interval in ms (250–5000)\n\
         interval = {}\n\
         # start section: 1 Overview, 2 GPU, 3 Processes, 4 Network & Disks, 5 Services\n\
         section = {}\n\
         # mouse enabled\n\
         mouse = {}\n\
         # colors: \"auto\", \"truecolor\" or \"256\"\n\
         colors = \"{}\"\n\
         # temperature unit: \"C\" or \"F\"\n\
         temperature = \"{}\"\n\
         # width of the left column of the Overview, in % (20–70)\n\
         left_width = {}\n",
        c.interval_ms,
        c.section,
        c.mouse,
        match c.colors {
            ColorSetting::Auto => "auto",
            ColorSetting::Truecolor => "truecolor",
            ColorSetting::C256 => "256",
        },
        match c.temperature {
            TempUnit::Celsius => "C",
            TempUnit::Fahrenheit => "F",
        },
        c.left_width,
    )
}

/// Percorso predefinito: `$XDG_CONFIG_HOME/ngtop/config.toml`, altrimenti `~/.config/...`.
pub fn default_path(xdg: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    let base = match xdg.filter(|x| !x.is_empty()) {
        Some(x) => PathBuf::from(x),
        None => PathBuf::from(home.filter(|h| !h.is_empty())?).join(".config"),
    };
    Some(base.join("ngtop/config.toml"))
}

pub fn default_path_from_env() -> Option<PathBuf> {
    default_path(
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        std::env::var("HOME").ok().as_deref(),
    )
}

/// Dimensione massima accettata per il file di configurazione.
const MAX_CONFIG_BYTES: u64 = 64 * 1024;

/// Legge la configurazione (con gli eventuali avvisi). File assente: default (o errore se
/// `required`, cioè con `--config`). File non regolari (es. /dev/zero) o troppo grandi: errore.
pub fn load(path: &Path, required: bool) -> Result<(Config, Vec<String>), String> {
    use std::io::Read;
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !required => {
            return Ok((Config::default(), Vec::new()));
        }
        Err(e) => return Err(format!("cannot read the file: {e}")),
    };
    if !meta.is_file() {
        return Err("not a regular file".into());
    }
    if meta.len() > MAX_CONFIG_BYTES {
        return Err(format!(
            "file too large (over {} KiB)",
            MAX_CONFIG_BYTES / 1024
        ));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|f| f.take(MAX_CONFIG_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|e| format!("cannot read the file: {e}"))?;
    if bytes.len() as u64 > MAX_CONFIG_BYTES {
        return Err("file too large".into());
    }
    let text = String::from_utf8(bytes).map_err(|_| "the file is not valid UTF-8".to_string())?;
    parse_full(&text)
}

/// Sostituisce (o aggiunge in fondo) le righe `interval =` e `section =` lasciando il resto.
pub fn update_text(old: &str, c: &Config) -> String {
    let wanted = [
        ("interval", format!("interval = {}", c.interval_ms)),
        ("section", format!("section = {}", c.section)),
    ];
    let mut done = [false; 2];
    let mut out = String::new();
    for line in old.lines() {
        let key = strip_comment(line).split_once('=').map(|(k, _)| k.trim());
        match wanted.iter().position(|(k, _)| Some(*k) == key) {
            Some(i) if !strip_comment(line).trim_start().starts_with('#') => {
                done[i] = true;
                // Mantiene un eventuale commento finale della riga.
                let tail = &line[strip_comment(line).len()..];
                out.push_str(&wanted[i].1);
                if !tail.is_empty() {
                    out.push(' ');
                    out.push_str(tail);
                }
            }
            _ => out.push_str(line),
        }
        out.push('\n');
    }
    for (i, (_, text)) in wanted.iter().enumerate() {
        if !done[i] {
            out.push_str(text);
            out.push('\n');
        }
    }
    out
}

/// Scrive la configurazione in modo atomico (file temporaneo, sync, rename). Se il file esiste
/// cambia solo `interval` e `section`, lasciando commenti e altre righe.
pub fn save(path: &Path, c: &Config) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = match load_text_for_update(path) {
        Some(old) => update_text(&old, c),
        None => to_toml(c),
    };
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let res = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    res
}

/// Testo del file esistente, se è regolare, piccolo e UTF-8.
fn load_text_for_update(path: &Path) -> Option<String> {
    let m = std::fs::metadata(path).ok()?;
    if !m.is_file() || m.len() > MAX_CONFIG_BYTES {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

// ---------------------------------------------------------------- riga di comando

#[derive(Debug, Default, PartialEq)]
pub struct Cli {
    pub help: bool,
    pub version: bool,
    pub delay: Option<u64>,
    pub section: Option<usize>,
    pub no_mouse: bool,
    pub force_256: bool,
    pub config: Option<PathBuf>,
}

pub const USAGE: &str = "\
ngtop — terminal system monitor (CPU, memory, GPU, network, disks, processes, services)

Usage: ngtop [options]

Options:
  -h, --help             show this help
  -V, --version          show the version
  -d, --delay MS         refresh interval in ms (250–5000)
  -s, --section N        start section: 1 Overview, 2 GPU, 3 Processes,
                         4 Network & Disks, 5 Services
      --no-mouse         do not use the mouse
      --256              force 256 colors (for terminals without truecolor)
      --config FILE      use FILE instead of ~/.config/ngtop/config.toml

Diagnostics:
  --dump                 print a text summary and exit (no interface)

Keys: 1-5 sections, F1 help, W save the configuration, q quit.
Command-line options override the configuration file.
";

/// Interpreta gli argomenti (senza il nome del programma).
pub fn parse_args(args: &[String]) -> Result<Cli, String> {
    let mut cli = Cli::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        // Forma --opzione=valore
        let (name, inline) = match a.split_once('=') {
            Some((n, v)) if n.starts_with("--") => (n, Some(v.to_string())),
            _ => (a.as_str(), None),
        };
        let mut value = |label: &str| -> Result<String, String> {
            inline
                .clone()
                .or_else(|| it.next().cloned())
                .ok_or_else(|| format!("option {name} requires a value ({label})"))
        };
        if inline.is_some() && matches!(name, "--help" | "--version" | "--no-mouse" | "--256") {
            return Err(format!("option {name} takes no value"));
        }
        match name {
            "-h" | "--help" => cli.help = true,
            "-V" | "--version" => cli.version = true,
            "--no-mouse" => cli.no_mouse = true,
            "--256" => cli.force_256 = true,
            "-d" | "--delay" => {
                let v = value("milliseconds")?;
                let n: u64 = v.parse().map_err(|_| format!("invalid interval: '{v}'"))?;
                if !(250..=5000).contains(&n) {
                    return Err(format!(
                        "the interval must be between 250 and 5000 ms (found {n})"
                    ));
                }
                cli.delay = Some(round_interval(n));
            }
            "-s" | "--section" => {
                let v = value("1-5")?;
                let n: usize = v.parse().map_err(|_| format!("invalid section: '{v}'"))?;
                if !(1..=5).contains(&n) {
                    return Err(format!(
                        "the section must be a number from 1 to 5 (found {n})"
                    ));
                }
                cli.section = Some(n);
            }
            "--config" => cli.config = Some(PathBuf::from(value("file")?)),
            other => return Err(format!("unknown option '{other}'")),
        }
    }
    Ok(cli)
}

/// Applica le opzioni da riga di comando sopra la configurazione (la riga di comando prevale).
pub fn merge(mut cfg: Config, cli: &Cli) -> Config {
    if let Some(d) = cli.delay {
        cfg.interval_ms = d;
    }
    if let Some(s) = cli.section {
        cfg.section = s;
    }
    if cli.no_mouse {
        cfg.mouse = false;
    }
    if cli.force_256 {
        cfg.colors = ColorSetting::C256;
    }
    cfg
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn config_valida_e_roundtrip() {
        let t = "# commento\ninterval = 2_000 # ms\nsection = 4\nmouse = false\ncolors = \"256\"\ntemperature = \"F\"\nleft_width = 50\n";
        let c = parse(t).unwrap();
        assert_eq!(c.interval_ms, 2000);
        assert_eq!(c.section, 4);
        assert!(!c.mouse);
        assert_eq!(c.colors, ColorSetting::C256);
        assert_eq!(c.temperature, TempUnit::Fahrenheit);
        assert_eq!(c.left_width, 50);
        assert_eq!(parse(&to_toml(&c)).unwrap(), c);
        assert_eq!(parse("").unwrap(), Config::default());
        assert_eq!(parse("  \n# solo commenti\n").unwrap(), Config::default());
        // intervallo arrotondato al passo più vicino
        assert_eq!(parse("interval = 700").unwrap().interval_ms, 500);
        // # dentro una stringa non è un commento
        assert_eq!(
            parse("colors = \"auto\" # ok").unwrap().colors,
            ColorSetting::Auto
        );
    }

    #[test]
    fn config_errori_chiari() {
        for (t, parte) in [
            (
                "interval = 10",
                "line 1: 'interval' must be between 250 and 5000",
            ),
            ("\nsection = 9", "line 2:"),
            ("mouse = 1", "'mouse'"),
            ("colors = \"viola\"", "'colors'"),
            ("temperature = C", "invalid value"),
            ("[tabella]", "tables"),
            ("interval 1000", "missing '='"),
            ("colors = \"auto", "closing quote"),
            ("left_width = 5", "between 20 and 70"),
        ] {
            let e = parse(t).unwrap_err();
            assert!(e.contains(parte), "{t:?} → {e}");
        }
    }

    #[test]
    fn chiavi_sconosciute_bom_e_file_strani() {
        let (c, w) = parse_full("\u{feff}pippo = 1\ninterval = 500\n").unwrap();
        assert_eq!(c.interval_ms, 500);
        assert_eq!(w.len(), 1);
        assert!(w[0].contains("line 1") && w[0].contains("pippo"), "{w:?}");
        // dispositivi e cartelle non si leggono (niente blocco su /dev/zero)
        assert!(
            load(Path::new("/dev/zero"), true)
                .unwrap_err()
                .contains("regular")
        );
        assert!(
            load(Path::new("/tmp"), true)
                .unwrap_err()
                .contains("regular")
        );
        let dir = std::env::temp_dir().join(format!("ngtop-big-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let f = dir.join("grande.toml");
        std::fs::write(&f, vec![b'#'; 70_000]).unwrap();
        assert!(load(&f, true).unwrap_err().contains("too large"));
        std::fs::write(&f, [0xff, 0xfe, 0x00]).unwrap();
        assert!(load(&f, true).unwrap_err().contains("UTF-8"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn salvataggio_atomico_che_conserva_commenti() {
        let dir = std::env::temp_dir().join(format!("ngtop-sv-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("config.toml");
        let old = "# la mia config\ninterval = 1000 # lento\n\nmouse = false\n# section = 2 (commentata)\ncolors = \"256\"\n";
        std::fs::write(&f, old).unwrap();
        let c = Config {
            interval_ms: 250,
            section: 4,
            ..Config::default()
        };
        save(&f, &c).unwrap();
        let t = std::fs::read_to_string(&f).unwrap();
        assert_eq!(
            t,
            "# la mia config\ninterval = 250 # lento\n\nmouse = false\n# section = 2 (commentata)\ncolors = \"256\"\nsection = 4\n"
        );
        // il resto è intatto e il file è ancora valido
        let (p, w) = load(&f, true).unwrap();
        assert!(w.is_empty());
        assert_eq!((p.interval_ms, p.section, p.mouse), (250, 4, false));
        // niente file temporaneo rimasto
        assert!(!dir.join("config.toml.tmp").exists());
        // file nuovo: configurazione completa
        let g = dir.join("nuovo/config.toml");
        save(&g, &c).unwrap();
        assert_eq!(load(&g, true).unwrap().0.interval_ms, 250);
        // un nuovo salvataggio sul file completo non lo altera oltre alle due righe
        save(
            &g,
            &Config {
                section: 2,
                ..c.clone()
            },
        )
        .unwrap();
        assert!(std::fs::read_to_string(&g).unwrap().contains("section = 2"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn percorsi_e_file() {
        assert_eq!(
            default_path(Some("/x"), Some("/h")),
            Some(PathBuf::from("/x/ngtop/config.toml"))
        );
        assert_eq!(
            default_path(None, Some("/h")),
            Some(PathBuf::from("/h/.config/ngtop/config.toml"))
        );
        assert_eq!(
            default_path(Some(""), Some("/h")).unwrap(),
            PathBuf::from("/h/.config/ngtop/config.toml")
        );
        assert_eq!(default_path(None, None), None);
        let dir = std::env::temp_dir().join(format!("ngtop-cfg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let f = dir.join("a/b/config.toml");
        // assente: default, oppure errore se richiesto
        assert_eq!(load(&f, false).unwrap().0, Config::default());
        assert!(load(&f, true).is_err());
        let c = Config {
            interval_ms: 250,
            section: 3,
            ..Config::default()
        };
        save(&f, &c).unwrap();
        assert_eq!(load(&f, true).unwrap().0, c);
        std::fs::write(&f, "section = x").unwrap();
        assert!(load(&f, false).unwrap_err().contains("line 1"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn riga_di_comando() {
        let c = parse_args(&args(&[
            "-d",
            "700",
            "-s",
            "3",
            "--no-mouse",
            "--256",
            "--config",
            "/f",
        ]))
        .unwrap();
        assert_eq!(c.delay, Some(500));
        assert_eq!(c.section, Some(3));
        assert!(c.no_mouse && c.force_256);
        assert_eq!(c.config, Some(PathBuf::from("/f")));
        let c = parse_args(&args(&["--delay=2000", "--section=5", "-h", "-V"])).unwrap();
        assert_eq!(
            (c.delay, c.section, c.help, c.version),
            (Some(2000), Some(5), true, true)
        );
        for bad in [
            &["--help=x"][..],
            &["--version=1"],
            &["--no-mouse=0"],
            &["--256=si"],
        ] {
            let e = parse_args(&args(bad)).unwrap_err();
            assert!(e.contains("takes no value"), "{bad:?} → {e}");
        }
        for bad in [
            &["-d"][..],
            &["-d", "x"],
            &["-d", "100"],
            &["-s", "7"],
            &["--boh"],
            &["-s"],
            &["--config"],
        ] {
            assert!(parse_args(&args(bad)).is_err(), "{bad:?}");
        }
        assert_eq!(parse_args(&[]).unwrap(), Cli::default());
    }

    #[test]
    fn la_riga_di_comando_prevale() {
        let file = Config {
            interval_ms: 2000,
            section: 2,
            ..Config::default()
        };
        let cli = parse_args(&args(&["-d", "250", "--no-mouse", "--256"])).unwrap();
        let m = merge(file.clone(), &cli);
        assert_eq!((m.interval_ms, m.section, m.mouse), (250, 2, false));
        assert_eq!(m.colors, ColorSetting::C256);
        assert_eq!(merge(file.clone(), &Cli::default()), file);
        assert_eq!(section_from_number(3), Section::Processi);
        assert_eq!(section_from_number(99), Section::Servizi);
    }
}
