# Riepilogo — ngtop 0.1.0 (23/10/2026)

Monitor di sistema da terminale (btop + nvtop + Gestore attività), Rust + ratatui, binario statico musl
(`./build.sh` → `dist/ngtop`, ~3.5 MB, nessun percorso locale dentro). Interfaccia in inglese, cinque sezioni
(Overview, GPU, Processes, Network & Disks, Services), licenza PolyForm Noncommercial 1.0.0 (NOTICE.md,
THIRD-PARTY-LICENSES da `scripts/third-party.py`, contatto ngtop@nicfio.it).

- Manuali in `docs/` (canone dei manuali, modello NESH/REMOTIX): `python3 docs/sources/build.py [--check]`;
  figure dalle schermate vere su `tests/fixtures/demo`; tasti e opzioni da `ngtop --doc-json`.
- Sito https://ngtop.nicfio.it (pagina «Daylight», manuali, licenza, decisioni, download con SHA256SUMS):
  `site/publish.sh` (ricezione sicura in cartella temporanea sul VPS).
- Verifiche: 175 test, clippy pulito, 8 distribuzioni (`scripts/test-distro.sh`), ~1 % CPU, ~5.6 MB RAM.
- Repository pubblico: https://github.com/nic-fio/NG-TOP.

Da fare in futuro: NVIDIA (NVML), cronologia registrata (mockup 10), riga singola per tmux, watt CPU (RAPL);
prova su RX 6800 reale e su Intel xe; F3/F4 nella sezione Services aprono il filtro globale (da rivedere).
