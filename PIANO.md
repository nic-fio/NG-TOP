# PIANO — ngtop 1.6: azioni sulle macchine remote (fase 2)

(Riepiloghi delle versioni precedenti in fondo.)

## Obiettivo
Sulla macchina remota si può fare quello che oggi si fa in locale:
- inviare segnali ai processi (anche a un sottoalbero);
- cambiare il nice;
- avviare, fermare, riavviare, abilitare e disabilitare i servizi.

Con `--sudo` tutto si fa come root sul server. Tutto si vede come root, anche i dati dei processi di altri utenti
(file aperti, I/O). Le conferme e le protezioni restano quelle di oggi.

## Come funziona (in parole semplici)
- **Segnali e nice**: ngtop chiede allo script già attivo sul server di agire su un processo e manda insieme l'ora di
  avvio che conosce. Lo script rilegge l'ora di avvio del processo e agisce solo se coincide. Così un pid riusato da un
  altro programma non viene mai colpito. Il controllo e il `kill` (o `renice`) avvengono in un unico comando sul server.
- **Servizi**: `systemctl --no-ask-password start|stop|restart|enable|disable -- <unità>` attraverso la stessa connessione.
  Senza permessi systemd risponde con un errore, e ngtop lo traduce in "needs root: start ngtop with --sudo".
- **`--sudo`**: lo script sul server parte con `sudo` invece che come utente normale.
  1. Prima si prova senza password (`sudo -n`), che funziona se il server lo permette (NOPASSWD).
  2. Altrimenti ngtop chiede la password sudo sul terminale, prima di aprire l'interfaccia, senza mostrarla.
     La manda una sola volta e aspetta la conferma di sudo prima di mandare lo script.
  3. Se la password è sbagliata, ngtop chiude e dice "sudo: wrong password". Lo script non viene mai letto come password.
- **Riconnessione con `--sudo`**: la password resta solo nella memoria di ngtop, per riconnettersi da solo. Si cancella
  all'uscita. Non si scrive mai su disco né nella configurazione. → **Da decidere con te** (vedi sotto).
- **Protezioni**: PID 1 e lo script di ngtop sul server sono protetti, come oggi PID 1 e ngtop in locale. Le finestre di
  conferma nominano la macchina ("Send SIGTERM to 1234 on alex@server?"). La barra in alto mostra "root" quando
  `--sudo` è attivo.
- Le unità dell'utente (`--user`) restano non disponibili in remoto: ssh non raggiunge quel bus.

## Lavoro in parallelo (5 programmatori Sonnet)
**Prima di tutto, io (Opus).** Scrivo l'interfaccia comune, che è delicata perché tocca la sicurezza:
- un'interfaccia `ProcActions`, con le azioni "segnale" e "nice", entrambe verificate con l'ora di avvio, più l'elenco
  dei pid protetti;
- la versione locale, cioè il codice di oggi spostato lì;
- il campo in `Roots`.

Scrivo anche il contratto con i nomi concordati. Poi partono in parallelo, ognuno sui propri file:

| # | Chi | File | Compito |
|---|-----|------|---------|
| P1 | trasporto | `src/remote/**` | Nuove richieste nello script del server: segnale e nice con verifica dell'ora di avvio, pid dello script stesso. `--sudo`: avvio con sudo, password, errori. Versione remota di `ProcActions`. |
| P2 | servizi | `src/collect/services/**` | Azioni sui servizi remoti con `systemctl`; controllo dei nomi delle unità (già c'è); errori leggibili ("needs root…", "unit not found"); aggiornamento dello stato dopo l'azione. |
| P3 | interfaccia | `src/ui/**`, `src/main.rs`, `src/config.rs`, README | Tolgo i blocchi "Not available on remote machines". Le azioni passano da `ProcActions`. Conferme con il nome della macchina, scritta "root" in alto. Opzione `--sudo` e chiave `sudo` nella configurazione. Richiesta della password prima dell'interfaccia. Aiuto F1. |
| P4 | prove | `tests/remote.rs`, `scripts/test-remote.sh`, `tests/fixtures/remote/**` | Container con sshd e sudo: un utente con NOPASSWD, uno con password. Container con systemd e un servizio di prova per avvio e arresto. Prove su pid riusato, password sbagliata, PID 1 protetto e servizi senza permessi. |
| P5 | manuali e sito | `docs/sources/**`, `docs/decisions-and-history.md`, `site/landing/**` | Manuale utente (capitolo remoto: azioni, `--sudo`, sicurezza), manuale tecnico (protocollo delle azioni, handshake sudo), decisioni, sito "New in 1.6". |

Alla fine: revisore (diff completo) → correzioni agli stessi programmatori → revisore → rilascio **1.6.0**: sito, manuali,
release GitHub, repository sincronizzati.

## Rischi
- **Il controllo dell'ora di avvio non è perfetto in remoto.** In locale ngtop "aggancia" il processo (pidfd) e la
  finestra di rischio è zero. Sul server lo script controlla e poi esegue `kill` subito dopo, quindi resta una finestra
  di pochi millisecondi. Perché il pid venga riusato proprio lì servono migliaia di processi nuovi in quell'istante.
  Lo scrivo nel manuale. Un `kill` con pidfd non si può usare sul server, perché non c'è ovunque (per esempio su
  busybox).
- **sudo senza terminale**: alcuni server vecchi hanno `requiretty` in sudoers. In quel caso ngtop lo spiega e
  suggerisce NOPASSWD o un utente root.
- **Una password sbagliata non deve mai finire nei log del server come testo dello script.** Per questo ngtop aspetta
  la conferma di sudo prima di mandare lo script; va provato apposta.
- **Azioni vere sui tuoi server**: le prove automatiche si fanno solo nei container. Sul VPS e sul server di casa provo
  solo la lettura e un segnale a un processo di prova che lancio io (`sleep`), mai su servizi reali.
- **Prove dei servizi**: richiedono un container con systemd attivo. Se podman non lo consente su questo PC, le provo
  con un servizio finto sul server di casa, ma solo dopo averti chiesto il permesso.

## Da decidere con te
1. **Password sudo e riconnessione**. Consiglio (a): la password resta in memoria solo finché ngtop è aperto, così dopo
   un calo di rete si riconnette da solo. In alternativa (b): non la tiene, e dopo un calo di rete chiede di riavviare
   ngtop.
2. **Versione**: 1.6.0. Consiglio questa, perché è una funzione nuova e non cambia il modo d'uso.

## Criteri di completamento
- Verso un container, il VPS e il server di casa: segnale e nice funzionano su un processo di prova. Un pid riusato non
  viene colpito. PID 1 chiede la doppia conferma come in locale.
- Nel container con systemd: avvio, arresto, riavvio, abilitazione e disabilitazione di un servizio di prova, con
  `--sudo` sia senza password sia con password. Senza `--sudo` compare il messaggio "needs root…".
- Password sbagliata → errore chiaro, nessuna riga dello script nei log del server.
- In locale non cambia niente: tutte le prove di oggi verdi, clippy pulito, binario statico.
- Manuali allineati (`build.py --check`), sito e release 1.6.0 pubblicati, repository sincronizzati.

---
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
