# PIANO

## Riepilogo — ngtop 1.5.0 (23/10/2026)
Monitoraggio di macchine remote via ssh, in sola lettura e senza agenti: `ngtop [user@]host`, `--host`, `--dump host`.
Una sessione `ssh` persistente esegue `sh` con uno script inviato su stdin (niente file sul server); una richiesta per
aggiornamento, record delimitati da un nonce, ~2-3 KB per aggiornamento, CPU remota 0,4-0,7 %. Riconnessione automatica,
mount di rete esclusi, servizi via systemctl/journalctl in JSON. Le azioni (segnali, nice, servizi) sono disattivate in remoto.
229 test; manuali con capitoli "Remote machines" (utente) e sul protocollo (tecnico); sito con sezione "New in 1.5".
Da fare: fase 2 (azioni remote con controllo dell'ora di avvio, `--sudo`), vista di più host insieme.

## Riepilogo — ngtop 1.0.0 (23/10/2026)
Rust + ratatui, binario statico musl (~3.5 MB). Cinque sezioni, interfaccia e manuali in inglese, licenza PolyForm
Noncommercial 1.0.0. Sito https://ngtop.nicfio.it (`site/publish.sh`), repository pubblico
https://github.com/nic-fio/NG-TOP, rilascio v1.0.0. 175 test, 8 distribuzioni, ~1 % CPU, ~5.6 MB RAM.
Da fare: NVIDIA (NVML), cronologia registrata, riga singola per tmux, watt CPU (RAPL); prova su RX 6800 reale e Intel xe;
F3/F4 nella sezione Services aprono il filtro globale.
