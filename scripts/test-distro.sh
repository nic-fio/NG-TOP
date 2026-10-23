#!/bin/bash
# Copyright (c) 2026 Nicola Fiorillo
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Prova dist/ngtop (statico) su più distribuzioni con podman rootless.
# Uso: scripts/test-distro.sh [cartella-log] [immagine...]
# Per ogni immagine esegue --dump, --snapshot 120x40 e --snapshot 80x24 --section 2
# e verifica codice d'uscita 0 e output non vuoto. Stampa una tabella finale.
set -u
cd "$(dirname "$0")/.."
BIN="$PWD/dist/ngtop"
LOGDIR="${1:-/tmp}"
shift 2>/dev/null || true
IMAGES=("$@")
if [ ${#IMAGES[@]} -eq 0 ]; then
  IMAGES=(
    docker.io/library/ubuntu:24.04
    docker.io/library/debian:12
    docker.io/library/fedora:41
    docker.io/library/archlinux:latest
    docker.io/library/alpine:3.20
    docker.io/opensuse/tumbleweed:latest
    docker.io/library/debian:10
    docker.io/library/centos:7
  )
fi
[ -x "$BIN" ] || { echo "missing $BIN (run ./build.sh)"; exit 2; }
command -v podman >/dev/null || { echo "podman not found"; exit 2; }
mkdir -p "$LOGDIR"

declare -A RESULT
ORDER=()
run_case() { # immagine etichetta comando...
  local img="$1" label="$2"; shift 2
  local log="$LOGDIR/$(echo "$img" | tr '/:' '__')-$label.log"
  podman run --rm --network=none -e TERM=xterm-256color -e COLUMNS=120 -e LINES=40 \
    -v "$BIN":/ngtop:ro "$img" /ngtop "$@" >"$log" 2>&1
  local rc=$?
  if [ $rc -ne 0 ]; then echo "rc=$rc ($label)"; return 1; fi
  if [ ! -s "$log" ]; then echo "empty output ($label)"; return 1; fi
  return 0
}

# Caso interattivo: ngtop sotto `script` (pty) per 3 s, poi SIGTERM; deve uscire pulito e
# lasciare le sequenze di ripristino del terminale (?1049l e ?25h). Result: OK, ERROR or n/a.
run_interactive() { # immagine
  local img="$1"
  local log="$LOGDIR/$(echo "$img" | tr '/:' '__')-interactive.log"
  podman run --rm -t -e TERM=xterm-256color -v "$BIN":/ngtop:ro "$img" sh -c \
    'command -v script >/dev/null 2>&1 || { echo NO_SCRIPT; exit 0; }
     T="timeout"; timeout --help 2>&1 | grep -q foreground && T="timeout --foreground"
     script -qc "stty cols 120 rows 40; $T -s TERM 3 /ngtop; echo EXIT=\$?" /dev/null' >"$log" 2>&1
  local rc=$?
  if grep -q NO_SCRIPT "$log"; then echo "n/a"; return 0; fi
  [ $rc -eq 0 ] || { echo "ERROR rc=$rc"; return 1; }
  grep -q $'\e\[?1049l' "$log" || { echo "ERROR: missing ?1049l"; return 1; }
  grep -q $'\e\[?25h' "$log" || { echo "ERROR: missing ?25h"; return 1; }
  grep -qi panicked "$log" && { echo "ERROR: panic"; return 1; }
  # EXIT=0 (uscita normale) o 124 (timeout ha mandato SIGTERM): mai crash (134/139/101)
  grep -Eq 'EXIT=(0|124|143)' "$log" || { echo "ERROR: exit code $(grep -o 'EXIT=[0-9]*' "$log" | head -1)"; return 1; }
  echo OK
}

declare -A INTER

for img in "${IMAGES[@]}"; do
  ORDER+=("$img")
  safe="$(echo "$img" | tr '/:' '__')"
  if ! podman image exists "$img" && ! podman pull -q "$img" >"$LOGDIR/$safe-pull.log" 2>&1; then
    RESULT[$img]="ERROR: image could not be pulled (see $safe-pull.log)"
    continue
  fi
  msgs=""
  m=$(run_case "$img" dump --dump) || msgs+="$m; "
  m=$(run_case "$img" snap120 --snapshot 120x40) || msgs+="$m; "
  m=$(run_case "$img" snap80gpu --snapshot 80x24 --section 2) || msgs+="$m; "
  if [ -z "$msgs" ]; then RESULT[$img]="OK"; else RESULT[$img]="ERROR: $msgs"; fi
  INTER[$img]="$(run_interactive "$img")"
done

echo
printf '%-45s %-12s %s\n' DISTRO "CLI/SNAPSHOT" INTERACTIVE
for img in "${ORDER[@]}"; do printf '%-45s %-12s %s\n' "$img" "${RESULT[$img]}" "${INTER[$img]:-n/a}"; done
echo "logs in $LOGDIR"
for img in "${ORDER[@]}"; do
  [[ "${RESULT[$img]}" == OK ]] || exit 1
  [[ "${INTER[$img]:-n/a}" == ERROR* ]] && exit 1
done
exit 0
