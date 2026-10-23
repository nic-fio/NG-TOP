#!/bin/bash
# Copyright (c) 2026 Nicola Fiorillo
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Prove end-to-end del monitoraggio remoto (`ngtop TARGET`) contro container con sshd (podman rootless).
# Uso: scripts/test-remote.sh [--measure] [cartella-log]
#   (senza opzioni)  prove funzionali + tabella OK/ERRORE
#   --measure        misure (byte, latenza, CPU server/locale) con intervallo 1 s, 30 s per container
#   --actions        prove delle azioni remote (segnali, nice, servizi, --sudo con/senza password);
#                    usa tests/remote.rs (cargo test, #[ignore]) e `--dump --sudo` sotto pty. Mai su macchine vere.
# Variabili: NGTOP_BIN (default dist/ngtop), MEASURE_SECS (default 30), CONTAINERS="alpine debian systemd".
# Chiave e ssh_config stanno in una cartella temporanea; ~/.ssh non viene mai toccato.
set -u
cd "$(dirname "$0")/.."
ROOT="$PWD"
MODE=test
[ "${1:-}" = "--measure" ] && { MODE=measure; shift; }
[ "${1:-}" = "--actions" ] && { MODE=actions; shift; }
LOGDIR="${1:-/tmp/ngtop-remote-logs}"
BIN="${NGTOP_BIN:-$ROOT/dist/ngtop}"
SECS="${MEASURE_SECS:-30}"
CONTAINERS="${CONTAINERS:-alpine debian systemd}"
CF="$ROOT/tests/fixtures/remote"
[ -x "$BIN" ] || { [ "$MODE" = actions ] && echo "note: $BIN missing: the --dump --sudo cases are skipped" || { echo "missing $BIN (run ./build.sh)"; exit 2; }; }
command -v podman >/dev/null || { echo "podman not found"; exit 2; }
mkdir -p "$LOGDIR"
TMP="$(mktemp -d /tmp/ngtop-remote.XXXXXX)"
RUN_ID="$$"
STARTED=()
cleanup() {
  for c in "${STARTED[@]:-}"; do [ -n "$c" ] && podman rm -f -t 0 "$c" >/dev/null 2>&1; done
  pkill -f "$TMP/ssh_config" 2>/dev/null
  rm -rf "$TMP"
}
trap cleanup EXIT

ssh-keygen -q -t ed25519 -N '' -f "$TMP/key" -C ngtop-test
: >"$TMP/ssh_config"
export NGTOP_SSH="ssh -F $TMP/ssh_config"

declare -A PORT CID
declare -A RESULT
ORDER=()

# ssh_config: un Host per container; BatchMode per non bloccarsi mai su una domanda.
add_host() { # alias porta [utente]
  cat >>"$TMP/ssh_config" <<CFG
Host $1
  HostName 127.0.0.1
  Port $2
  User ${3:-tester}
  IdentityFile $TMP/key
  IdentitiesOnly yes
  BatchMode yes
  StrictHostKeyChecking no
  UserKnownHostsFile /dev/null
  LogLevel ERROR
  ConnectTimeout 5
CFG
}

start_container() { # nome
  local n="$1" name="ngtop-test-$1-$RUN_ID" extra=() sum img
  # il tag dell'immagine contiene la somma dei file: se cambia la Containerfile, si ricostruisce
  sum="$(cat "$CF/Containerfile.$n" "$CF"/ngtop-test.service | cksum | cut -d' ' -f1)"
  img="localhost/ngtop-test-$n:$sum"
  if ! podman image exists "$img"; then
    podman build -q -t "$img" -f "$CF/Containerfile.$n" "$CF" >"$LOGDIR/build-$n.log" 2>&1 || return 1
  fi
  [ "$n" = systemd ] && extra=(--systemd=always)
  # SYS_NICE: senza, nemmeno root puo' cambiare il nice dei processi di un altro utente nel container
  podman run -d --name "$name" --cap-add SYS_NICE "${extra[@]}" -p 127.0.0.1::22 \
    -v "$TMP/key.pub":/tmp/authorized_keys.src:ro "$img" >/dev/null 2>"$LOGDIR/start-$n.log" || return 1
  STARTED+=("$name"); CID[$n]="$name"
  # authorized_keys: copiato con i permessi giusti (la mappatura rootless rende l'utente root il proprietario)
  local i
  for i in 1 2 3 4 5 6 7 8 9 10; do
    podman exec "$name" sh -c 'for u in tester sudon sudop sudotty; do
      id "$u" >/dev/null 2>&1 || continue
      mkdir -p /home/$u/.ssh && cp /tmp/authorized_keys.src /home/$u/.ssh/authorized_keys \
      && chown -R $u /home/$u/.ssh 2>/dev/null; chmod 700 /home/$u/.ssh; chmod 600 /home/$u/.ssh/authorized_keys; done' \
      >/dev/null 2>&1 && break
    sleep 1
  done
  PORT[$n]="$(podman port "$name" 22/tcp | head -1 | sed 's/.*://')"
  add_host "ct-$n" "${PORT[$n]}"
  add_host "ct-$n-nopw" "${PORT[$n]}" sudon
  add_host "ct-$n-pw" "${PORT[$n]}" sudop
  add_host "ct-$n-tty" "${PORT[$n]}" sudotty
  for i in $(seq 1 30); do
    ssh -F "$TMP/ssh_config" "ct-$n" true >/dev/null 2>&1 && return 0
    sleep 1
  done
  return 1
}

# Ritorna 0 se il file contiene tutte le espressioni (grep -E) date.
has() { local f="$1"; shift; local p; for p in "$@"; do grep -aEq -- "$p" "$f" || { echo "missing /$p/ in $(basename "$f")"; return 1; }; done; }
nopanic() { ! grep -aqi "panicked" "$1"; }

run_to() { # logfile comando... (con timeout 60)
  local log="$1"; shift
  timeout 60 "$@" >"$log" 2>&1
}

functional() { # nome
  local n="$1" t="ct-$1" msgs="" log rc
  log="$LOGDIR/$n-dump.log"
  run_to "$log" "$BIN" --dump "$t"; rc=$?
  if [ $rc -ne 0 ]; then msgs+="dump rc=$rc; "
  else
    m=$(has "$log" '^CPU:' '^Memory: total' '^Load:' '^Uptime:' '^ *PID ' '^ *1 +root') || msgs+="dump: $m; "
    nopanic "$log" || msgs+="dump panic; "
  fi
  local N pat
  for N in 1 2 3 4 5; do
    log="$LOGDIR/$n-snap$N.log"
    run_to "$log" "$BIN" --snapshot 120x40 --section "$N" "$t"; rc=$?
    if [ $rc -ne 0 ]; then msgs+="snap$N rc=$rc; "; continue; fi
    nopanic "$log" || msgs+="snap$N panic; "
    [ "$(wc -l <"$log")" -ge 20 ] || msgs+="snap$N too short; "
    case $N in
      1) pat='CPU' ;; 2) pat='GPU' ;; 3) pat='PID' ;; 4) pat='[Nn]etwork|[Dd]isk' ;;
      5) pat='[Ss]ervice|systemd' ;;
    esac
    grep -aEq "$pat" "$log" || msgs+="snap$N no /$pat/; "
    [ $N -eq 1 ] && ! grep -aq "$t\|127.0.0.1\|tester" "$log" && msgs+="snap1 no host name in bar; "
  done
  if [ "$n" = systemd ]; then
    grep -aq 'systemd units' "$LOGDIR/$n-snap5.log" && grep -aEq 'Active [1-9]' "$LOGDIR/$n-snap5.log" \
      || msgs+="services: no systemd units listed; "
    grep -aq 'Error:' "$LOGDIR/$n-snap5.log" && msgs+="services: 'Error:' shown in the detail pane; "
  fi
  # interattivo: pty, 5 s, SIGTERM; deve ripristinare il terminale
  log="$LOGDIR/$n-interactive.log"
  script -qc "stty cols 120 rows 40; timeout --foreground -s TERM 5 $BIN $t; echo EXIT=\$?" /dev/null >"$log" 2>&1
  grep -aq $'\e\[?1049l' "$log" || msgs+="interactive: no ?1049l; "
  grep -aq $'\e\[?25h' "$log" || msgs+="interactive: no ?25h; "
  nopanic "$log" || msgs+="interactive panic; "
  grep -aEq 'EXIT=(0|124|143)' "$log" || msgs+="interactive exit $(grep -ao 'EXIT=[0-9]*' "$log" | head -1); "
  [ -z "$msgs" ] && echo OK || echo "ERROR: $msgs"
}

# Host irraggiungibile: porta chiusa -> errore chiaro, exit 2.
t_unreachable() {
  cat >>"$TMP/ssh_config" <<CFG
Host ct-closed
  HostName 127.0.0.1
  Port 1
  User tester
  BatchMode yes
  StrictHostKeyChecking no
  UserKnownHostsFile /dev/null
  ConnectTimeout 5
CFG
  local log="$LOGDIR/unreachable.log" rc
  timeout 30 "$BIN" --dump ct-closed >"$log" 2>&1; rc=$?
  [ $rc -eq 2 ] || { echo "ERROR: exit $rc (expected 2)"; return; }
  nopanic "$log" || { echo "ERROR: panic"; return; }
  grep -aEqi 'refused|cannot|could not|unable|failed|connect' "$log" || { echo "ERROR: unclear message: $(head -c 200 "$log")"; return; }
  echo OK
}

# Container fermato durante l'esecuzione: "reconnecting", niente panic, uscita pulita con SIGTERM.
t_stopped() { # nome
  local n="$1" log="$LOGDIR/$n-stopped.log"
  ( script -qc "stty cols 120 rows 40; timeout --foreground -s TERM 14 $BIN ct-$n; echo EXIT=\$?" /dev/null >"$log" 2>&1 ) &
  local bg=$!
  sleep 5
  podman stop -t 1 "${CID[$n]}" >/dev/null 2>&1
  wait $bg
  nopanic "$log" || { echo "ERROR: panic"; return; }
  sed 's/\x1b\[[0-9;?]*[a-zA-Z]//g' "$log" | grep -aqi 'reconnect\|offline' || { echo "ERROR: no 'reconnecting' state shown"; return; }
  grep -aq $'\e\[?1049l' "$log" || { echo "ERROR: terminal not restored"; return; }
  grep -aEq 'EXIT=(0|124|143)' "$log" || { echo "ERROR: exit $(grep -ao 'EXIT=[0-9]*' "$log" | head -1)"; return; }
  echo OK
}

# Chiave tolta mentre ngtop e' collegato + sessione ssh uccisa: deve passare a "connection failed"
# (errore definitivo) senza riprovare all'infinito, e uscire con q.
t_authfail() { # nome
  local n="$1" log="$LOGDIR/$n-authfail.log"
  ( (sleep 14; printf q; sleep 3) | script -qc "stty cols 120 rows 40; timeout --foreground -s TERM 20 $BIN ct-$n; echo EXIT=\$?" /dev/null >"$log" 2>&1 ) &
  local bg=$!
  sleep 5
  podman exec "${CID[$n]}" sh -c 'rm -f /home/tester/.ssh/authorized_keys
    for p in /proc/[0-9]*; do tr "\0" " " <$p/cmdline 2>/dev/null | grep -q "sshd: tester" && kill ${p#/proc/}; done; true' >/dev/null 2>&1
  wait $bg
  # ripristina la chiave per le prove successive
  podman exec "${CID[$n]}" sh -c 'cp /tmp/authorized_keys.src /home/tester/.ssh/authorized_keys && chown tester /home/tester/.ssh/authorized_keys && chmod 600 /home/tester/.ssh/authorized_keys' >/dev/null 2>&1
  local clean; clean="$(sed 's/\x1b\[[0-9;?]*[a-zA-Z]//g' "$log")"
  nopanic "$log" || { echo "ERROR: panic"; return; }
  echo "$clean" | grep -aqi 'connection failed' || { echo "ERROR: no 'connection failed' state"; return; }
  echo "$clean" | grep -aqi 'press q' || { echo "ERROR: no 'press q' hint"; return; }
  grep -aq $'\e\[?1049l' "$log" || { echo "ERROR: terminal not restored"; return; }
  grep -aEq 'EXIT=(0|124|143)' "$log" || { echo "ERROR: exit $(grep -ao 'EXIT=[0-9]*' "$log" | head -1)"; return; }
  grep -aq 'EXIT=0' "$log" || { echo "ERROR: did not exit on q (EXIT=124, still running)"; return; }
  echo OK
}

# ---- misure -------------------------------------------------------------------------------
ticks() { # pid... somma utime+stime(+figli) in tick, da /proc locale
  local p s=0 f
  for p in "$@"; do
    f=$(sed 's/.*) //' "/proc/$p/stat" 2>/dev/null) || continue
    set -- $f; s=$((s + ${12} + ${13} + ${14} + ${15}))
  done
  echo $s
}
rticks() { # nome: tick (utime+stime+cutime+cstime) dei processi dell'utente tester: sh / sshd
  podman exec "${CID[$1]}" sh -c '
    for p in /proc/[0-9]*; do
      u=$(awk "/^Uid:/{print \$2}" $p/status 2>/dev/null); [ "$u" = "$(id -u tester)" ] || continue
      c=$(cat $p/comm 2>/dev/null); case "$c" in sh|sshd|bash|dash|ash) ;; *) continue;; esac
      set -- $(sed "s/.*) //" $p/stat); echo "$c $((${12}+${13}+${14}+${15}))"
    done' 2>/dev/null
}
measure() { # nome
  local n="$1" t="ct-$1" log="$LOGDIR/$n-measure.log" hz up
  hz=$(getconf CLK_TCK)
  ( script -qc "stty cols 182 rows 52; timeout --foreground -s TERM $((SECS + 12)) $BIN -d 1000 $t" /dev/null >"$log" 2>&1 ) &
  local bg=$!
  sleep 6
  local ng ssh_pid
  # il processo ngtop vero (non timeout/script): la riga di comando inizia con il binario
  ng=$(pgrep -n -f "^$BIN -d 1000 $t")
  ssh_pid=$(pgrep -n -f "ssh -F $TMP/ssh_config.* ct-$n")
  [ -n "$ng" ] && [ -n "$ssh_pid" ] || { echo "ERROR: ngtop/ssh process not found"; kill $bg 2>/dev/null; return; }
  local r0 r1 l0 l1 s0 s1 io0 io1
  r0=$(rticks "$n"); l0=$(ticks "$ng"); s0=$(ticks "$ssh_pid"); io0=$(awk '/^rchar/{print $2}' /proc/$ssh_pid/io)
  sleep "$SECS"
  r1=$(rticks "$n"); l1=$(ticks "$ng"); s1=$(ticks "$ssh_pid"); io1=$(awk '/^rchar/{print $2}' /proc/$ssh_pid/io)
  # snapshot per la latenza mostrata nella barra (--snapshot passa da una connessione nuova: non è lo stesso valore)
  kill -TERM "$ng" 2>/dev/null
  wait $bg
  # per processo remoto: delta tick
  local sh0 sh1 sshd0 sshd1
  sh0=$(echo "$r0" | awk '$1!="sshd"{s+=$2}END{print s+0}'); sh1=$(echo "$r1" | awk '$1!="sshd"{s+=$2}END{print s+0}')
  sshd0=$(echo "$r0" | awk '$1=="sshd"{s+=$2}END{print s+0}'); sshd1=$(echo "$r1" | awk '$1=="sshd"{s+=$2}END{print s+0}')
  local lat
  lat=$(sed 's/\x1b\[[0-9;?]*[a-zA-Z]//g' "$log" | grep -ao '⇄ [^ ]* [0-9]* ms' | awk '{s+=$3;c++}END{if(c)printf "%.1f ms (%d samples)", s/c, c; else print "n/a"}')
  echo "$n latency in bar (avg): $lat"
  awk -v n="$n" -v hz="$hz" -v secs="$SECS" -v sh="$((sh1 - sh0))" -v sshd="$((sshd1 - sshd0))" \
      -v ng="$((l1 - l0))" -v sc="$((s1 - s0))" -v by="$((io1 - io0))" 'BEGIN{
    printf "%-8s server sh %.2f%%  server sshd %.2f%%  ngtop local %.2f%%  ssh local %.2f%%  wire %.1f KB/update (rchar of ssh)\n",
      n, 100*sh/(hz*secs), 100*sshd/(hz*secs), 100*ng/(hz*secs), 100*sc/(hz*secs), by/secs/1024 }'
}


# ---- azioni remote ------------------------------------------------------------------------
NICE_LOWER=1
slug() { echo "$1" | tr -c 'A-Za-z0-9\n' _ | tr -s _; }
PWD_OK="pw-test-1"
ACT_RES=()
act_res() { # nome esito(0/1) dettaglio
  if [ "$2" = 0 ]; then ACT_RES+=("OK     $1"); else ACT_RES+=("ERROR  $1: $3"); ACTRC=1; fi
}
TESTBIN=""
find_testbin() {
  local out="$LOGDIR/cargo-build-test.log"
  ( cd "$ROOT" && PATH="$HOME/.cargo/bin:$PATH" cargo test --test remote --no-run ) >"$out" 2>&1 || return 1
  TESTBIN="$(grep -ao 'Executable tests/remote.rs (.*)' "$out" | sed 's/.*(\(.*\))/\1/' | head -1)"
  [ -n "$TESTBIN" ] && TESTBIN="$ROOT/$TESTBIN"
  [ -x "$TESTBIN" ]
}
# Esegue un test #[ignore] di tests/remote.rs. pwd non vuoto = sotto pty, la password viene scritta dopo 5 s.
e2e() { # nome host sudo(0/1) filtro unit foreign pwd
  local name="$1" host="$2" sudo="$3" filter="$4" unit="$5" fpid="$6" pw="${7:-}" log="$LOGDIR/actions-$(slug "$1").log" rc
  local envs=(NGTOP_E2E_NICE_LOWER="${NICE_LOWER:-1}" NGTOP_E2E_HOST="$host" NGTOP_E2E_SUDO="$sudo" NGTOP_E2E_UNIT="$unit" NGTOP_E2E_FOREIGN_PID="$fpid")
  if [ -n "$pw" ]; then
    (sleep 5; printf '%s\n' "$pw") | env "${envs[@]}" timeout 120 script -qec "$TESTBIN --ignored $filter --test-threads=1 --nocapture" /dev/null >"$log" 2>&1
  else
    env "${envs[@]}" timeout 120 "$TESTBIN" --ignored "$filter" --test-threads=1 --nocapture >"$log" 2>&1 </dev/null
  fi
  rc=$?
  if [ $rc -eq 0 ] && grep -aq '1 passed' "$log"; then act_res "$name" 0
  else act_res "$name" 1 "rc=$rc: $(grep -aE 'panicked|assert|left:|right:|Error|error' "$log" | head -3 | tr '\n' ' ' | cut -c1-300) (see $log)"; fi
}
foreign_sleep() { # nome container: sleep di tester (un altro utente) avviato nel container
  podman exec -u tester "${CID[$1]}" sh -c 'sleep 601 </dev/null >/dev/null 2>&1 & echo $!' 2>/dev/null
}
wait_systemd() { # nome container
  local i
  for i in $(seq 1 40); do
    podman exec "${CID[$1]}" sh -c 'systemctl is-system-running 2>&1 | grep -Eq "running|degraded" && systemctl is-active ngtop-test.service >/dev/null' 2>/dev/null && return 0
    sleep 1
  done
  return 1
}
# ngtop --dump --sudo sotto pty, con la password data. Risultato in $log, rc in $DRC.
dump_sudo() { # nome host password
  local log="$LOGDIR/actions-$(slug "$1").log"
  (sleep 5; printf '%s\n' "$3") | timeout 90 script -qec "stty cols 150 rows 50; $BIN --dump --sudo $2" /dev/null >"$log" 2>&1
  DRC=$?
}
sudo_log_clean() { # nome container: nessuna riga dell'agente nel log di sudo / auth / journal del server
  local out
  out="$(podman exec "${CID[$1]}" sh -c 'cat /var/log/sudo.log /var/log/auth.log 2>/dev/null; journalctl --no-pager 2>/dev/null' 2>/dev/null)"
  ! grep -aEq 'HAVE_TIMEOUT|PAGESIZE|CLKTCK|exec 3>&1|^NG=|Agente di ngtop' <<<"$out"
}
sudo_pw_leak() { # nome container password: la password non compare nei log del server
  ! podman exec "${CID[$1]}" sh -c 'cat /var/log/sudo.log /var/log/auth.log 2>/dev/null; journalctl --no-pager 2>/dev/null' 2>/dev/null | grep -aqF -- "$2"
}

actions() { # nome
  local n="$1" fp unit="" cx=1
  [ "$n" = systemd ] && { unit=ngtop-test.service; wait_systemd "$n" || { act_res "$n: systemd up" 1 "container systemd not ready"; return; }; }
  # il nice si puo' abbassare (anche da root) solo se il kernel lo permette: in podman rootless no
  podman exec "${CID[$n]}" sh -c '[ -z "$(nice -n -1 true 2>&1)" ]' >/dev/null 2>&1 && NICE_LOWER=1 || NICE_LOWER=0
  fp="$(foreign_sleep "$n")"
  # utente senza sudo
  e2e "$n: process actions, user tester (no sudo)" "ct-$n" 0 e2e_process_signal "" ""
  e2e "$n: foreign process refused (EPERM), sudon without --sudo" "ct-$n-nopw" 0 e2e_process_of_another_user "" "$fp"
  [ -n "$unit" ] && e2e "$n: services without --sudo: 'needs root' text" "ct-$n" 0 e2e_service_actions "$unit" ""
  # utente con sudo NOPASSWD
  e2e "$n: process actions, sudon without --sudo" "ct-$n-nopw" 0 e2e_process_signal "" ""
  e2e "$n: process actions, sudon --sudo (NOPASSWD)" "ct-$n-nopw" 1 e2e_process_signal "" ""
  [ -n "$unit" ] && e2e "$n: services without --sudo (sudon)" "ct-$n-nopw" 0 e2e_service_actions "$unit" ""
  [ -n "$unit" ] && e2e "$n: services --sudo (NOPASSWD): start/stop/restart/enable/disable" "ct-$n-nopw" 1 e2e_service_actions "$unit" ""
  fp="$(foreign_sleep "$n")"
  e2e "$n: foreign process killed with --sudo (NOPASSWD)" "ct-$n-nopw" 1 e2e_process_of_another_user "" "$fp"
  # utente con sudo a password (prompt su pty)
  e2e "$n: process actions, sudop --sudo (password)" "ct-$n-pw" 1 e2e_process_signal "" "" "$PWD_OK"
  [ -n "$unit" ] && e2e "$n: services --sudo (password)" "ct-$n-pw" 1 e2e_service_actions "$unit" "" "$PWD_OK"
  # password sbagliata: errore chiaro, nessuna riga dell'agente nei log, password non in chiaro
  if [ -x "$BIN" ] && "$BIN" --help 2>&1 | grep -q -- '--sudo'; then
    local bad="wrongpw-Zq7" log="$LOGDIR/actions-$(slug "$n: wrong password").log" m=""
    dump_sudo "$n: wrong password" "ct-$n-pw" "$bad"
    [ $DRC -ne 0 ] || m+="exit 0; "
    sed 's/\x1b\[[0-9;?]*[a-zA-Z]//g' "$log" | grep -aqi 'wrong password' || m+="no 'wrong password' text; "
    grep -aqF -- "$bad" <(sed 's/\x1b\[[0-9;?]*[a-zA-Z]//g' "$log" | tr -d '\r' | grep -av 'password for') && m+="typed password echoed; "
    nopanic "$log" || m+="panic; "
    sleep 2
    sudo_log_clean "$n" || m+="agent script lines found in the server's sudo/auth log or journal; "
    sudo_pw_leak "$n" "$bad" || m+="password found in the server's logs; "
    podman exec "${CID[$n]}" sh -c 'ps -eo user,args | grep -E "^sudop .*(sudo|sh)" | grep -v grep' 2>/dev/null | grep -aq . && m+="leftover sudo/sh session of sudop on the server; "
    [ -z "$m" ]; act_res "$n: wrong sudo password" $? "$m(see $log)"
    # password giusta: dump come root
    dump_sudo "$n: dump --sudo ok" "ct-$n-pw" "$PWD_OK"
    log="$LOGDIR/actions-$(slug "$n: dump --sudo ok").log"; m=""
    [ $DRC -eq 0 ] || m+="rc=$DRC; "
    grep -aEq '^ *1 +root' "$log" || m+="no pid 1 line; "
    grep -aqF -- "$PWD_OK" <(tr -d '\r' <"$log" | grep -av 'password for') && m+="password in output; "
    sudo_pw_leak "$n" "$PWD_OK" || m+="password found in the server's logs; "
    [ -z "$m" ]; act_res "$n: dump --sudo with correct password" $? "$m(see $log)"
    # utente senza diritti sudo
    dump_sudo "$n: not allowed" "ct-$n" "whatever"
    log="$LOGDIR/actions-$(slug "$n: not allowed").log"
    [ $DRC -ne 0 ] && sed 's/\x1b\[[0-9;?]*[a-zA-Z]//g' "$log" | grep -aEqi 'not allowed|wrong password|not in the sudoers' ; act_res "$n: user without sudo rights: clear error" $? "rc=$DRC, text: $(tr -d '\r' <"$log" | tail -c 200 | tr '\n' ' ') "
    # requiretty
    if [ "$n" = debian ]; then
      dump_sudo "$n: requiretty" "ct-$n-tty" "x"
      log="$LOGDIR/actions-$(slug "$n: requiretty").log"
      [ $DRC -ne 0 ] && sed 's/\x1b\[[0-9;?]*[a-zA-Z]//g' "$log" | grep -aEqi 'requiretty|terminal is required'; act_res "$n: requiretty: clear error" $? "rc=$DRC, text: $(tr -d '\r' <"$log" | tail -c 200 | tr '\n' ' ') "
    fi
  else
    act_res "$n: --dump --sudo cases" 1 "ngtop binary missing or without --sudo (run ./build.sh)"
  fi
}

# ---- main ---------------------------------------------------------------------------------
echo "images/containers: $CONTAINERS (logs in $LOGDIR)"
READY=()
for n in $CONTAINERS; do
  ORDER+=("$n")
  if start_container "$n"; then READY+=("$n"); else RESULT[$n]="ERROR: container did not start (see start-$n.log / build-$n.log)"; fi
done

if [ "$MODE" = measure ]; then
  echo "--- measurements: ${SECS}s per container, interval 1 s ---"
  for n in "${READY[@]}"; do measure "$n"; done
  for n in "${ORDER[@]}"; do [ -n "${RESULT[$n]:-}" ] && echo "$n: ${RESULT[$n]}"; done
  exit 0
fi

if [ "$MODE" = actions ]; then
  ACTRC=0
  find_testbin || { echo "cannot build tests/remote.rs (see $LOGDIR/cargo-build-test.log)"; exit 2; }
  echo "--- remote actions (test binary: $TESTBIN) ---"
  for n in "${READY[@]}"; do actions "$n"; done
  for n in "${ORDER[@]}"; do [ -n "${RESULT[$n]:-}" ] && echo "$n: ${RESULT[$n]}" && ACTRC=1; done
  printf '%s\n' "${ACT_RES[@]}"
  echo "logs in $LOGDIR"
  exit $ACTRC
fi

declare -A STOP
UNREACH="$(t_unreachable)"
for n in "${READY[@]}"; do RESULT[$n]="$(functional "$n")"; done
declare -A AUTH
for n in "${READY[@]}"; do [ "$n" = debian ] && AUTH[$n]="$(t_authfail "$n")"; done
for n in "${READY[@]}"; do STOP[$n]="$(t_stopped "$n")"; done

echo
printf '%-10s %-9s %s\n' CONTAINER FUNCTIONAL "STOPPED-DURING-RUN | AUTH-FAILURE"
rc=0
for n in "${ORDER[@]}"; do
  printf '%-10s %s | %s | %s\n' "$n" "${RESULT[$n]}" "${STOP[$n]:--}" "${AUTH[$n]:--}"
  [[ "${RESULT[$n]}" == OK && "${STOP[$n]:-OK}" == OK && "${AUTH[$n]:-OK}" == OK ]] || rc=1
done
printf '%-10s %s\n' unreachable "$UNREACH"
[ "$UNREACH" = OK ] || rc=1
echo "logs in $LOGDIR"
exit $rc
