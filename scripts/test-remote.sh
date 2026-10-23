#!/bin/bash
# Copyright (c) 2026 Nicola Fiorillo
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Prove end-to-end del monitoraggio remoto (`ngtop TARGET`) contro container con sshd (podman rootless).
# Uso: scripts/test-remote.sh [--measure] [cartella-log]
#   (senza opzioni)  prove funzionali + tabella OK/ERRORE
#   --measure        misure (byte, latenza, CPU server/locale) con intervallo 1 s, 30 s per container
# Variabili: NGTOP_BIN (default dist/ngtop), MEASURE_SECS (default 30), CONTAINERS="alpine debian systemd".
# Chiave e ssh_config stanno in una cartella temporanea; ~/.ssh non viene mai toccato.
set -u
cd "$(dirname "$0")/.."
ROOT="$PWD"
MODE=test
[ "${1:-}" = "--measure" ] && { MODE=measure; shift; }
LOGDIR="${1:-/tmp/ngtop-remote-logs}"
BIN="${NGTOP_BIN:-$ROOT/dist/ngtop}"
SECS="${MEASURE_SECS:-30}"
CONTAINERS="${CONTAINERS:-alpine debian systemd}"
CF="$ROOT/tests/fixtures/remote"
[ -x "$BIN" ] || { echo "missing $BIN (run ./build.sh)"; exit 2; }
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
add_host() { # nome porta
  cat >>"$TMP/ssh_config" <<CFG
Host $1
  HostName 127.0.0.1
  Port $2
  User tester
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
  local n="$1" img="localhost/ngtop-test-$1" name="ngtop-test-$1-$RUN_ID" extra=()
  if ! podman image exists "$img"; then
    podman build -q -t "$img" -f "$CF/Containerfile.$n" "$CF" >"$LOGDIR/build-$n.log" 2>&1 || return 1
  fi
  [ "$n" = systemd ] && extra=(--systemd=always)
  podman run -d --name "$name" "${extra[@]}" -p 127.0.0.1::22 \
    -v "$TMP/key.pub":/tmp/authorized_keys.src:ro "$img" >/dev/null 2>"$LOGDIR/start-$n.log" || return 1
  STARTED+=("$name"); CID[$n]="$name"
  # authorized_keys: copiato con i permessi giusti (la mappatura rootless rende l'utente root il proprietario)
  local i
  for i in 1 2 3 4 5 6 7 8 9 10; do
    podman exec "$name" sh -c 'mkdir -p /home/tester/.ssh && cp /tmp/authorized_keys.src /home/tester/.ssh/authorized_keys \
      && chown -R tester /home/tester/.ssh 2>/dev/null; chmod 700 /home/tester/.ssh; chmod 600 /home/tester/.ssh/authorized_keys' \
      >/dev/null 2>&1 && break
    sleep 1
  done
  PORT[$n]="$(podman port "$name" 22/tcp | head -1 | sed 's/.*://')"
  add_host "ct-$n" "${PORT[$n]}"
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
