// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! NVIDIA: lettura delle uscite di `nvidia-smi` (query CSV, `pmon`, `--query-compute-apps`).
//! Solo funzioni pure su testo: chi lancia il comando è `gpu.rs`. L'uscita è trattata come ostile:
//! lunghezze limitate, caratteri di controllo tolti, numeri limitati, mai panic.

use super::gpu::WorkKind;

/// Byte massimi di un'uscita considerati: il resto si scarta.
pub const MAX_OUTPUT: usize = 256 * 1024;
/// Schede massime lette da una query.
pub const MAX_GPUS: usize = 64;
/// Processi massimi letti da `pmon` / `--query-compute-apps`.
pub const MAX_PROCS: usize = 4096;
/// Caratteri massimi del nome di una scheda.
pub const MAX_NAME: usize = 96;

/// Campi chiesti a `--query-gpu`, nell'ordine in cui arrivano le colonne.
pub const FIELDS_FULL: &[&str] = &[
    "index",
    "pci.bus_id",
    "name",
    "utilization.gpu",
    "utilization.encoder",
    "utilization.decoder",
    "memory.used",
    "memory.total",
    "clocks.gr",
    "clocks.max.gr",
    "clocks.mem",
    "temperature.gpu",
    "power.draw",
    "power.limit",
    "fan.speed",
    "pcie.link.gen.current",
    "pcie.link.width.current",
    "pcie.link.gen.max",
    "pcie.link.width.max",
];

/// Ripiego per i driver che rifiutano qualche campo di `FIELDS_FULL`.
pub const FIELDS_BASE: &[&str] = &[
    "index",
    "pci.bus_id",
    "name",
    "utilization.gpu",
    "memory.used",
    "memory.total",
    "clocks.gr",
    "clocks.max.gr",
    "clocks.mem",
    "temperature.gpu",
    "power.draw",
    "power.limit",
    "fan.speed",
    "pcie.link.gen.current",
    "pcie.link.width.current",
    "pcie.link.gen.max",
    "pcie.link.width.max",
];

/// Una scheda letta da `--query-gpu`. `None` = campo assente, `[N/A]`, `[Not Supported]` o non valido.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NvGpu {
    /// Indice di `nvidia-smi` (lo usa `pmon`).
    pub index: Option<u32>,
    /// Indirizzo PCI nella forma di sysfs, es. "0000:01:00.0".
    pub bus_id: Option<String>,
    /// Nome ripulito (niente caratteri di controllo, al massimo `MAX_NAME`).
    pub name: Option<String>,
    /// % 0–100.
    pub usage: Option<f64>,
    pub enc: Option<f64>,
    pub dec: Option<f64>,
    /// Byte.
    pub vram_used: Option<u64>,
    pub vram_total: Option<u64>,
    pub clock_mhz: Option<u64>,
    pub clock_max_mhz: Option<u64>,
    pub mem_clock_mhz: Option<u64>,
    /// °C.
    pub temp: Option<f64>,
    pub power_w: Option<f64>,
    pub power_cap_w: Option<f64>,
    /// % 0–100.
    pub fan_pct: Option<f64>,
    /// Stesso formato di `Gpu::pcie`, es. "4.0 ×16".
    pub pcie: Option<String>,
    pub pcie_max: Option<String>,
}

/// Un processo letto da `pmon` o da `--query-compute-apps`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NvProc {
    /// Indice della scheda (`pmon`).
    pub gpu_index: Option<u32>,
    /// Indirizzo PCI nella forma di sysfs (`--query-compute-apps`).
    pub bus_id: Option<String>,
    pub pid: u32,
    /// G → Graphics, C → Compute, "C+G" → Graphics; altrimenti None.
    pub kind: WorkKind,
    /// % 0–100 (`sm` di pmon).
    pub gpu_pct: Option<f64>,
    pub enc_pct: Option<f64>,
    pub dec_pct: Option<f64>,
    /// Memoria della scheda in byte.
    pub mem: Option<u64>,
}

/// Limiti dei valori accettati (fuori → `None`).
const MAX_PCT: f64 = 100.0;
const MIN_TEMP: f64 = -50.0;
const MAX_TEMP: f64 = 200.0;
const MAX_WATT: f64 = 5000.0;
const MAX_MHZ: f64 = 100_000.0;
/// MiB massimi per una memoria (1 PiB): oltre è un dato assurdo.
const MAX_MIB: f64 = 1_073_741_824.0;
const MIB: f64 = 1_048_576.0;

/// Argomenti di `nvidia-smi` per la query delle schede con i campi dati.
pub fn query_args(fields: &[&str]) -> Vec<String> {
    vec![
        format!("--query-gpu={}", fields.join(",")),
        "--format=csv,noheader,nounits".to_string(),
    ]
}

/// Argomenti di `nvidia-smi pmon` per un solo campione.
pub fn pmon_args() -> Vec<String> {
    ["pmon", "-c", "1", "-s", "um"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// Argomenti di `nvidia-smi --query-compute-apps`.
pub fn compute_apps_args() -> Vec<String> {
    vec![
        "--query-compute-apps=gpu_bus_id,pid,used_memory".to_string(),
        "--format=csv,noheader,nounits".to_string(),
    ]
}

/// Taglia il testo a `MAX_OUTPUT` byte su un confine di carattere.
fn limita(text: &str) -> &str {
    if text.len() <= MAX_OUTPUT {
        return text;
    }
    let mut n = MAX_OUTPUT;
    while n > 0 && !text.is_char_boundary(n) {
        n -= 1;
    }
    &text[..n]
}

/// Valore di una cella; `None` per vuoto, "-", "N/A", "[Not Supported]", "[Unknown Error]", ecc.
fn cella(s: &str) -> Option<&str> {
    let t = s.trim();
    if t.is_empty() || t.starts_with('[') || t == "-" || t.eq_ignore_ascii_case("n/a") {
        None
    } else {
        Some(t)
    }
}

/// Numero finito entro `[lo, hi]`.
fn numero(s: &str, lo: f64, hi: f64) -> Option<f64> {
    let v: f64 = cella(s)?.parse().ok()?;
    (v.is_finite() && v >= lo && v <= hi).then_some(v)
}

fn percento(s: &str) -> Option<f64> {
    numero(s, 0.0, MAX_PCT)
}

fn mhz(s: &str) -> Option<u64> {
    numero(s, 0.0, MAX_MHZ).map(|v| v.round() as u64)
}

/// Memoria in MiB → byte.
fn mib_in_byte(s: &str) -> Option<u64> {
    numero(s, 0.0, MAX_MIB).map(|v| (v * MIB) as u64)
}

fn indice(s: &str) -> Option<u32> {
    cella(s)?.parse().ok()
}

/// Nome senza caratteri di controllo, al massimo `MAX_NAME` caratteri.
fn nome_pulito(s: &str) -> Option<String> {
    let n: String = s
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_NAME)
        .collect();
    let n = n.trim().to_string();
    (!n.is_empty()).then_some(n)
}

/// Generazione e larghezza del link PCIe → "4.0 ×16" (come `pcie_text` di gpu.rs).
fn pcie(gen_: &str, width: &str) -> Option<String> {
    let g = match cella(gen_)?.parse::<u32>().ok()? {
        1 => "1.0",
        2 => "2.0",
        3 => "3.0",
        4 => "4.0",
        5 => "5.0",
        6 => "6.0",
        _ => return None,
    };
    let w: u32 = cella(width)?.parse().ok()?;
    (1..=64).contains(&w).then(|| format!("{g} ×{w}"))
}

/// Legge l'uscita CSV (`noheader,nounits`) di `--query-gpu` ottenuta con `fields`.
pub fn parse_query(text: &str, fields: &[&str]) -> Vec<NvGpu> {
    let pos = |nome: &str| fields.iter().position(|f| *f == nome);
    let mut out = Vec::new();
    for riga in limita(text).lines() {
        if out.len() >= MAX_GPUS {
            break;
        }
        let col: Vec<&str> = riga.split(',').map(str::trim).collect();
        if riga.trim().is_empty() || col.len() != fields.len() {
            continue;
        }
        let get = |nome: &str| pos(nome).and_then(|i| col.get(i).copied()).unwrap_or("");
        let g = NvGpu {
            index: indice(get("index")),
            bus_id: cella(get("pci.bus_id")).and_then(norm_bus_id),
            name: cella(get("name")).and_then(nome_pulito),
            usage: percento(get("utilization.gpu")),
            enc: percento(get("utilization.encoder")),
            dec: percento(get("utilization.decoder")),
            vram_used: mib_in_byte(get("memory.used")),
            vram_total: mib_in_byte(get("memory.total")),
            clock_mhz: mhz(get("clocks.gr")),
            clock_max_mhz: mhz(get("clocks.max.gr")),
            mem_clock_mhz: mhz(get("clocks.mem")),
            temp: numero(get("temperature.gpu"), MIN_TEMP, MAX_TEMP),
            power_w: numero(get("power.draw"), 0.0, MAX_WATT),
            power_cap_w: numero(get("power.limit"), 0.0, MAX_WATT),
            fan_pct: percento(get("fan.speed")),
            pcie: pcie(get("pcie.link.gen.current"), get("pcie.link.width.current")),
            pcie_max: pcie(get("pcie.link.gen.max"), get("pcie.link.width.max")),
        };
        // riga senza alcun dato utile (tutte celle vuote o N/A): scartata
        if g != NvGpu::default() {
            out.push(g);
        }
    }
    out
}

/// Legge l'uscita di `pmon -c 1`. `None` se non è un'uscita di pmon (es. "not supported").
pub fn parse_pmon(text: &str) -> Option<Vec<NvProc>> {
    let mut intest: Option<Vec<&str>> = None;
    let mut procs = Vec::new();
    for riga in limita(text).lines() {
        let t = riga.trim();
        if let Some(c) = t.strip_prefix('#') {
            // La prima riga di commento con "gpu" e "pid" è l'intestazione; le altre (unità) si saltano.
            if intest.is_none() {
                let nomi: Vec<&str> = c.split_whitespace().collect();
                if nomi.contains(&"gpu") && nomi.contains(&"pid") {
                    intest = Some(nomi);
                }
            }
            continue;
        }
        let Some(h) = intest.as_ref() else {
            continue;
        };
        if t.is_empty() || procs.len() >= MAX_PROCS {
            continue;
        }
        let v: Vec<&str> = t.split_whitespace().collect();
        let get = |nome: &str| {
            h.iter()
                .position(|x| *x == nome)
                .and_then(|i| v.get(i).copied())
                .unwrap_or("-")
        };
        let Some(pid) = cella(get("pid")).and_then(|p| p.parse::<u32>().ok()) else {
            continue;
        };
        let kind = match get("type") {
            "G" | "C+G" | "G+C" => WorkKind::Graphics,
            "C" => WorkKind::Compute,
            _ => WorkKind::None,
        };
        procs.push(NvProc {
            gpu_index: indice(get("gpu")),
            bus_id: None,
            pid,
            kind,
            gpu_pct: percento(get("sm")),
            enc_pct: percento(get("enc")),
            dec_pct: percento(get("dec")),
            mem: mib_in_byte(get("fb")),
        });
    }
    intest.map(|_| procs)
}

/// Legge l'uscita CSV di `--query-compute-apps`.
pub fn parse_compute_apps(text: &str) -> Vec<NvProc> {
    let mut out = Vec::new();
    for riga in limita(text).lines() {
        if out.len() >= MAX_PROCS {
            break;
        }
        let col: Vec<&str> = riga.split(',').map(str::trim).collect();
        if col.len() != 3 {
            continue;
        }
        let (Some(bus), Some(pid)) = (
            norm_bus_id(col[0]),
            cella(col[1]).and_then(|p| p.parse().ok()),
        ) else {
            continue;
        };
        out.push(NvProc {
            gpu_index: None,
            bus_id: Some(bus),
            pid,
            kind: WorkKind::Compute,
            mem: mib_in_byte(col[2]),
            ..NvProc::default()
        });
    }
    out
}

/// "00000000:01:00.0" (o già "0000:01:00.0", maiuscole comprese) → "0000:01:00.0".
/// Con dominio > 16 bit tiene il valore senza zeri iniziali in eccesso ("00010000:E1:00.0" → "10000:e1:00.0").
pub fn norm_bus_id(s: &str) -> Option<String> {
    let s = s.trim();
    if !s.is_ascii() {
        return None;
    }
    let p: Vec<&str> = s.split(':').collect();
    if p.len() != 3 {
        return None;
    }
    let dom = match p[0].len() {
        4 => p[0],
        5..=7 if !p[0].starts_with('0') => p[0],
        8 if p[0].starts_with("0000") => &p[0][4..],
        8 => p[0].trim_start_matches('0'),
        _ => return None,
    };
    let (dev, func) = p[2].split_once('.')?;
    let ok = |x: &str, n: usize| x.len() == n && x.bytes().all(|b| b.is_ascii_hexdigit());
    if !((4..=8).contains(&dom.len())
        && dom.bytes().all(|b| b.is_ascii_hexdigit())
        && ok(p[1], 2)
        && ok(dev, 2)
        && ok(func, 1))
    {
        return None;
    }
    Some(format!("{dom}:{}:{dev}.{func}", p[1]).to_ascii_lowercase())
}

/// `true` se stdout/stderr di `nvidia-smi` dicono che un campo della query non è valido.
pub fn is_unknown_field_error(out: &str) -> bool {
    let o = limita(out).to_ascii_lowercase();
    o.contains("is not a valid field to query")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fix(nome: &str) -> String {
        let p = format!(
            "{}/tests/fixtures/nvidia/{nome}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(p).unwrap()
    }

    const GIB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn argomenti() {
        assert_eq!(
            query_args(&["index", "name"]),
            ["--query-gpu=index,name", "--format=csv,noheader,nounits"]
        );
        assert_eq!(pmon_args(), ["pmon", "-c", "1", "-s", "um"]);
        assert_eq!(
            compute_apps_args(),
            [
                "--query-compute-apps=gpu_bus_id,pid,used_memory",
                "--format=csv,noheader,nounits"
            ]
        );
    }

    #[test]
    fn query_completa_3090() {
        let v = parse_query(&fix("query_full_3090.txt"), FIELDS_FULL);
        assert_eq!(v.len(), 1);
        let g = &v[0];
        assert_eq!(g.index, Some(0));
        assert_eq!(g.bus_id.as_deref(), Some("0000:01:00.0"));
        assert_eq!(g.name.as_deref(), Some("NVIDIA GeForce RTX 3090"));
        assert_eq!(g.usage, Some(37.0));
        assert_eq!(g.enc, Some(0.0));
        assert_eq!(g.vram_used, Some(1523 * 1024 * 1024));
        assert_eq!(g.vram_total, Some(24 * GIB));
        assert_eq!(g.clock_mhz, Some(1695));
        assert_eq!(g.clock_max_mhz, Some(2100));
        assert_eq!(g.mem_clock_mhz, Some(9751));
        assert_eq!(g.temp, Some(54.0));
        assert_eq!(g.power_w, Some(112.45));
        assert_eq!(g.power_cap_w, Some(350.0));
        assert_eq!(g.fan_pct, Some(41.0));
        assert_eq!(g.pcie.as_deref(), Some("4.0 ×16"));
        assert_eq!(g.pcie_max.as_deref(), Some("4.0 ×16"));
    }

    #[test]
    fn portatile_senza_ventola() {
        let v = parse_query(&fix("query_laptop_4060.txt"), FIELDS_FULL);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].fan_pct, None);
        assert_eq!(v[0].pcie.as_deref(), Some("1.0 ×8"));
        assert_eq!(v[0].pcie_max.as_deref(), Some("4.0 ×8"));
        assert_eq!(v[0].vram_total, Some(8188 * 1024 * 1024));
        assert!(v[0].name.as_deref().unwrap().contains("Laptop"));
    }

    #[test]
    fn due_schede() {
        let v = parse_query(&fix("query_two_gpus.txt"), FIELDS_FULL);
        assert_eq!(v.len(), 2);
        assert_eq!(v[1].index, Some(1));
        assert_eq!(v[1].bus_id.as_deref(), Some("0000:08:00.0"));
        assert_eq!(v[0].usage, Some(90.0));
        assert_eq!(v[1].fan_pct, Some(0.0));
    }

    #[test]
    fn ripiego_base_driver_vecchio() {
        let t = fix("query_base_old_driver.txt");
        let v = parse_query(&t, FIELDS_BASE);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].name.as_deref(), Some("Quadro P2000"));
        assert_eq!(v[0].enc, None);
        assert_eq!(v[0].dec, None);
        assert_eq!(v[0].usage, Some(11.0));
        assert_eq!(v[0].fan_pct, None);
        assert_eq!(v[0].pcie.as_deref(), Some("3.0 ×16"));
        // con l'elenco sbagliato le colonne non coincidono: riga scartata
        assert!(parse_query(&t, FIELDS_FULL).is_empty());
    }

    #[test]
    fn errore_campo_non_valido() {
        let e = fix("query_old_driver_stderr.txt");
        assert!(is_unknown_field_error(&e));
        assert!(is_unknown_field_error(
            "Field \"utilization.decoder\" is not a valid field to query."
        ));
        assert!(!is_unknown_field_error(""));
        assert!(!is_unknown_field_error(
            "NVIDIA-SMI has failed because it couldn't communicate with the NVIDIA driver."
        ));
        assert!(!is_unknown_field_error("No devices were found"));
        assert!(parse_query(&e, FIELDS_FULL).is_empty());
    }

    #[test]
    fn valori_non_supportati() {
        let v = parse_query(&fix("query_not_supported.txt"), FIELDS_FULL);
        assert_eq!(v.len(), 1);
        let g = &v[0];
        assert_eq!(g.usage, None);
        assert_eq!(g.enc, None);
        assert_eq!(g.dec, None);
        assert_eq!(g.power_w, None);
        assert_eq!(g.power_cap_w, None);
        assert_eq!(g.fan_pct, None);
        assert_eq!(g.vram_used, Some(200 * 1024 * 1024));
        assert_eq!(g.temp, Some(40.0));
    }

    #[test]
    fn query_troncata_e_rovinata() {
        let v = parse_query(&fix("query_truncated.txt"), FIELDS_FULL);
        assert_eq!(v.len(), 1);
        let v = parse_query(&fix("query_garbled.txt"), FIELDS_FULL);
        assert_eq!(v.len(), 1);
        let g = &v[0];
        assert_eq!(g.bus_id, None);
        assert_eq!(g.name.as_deref(), Some("[31mEvilName"));
        assert_eq!(g.usage, None);
        assert_eq!(g.enc, None);
        assert_eq!(g.dec, None);
        assert_eq!(g.vram_used, None);
        assert_eq!(g.vram_total, Some(5 * 1024 * 1024));
        assert_eq!(g.clock_mhz, None);
        assert_eq!(g.clock_max_mhz, None);
        assert_eq!(g.temp, None);
        assert_eq!(g.power_w, None);
        assert_eq!(g.fan_pct, None);
        assert_eq!(g.pcie.as_deref(), Some("3.0 ×9"));
        assert!(parse_query("", FIELDS_FULL).is_empty());
    }

    #[test]
    fn limiti_di_schede_e_nome() {
        let riga = format!("0, 00000000:01:00.0, {}, 1\n", "x".repeat(500));
        let f = &["index", "pci.bus_id", "name", "utilization.gpu"];
        let v = parse_query(&riga, f);
        assert_eq!(v[0].name.as_ref().unwrap().chars().count(), MAX_NAME);
        let tante = "0, 0000:01:00.0, a, 1\n".repeat(MAX_GPUS + 20);
        assert_eq!(parse_query(&tante, f).len(), MAX_GPUS);
    }

    #[test]
    fn pmon_misto() {
        let v = parse_pmon(&fix("pmon_mixed.txt")).unwrap();
        assert_eq!(v.len(), 3);
        assert_eq!(v[0].pid, 1234);
        assert_eq!(v[0].kind, WorkKind::Graphics);
        assert_eq!(v[0].gpu_pct, Some(12.0));
        assert_eq!(v[0].enc_pct, Some(0.0));
        assert_eq!(v[0].mem, Some(512 * 1024 * 1024));
        assert_eq!(v[0].gpu_index, Some(0));
        assert_eq!(v[1].kind, WorkKind::Graphics);
        assert_eq!(v[1].enc_pct, None);
        assert_eq!(v[2].kind, WorkKind::Compute);
        assert_eq!(v[2].mem, Some(4 * GIB));
    }

    #[test]
    fn pmon_driver_nuovo() {
        let v = parse_pmon(&fix("pmon_new_jpg_ofa.txt")).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].enc_pct, Some(1.0));
        assert_eq!(v[0].dec_pct, Some(2.0));
        assert_eq!(v[0].mem, Some(700 * 1024 * 1024));
        assert_eq!(v[1].gpu_index, Some(1));
        assert_eq!(v[1].kind, WorkKind::Compute);
        assert_eq!(v[1].mem, Some(10 * GIB));
    }

    #[test]
    fn pmon_non_supportato_o_vuoto() {
        assert_eq!(parse_pmon(&fix("pmon_not_supported.txt")), None);
        assert_eq!(parse_pmon(""), None);
        assert_eq!(parse_pmon("0 1 C 5 5 5 5 5\n"), None);
    }

    #[test]
    fn pmon_rovinato() {
        let v = parse_pmon(&fix("pmon_garbled.txt")).unwrap();
        // "99999999999" non è un pid u32; "77" ha colonne mancanti
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].pid, 12);
        assert_eq!(v[0].gpu_index, None);
        assert_eq!(v[0].kind, WorkKind::None);
        assert_eq!(v[0].gpu_pct, None);
        assert_eq!(v[0].enc_pct, None);
        assert_eq!(v[0].mem, None);
        assert_eq!(v[1].pid, 77);
        assert_eq!(v[1].kind, WorkKind::Compute);
        assert_eq!(v[1].mem, None);
    }

    #[test]
    fn pmon_solo_intestazione() {
        let t = "# gpu pid type sm mem enc dec fb command\n# Idx # C/G % % % % MB name\n";
        assert_eq!(parse_pmon(t), Some(Vec::new()));
    }

    #[test]
    fn compute_apps() {
        let v = parse_compute_apps(&fix("compute_apps.txt"));
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].bus_id.as_deref(), Some("0000:01:00.0"));
        assert_eq!(v[0].pid, 3456);
        assert_eq!(v[0].kind, WorkKind::Compute);
        assert_eq!(v[0].mem, Some(4 * GIB));
        assert_eq!(v[1].pid, 7788);
        assert!(parse_compute_apps(&fix("compute_apps_none.txt")).is_empty());
        assert!(parse_compute_apps("00000000:01:00.0, abc, 5\nx,y\n, , ,\n").is_empty());
    }

    #[test]
    fn bus_id_normalizzato() {
        assert_eq!(
            norm_bus_id("00000000:01:00.0").as_deref(),
            Some("0000:01:00.0")
        );
        assert_eq!(
            norm_bus_id("00000001:01:00.0").as_deref(),
            Some("0001:01:00.0")
        );
        assert_eq!(norm_bus_id("0000:0A:00.1").as_deref(), Some("0000:0a:00.1"));
        assert_eq!(
            norm_bus_id(" 00000000:AB:1f.0 ").as_deref(),
            Some("0000:ab:1f.0")
        );
        assert_eq!(
            norm_bus_id("00010000:E1:00.0").as_deref(),
            Some("10000:e1:00.0")
        );
        assert_eq!(
            norm_bus_id("10000:E1:00.0").as_deref(),
            Some("10000:e1:00.0")
        );
        assert_eq!(
            norm_bus_id("12345678:01:00.0").as_deref(),
            Some("12345678:01:00.0")
        );
        for s in [
            "",
            "01:00.0",
            "0000:01:00",
            "0000:zz:00.0",
            "00000:01:00.0",
            "000g0000:01:00.0",
            "0000:01:00.00",
            "0000:01:00:0",
            "è000:01:00.0",
        ] {
            assert_eq!(norm_bus_id(s), None, "{s}");
        }
    }

    // --- fuzz deterministico ---

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            // xorshift64*
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next() % n.max(1) as u64) as usize
        }
    }

    fn pct_ok(v: Option<f64>) -> bool {
        v.is_none_or(|x| x.is_finite() && (0.0..=100.0).contains(&x))
    }

    fn controlla_gpu(g: &NvGpu) {
        for p in [g.usage, g.enc, g.dec, g.fan_pct] {
            assert!(pct_ok(p), "{g:?}");
        }
        assert!(g.temp.is_none_or(|t| (-50.0..=200.0).contains(&t)));
        for w in [g.power_w, g.power_cap_w] {
            assert!(w.is_none_or(|x| (0.0..=5000.0).contains(&x)));
        }
        for c in [g.clock_mhz, g.clock_max_mhz, g.mem_clock_mhz] {
            assert!(c.is_none_or(|x| x <= 100_000));
        }
        for m in [g.vram_used, g.vram_total] {
            assert!(m.is_none_or(|x| x <= (MAX_MIB as u64) * (MIB as u64)));
        }
        if let Some(n) = &g.name {
            assert!(!n.is_empty() && n.chars().count() <= MAX_NAME);
            assert!(!n.chars().any(|c| c.is_control()));
        }
        if let Some(b) = &g.bus_id {
            assert_eq!(norm_bus_id(b).as_deref(), Some(b.as_str()));
        }
    }

    fn controlla_proc(p: &NvProc) {
        for v in [p.gpu_pct, p.enc_pct, p.dec_pct] {
            assert!(pct_ok(v), "{p:?}");
        }
        assert!(p.mem.is_none_or(|x| x <= (MAX_MIB as u64) * (MIB as u64)));
    }

    fn prova_tutto(t: &str) {
        for f in [FIELDS_FULL, FIELDS_BASE] {
            let v = parse_query(t, f);
            assert!(v.len() <= MAX_GPUS);
            v.iter().for_each(controlla_gpu);
        }
        if let Some(v) = parse_pmon(t) {
            assert!(v.len() <= MAX_PROCS);
            v.iter().for_each(controlla_proc);
        }
        let v = parse_compute_apps(t);
        assert!(v.len() <= MAX_PROCS);
        v.iter().for_each(controlla_proc);
        let _ = is_unknown_field_error(t);
        let _ = norm_bus_id(t);
    }

    #[test]
    fn fuzz_byte_casuali() {
        let mut r = Rng(0x1234_5678_9abc_def1);
        for _ in 0..2000 {
            let n = r.below(600);
            let b: Vec<u8> = (0..n).map(|_| r.next() as u8).collect();
            prova_tutto(&String::from_utf8_lossy(&b));
        }
    }

    #[test]
    fn fuzz_righe_casuali() {
        let pezzi = [
            "0",
            "1",
            "-",
            "N/A",
            "[N/A]",
            "[Not Supported]",
            "99999999999999999999",
            "-7",
            "1e400",
            "NaN",
            "inf",
            "C",
            "G",
            "C+G",
            "00000000:01:00.0",
            "# gpu pid type sm enc dec fb",
            "# Idx # C/G % % % MB",
            "NVIDIA X",
            "\u{1b}[0m",
            "é",
            " ",
            "",
            "4",
            "16",
            "512",
            "300.5",
        ];
        let mut r = Rng(0xdead_beef_cafe_f00d);
        for _ in 0..2000 {
            let mut s = String::new();
            for _ in 0..r.below(8) {
                for k in 0..r.below(22) {
                    if k > 0 {
                        s.push_str(if r.below(3) == 0 { " " } else { ", " });
                    }
                    s.push_str(pezzi[r.below(pezzi.len())]);
                }
                s.push('\n');
            }
            prova_tutto(&s);
        }
    }

    #[test]
    fn fuzz_mutazioni_fixture() {
        let nomi = [
            "query_full_3090.txt",
            "query_laptop_4060.txt",
            "query_two_gpus.txt",
            "query_base_old_driver.txt",
            "query_not_supported.txt",
            "query_garbled.txt",
            "pmon_mixed.txt",
            "pmon_new_jpg_ofa.txt",
            "pmon_garbled.txt",
            "compute_apps.txt",
        ];
        let mut r = Rng(42);
        for nome in nomi {
            let orig = fix(nome).into_bytes();
            prova_tutto(&String::from_utf8_lossy(&orig));
            for _ in 0..300 {
                let mut b = orig.clone();
                for _ in 0..1 + r.below(6) {
                    if b.is_empty() {
                        break;
                    }
                    let i = r.below(b.len());
                    match r.below(3) {
                        0 => b[i] = r.next() as u8,
                        1 => b.truncate(i),
                        _ => {
                            b.remove(i);
                        }
                    }
                }
                prova_tutto(&String::from_utf8_lossy(&b));
            }
        }
    }

    #[test]
    fn uscita_enorme_limitata() {
        let riga = "0, 0000:01:00.0, a, 1\n";
        let t = riga.repeat(MAX_OUTPUT / riga.len() * 3);
        let f = &["index", "pci.bus_id", "name", "utilization.gpu"];
        assert_eq!(parse_query(&t, f).len(), MAX_GPUS);
        // Dati validi solo dopo MAX_OUTPUT byte di riempimento (300 KB): scartati dal taglio.
        let spazi = " ".repeat(300 * 1024);
        let pmon = format!(
            "{spazi}\n# gpu pid type sm mem enc dec fb command\n0 1234 C 10 5 0 0 100 python\n"
        );
        assert_eq!(parse_pmon(&pmon), None);
        let vuote = "\n".repeat(300 * 1024);
        assert!(parse_query(&format!("{vuote}{riga}"), f).is_empty());
        assert!(parse_compute_apps(&format!("{vuote}00000000:01:00.0, 1234, 512\n")).is_empty());
        let t = "é".repeat(MAX_OUTPUT);
        prova_tutto(&t);
    }
}
