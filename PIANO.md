# PIANO

## Riepilogo — ngtop 1.6.0 (23/10/2026)
Azioni sulle macchine remote:
- segnali e nice, con l'ora di avvio verificata sul server nella stessa chiamata;
- azioni sui servizi via `systemctl --no-ask-password`;
- `--sudo` su una sola connessione: prompt casuale e password inviata solo dopo averlo visto, tenuta in memoria per le
  riconnessioni, processo non-dumpable.
Conferme con il nome della macchina e la scritta "root" in alto. Le azioni preparate prima di una riconnessione vengono
rifiutate e i testi che arrivano dal server sono ripuliti.
Hardening:
- tre giri di revisione di sicurezza con sabotaggio (ogni correzione è protetta da un test);
- `tests/hostile.rs`: server ostile, fuzz, sudo finti, cadute;
- `scripts/test-remote.sh --actions`: 32 casi nei container.
Suite di sicurezza obbligatoria prima di ogni rilascio (vedi CLAUDE.md).
Da fare: vista di più host insieme; unità `--user` in remoto; mlock della password.

## Riepilogo — ngtop 1.5.0 (23/10/2026)
Monitoraggio di macchine remote via ssh, in sola lettura e senza agenti: `ngtop [user@]host`, `--host`, `--dump host`.
Una sessione `ssh` persistente esegue `sh` con uno script inviato su stdin (niente file sul server); una richiesta per
aggiornamento, record delimitati da un nonce, ~2-3 KB per aggiornamento, CPU remota 0,4-0,7 %. Riconnessione automatica,
mount di rete esclusi, servizi via systemctl/journalctl in JSON. Le azioni (segnali, nice, servizi) sono disattivate in remoto.
229 test; manuali con capitoli "Remote machines" (utente) e sul protocollo (tecnico); sito con sezione "New in 1.5".

## Riepilogo — ngtop 1.0.0 (23/10/2026)
Rust + ratatui, binario statico musl (~3.5 MB). Cinque sezioni, interfaccia e manuali in inglese, licenza PolyForm
Noncommercial 1.0.0. Sito https://ngtop.nicfio.it (`site/publish.sh`), repository pubblico
https://github.com/nic-fio/NG-TOP, rilascio v1.0.0. 175 test, 8 distribuzioni, ~1 % CPU, ~5.6 MB RAM.
Da fare: NVIDIA (NVML), cronologia registrata, riga singola per tmux, watt CPU (RAPL); prova su RX 6800 reale e Intel xe;
F3/F4 nella sezione Services aprono il filtro globale.
