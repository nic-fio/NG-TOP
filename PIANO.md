# PIANO — ngtop 1.1: monitorare macchine remote via ssh, senza agenti

(Riepilogo della 1.0.0 in fondo.)

## Obiettivo
`ngtop user@host` mostra la macchina remota esattamente come oggi mostra quella locale, leggendo il suo /proc e /sys
attraverso ssh. Sulla macchina remota non si installa niente e non resta niente: basta un server ssh e una shell
POSIX con `cat` (c'è su qualsiasi Linux, anche Alpine/busybox).

## Come funziona (in parole semplici)
- ngtop avvia il comando `ssh` già presente sul PC (usa le tue chiavi, ~/.ssh/config, ProxyJump, known_hosts; se serve
  chiede la password PRIMA di aprire l'interfaccia) e tiene aperta **una sola connessione** per tutta la sessione.
- Attraverso quella connessione invia un piccolo script di shell (solo in memoria, mai scritto su disco remoto) che resta
  in ascolto: a ogni aggiornamento ngtop chiede in un'unica richiesta tutti i file che gli servono e riceve un unico
  pacchetto con i contenuti. Una sola andata e ritorno per aggiornamento.
- Misure di oggi: ~60 KB per aggiornamento (~17 KB compressi con `ssh -C`); aprire una connessione nuova costerebbe
  ~0,6 s, per questo la connessione resta aperta.
- Dentro ngtop cambia solo il "punto di lettura": oggi tutte le letture passano da `Roots`; diventerà una sorgente
  intercambiabile, **locale** (come ora) o **remota** (il pacchetto ricevuto via ssh). Tutto il resto — calcoli, grafici,
  sezioni — resta identico.

## Fase 1 — monitoraggio remoto (sola lettura)
1. **Sorgente astratta**: tutte le letture dei raccoglitori (file, cartelle, link, proprietario, spazio dei filesystem)
   passano da un'interfaccia unica con due implementazioni: locale e "istantanea" (contenuti già ricevuti).
2. **Protocollo ssh**: processo `ssh -T -C host sh` persistente; script remoto che legge richieste (elenchi di percorsi e
   modelli come `/proc/[0-9]*/stat`) e risponde con blocchi delimitati (percorso, lunghezza, contenuto; link e cartelle
   inclusi); `df -P -k` per lo spazio dei filesystem; `getconf` per tick e pagina; ora remota da /etc/localtime remoto.
   Le cache attuali (cmdline, utente, fd ogni 5/20 giri) riducono i dati.
3. **Tutte e cinque le sezioni** funzionano: Overview, GPU (fdinfo remoti), Processes (dettaglio letto solo per il
   processo selezionato), Network & Disks (connessioni solo con la sezione visibile), Services (in sola lettura via
   `systemctl … --output=json`, `journalctl -o json`, `loginctl`, `systemd-analyze` remoti).
4. **Interfaccia**: nome dell'host e latenza nella barra in alto; connessione persa → avviso e riconnessione
   automatica; `ngtop --dump user@host` per il riepilogo testuale; avvio `ngtop user@host` o `ngtop --host user@host`
   (anche nel file di configurazione).
5. In remoto le **azioni sono disattivate** (segnali, nice, avvio/arresto servizi): si vedono, ma la finestra spiega che
   arriveranno nella fase 2.

## Fase 2 — azioni remote (dopo, se la fase 1 va bene)
Segnali e nice con la stessa sicurezza di oggi: prima di inviare si ricontrolla sul server l'ora di avvio del processo
nello stesso comando (`kill` solo se coincide); avvio/arresto servizi con `systemctl`; opzione `--sudo` per vedere e fare
tutto come root (richiede sudo senza password sul server, oppure lo chiede all'avvio).

## Rischi
- **Velocità su reti lente**: con molti processi e collegamento lento l'aggiornamento può superare 1 s → ngtop allarga
  da solo l'intervallo e lo dice; i dati per processo pesanti (I/O, fd) restano a cadenza ridotta.
- **Server minimali** (busybox, niente systemd, niente `df`): ciò che manca diventa "—", come oggi in locale.
- **Carico sul server**: lo script usa `cat` e poco altro; va misurato (obiettivo: sotto 1 % di CPU sul server).
- **Sicurezza**: nessun file lasciato sul server, nessuna porta aperta; tutto passa dalla connessione ssh dell'utente.
  Il protocollo deve resistere a nomi di file e contenuti strani (delimitatori con lunghezza esplicita).
- **Il binario resta statico**: si usa il comando `ssh` del sistema, non una libreria ssh (le librerie ssh in Rust
  richiedono codice crittografico in C).

## Criteri di completamento
- `ngtop user@host` funziona verso il tuo VPS e verso il server 192.168.1.2 (e verso un container con sshd), con tutte
  le sezioni popolate e numeri coerenti con un ngtop lanciato lì in locale.
- Test automatici: lo stesso protocollo provato in locale usando `sh` al posto di `ssh` (nessuna rete necessaria);
  fixture lette come "remote" danno gli stessi risultati della lettura diretta.
- Misure: dati per aggiornamento, latenza, CPU di ngtop e CPU sul server, riportate nel manuale tecnico.
- Manuali aggiornati (nuovo capitolo "Remote machines" nel manuale utente e capitolo tecnico sul protocollo), sito
  aggiornato, rilascio 1.1.0.

---
## Riepilogo — ngtop 1.0.0 (23/10/2026)
Rust + ratatui, binario statico musl (~3.5 MB). Cinque sezioni, interfaccia e manuali in inglese, licenza PolyForm
Noncommercial 1.0.0. Sito https://ngtop.nicfio.it (`site/publish.sh`), repository pubblico
https://github.com/nic-fio/NG-TOP, rilascio v1.0.0. 175 test, 8 distribuzioni, ~1 % CPU, ~5.6 MB RAM.
Da fare: NVIDIA (NVML), cronologia registrata, riga singola per tmux, watt CPU (RAPL); prova su RX 6800 reale e Intel xe;
F3/F4 nella sezione Services aprono il filtro globale.
