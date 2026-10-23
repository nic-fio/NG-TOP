# ngtop

Monitor di sistema da terminale (btop + nvtop + Gestore attività di Windows), distribuito come un unico eseguibile statico per Linux x86_64.

## Struttura
- `src/collect/` — "raccoglitori": leggono /proc, /sys, D-Bus e calcolano i dati. Nessuna dipendenza dall'interfaccia; testati con file finti in `tests/fixtures/`.
- `src/ui/` — "viste" ratatui: una per sezione (panoramica, gpu, processi, rete_dischi, servizi) + componenti comuni.
- `mockup/` — mockup HTML dell'interfaccia; `mockup/app.html` è il riferimento visivo.
- `PIANO.md` — piano in corso.

## Comandi
- Build statica: `./build.sh` → `dist/ngtop` (target x86_64-unknown-linux-musl, verifica con ldd).
- Test: `cargo test` · Lint: `cargo clippy --all-targets -- -D warnings` · Formato: `cargo fmt`.
- Rust è in `~/.cargo/bin` (aggiungerlo al PATH se manca).

## Convenzioni
- Solo dipendenze in Rust puro (niente crate che compilano codice C): l'eseguibile deve restare statico.
- I raccoglitori non devono mai andare in panic: dati mancanti o permessi negati → `None`/"—".
- Testi dell'interfaccia in inglese (schermate, tasti, aiuto F1, messaggi, `--help`, errori); commenti brevi in italiano.
- Licenza PolyForm Noncommercial 1.0.0 (`LICENSE.md`): in testa a ogni `.rs` e script le due righe `Copyright (c) 2026 Nicola Fiorillo` e `SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0`. `THIRD-PARTY-LICENSES` si rigenera con `scripts/third-party.py`.
- Percorsi di /proc e /sys sempre passati come radice configurabile (per i test).
