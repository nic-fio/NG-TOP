#!/bin/sh
# Copyright (c) 2026 Nicola Fiorillo
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
#
# "ssh" finto e OSTILE per tests/hostile.rs: IGNORA agent.sh e risponde come un server malevolo.
# Uso (da NGTOP_SSH):  ssh SCENARIO LOG PARAM [argomenti ssh...]
#   LOG    file dove si registrano sessioni e richieste di azione ("-" = nessuno)
#   PARAM  parametro libero del scenario ("-" = nessuno)
# Gli scenari `proxy*` usano invece l'agente VERO (sh locale) con una cartella `bin/` di comandi finti
# davanti al PATH; gli scenari `sudo_*` fanno la parte di ssh + sudo.
SC=$1
LOG=$2
PARAM=$3
HERE=$(dirname "$0")
set -f
[ "$LOG" != - ] && echo "SESSION $SC" >> "$LOG"

# ---- agente vero, con comandi finti ----------------------------------------------------------
case $SC in
  proxy)
    PATH="$HERE/bin:$PATH"
    export PATH
    exec sh
    ;;
  proxy_ready_*)
    PATH="$HERE/bin:$PATH"
    export PATH
    case $SC in
      proxy_ready_huge) F='18446744073709551615 18446744073709551615 4294967296 99999999999999999999,4294967295' ;;
      proxy_ready_zero) F='0 0 0 0' ;;
      proxy_ready_neg) F='-1 -5 -1 -1,-2' ;;
      proxy_ready_root) F="4096 100 0 1,$PARAM" ;;
    esac
    sh | sed -u "s/^\(ngtop-[0-9a-f]*\) READY \([0-9]*\) .*/\1 READY \2 $F/"
    exit 0
    ;;
esac

# ---- sudo finto ------------------------------------------------------------------------------
# Protocollo di ngtop: un solo comando `env LC_ALL=C sudo -S -k -p 'PROMPT' env TERM=dumb sh -c 'printf ... ngsudo TAG; exec sh'`.
# sudo mostra PROMPT (senza newline) solo se serve la password; la shell avviata da sudo stampa ngsudo-TAG.
case $SC in
  sudo_*)
    PW='Sup3r-Secret-Zq9'
    { for a in "$@"; do printf '%s\n' "$a"; done; } >> "$LOG.argv"
    env >> "$LOG.env"
    PROMPT=$(printf '%s' "$*" | sed -n "s/.*-p '\([^']*\)'.*/\1/p")
    TAG=$(printf '%s' "$*" | sed -n 's/.*ngsudo \([0-9a-f]*\);.*/\1/p')
    [ -n "$PROMPT" ] && [ -n "$TAG" ] || { echo 'fake sudo: unexpected command' >&2; exit 1; }
    N=$(cat "$LOG.n" 2>/dev/null || echo 0)
    echo $((N + 1)) > "$LOG.n"
    # shell avviata da sudo: stampa il marcatore e registra una prima riga che NON sia "NG=..."
    # (sarebbe la password, o una riga dello script, finita in una shell che non doveva riceverle)
    cat > "$LOG.shell" <<'SH'
printf '%s-%s\n' ngsudo "$TAG"
IFS= read -r first
case $first in
  "NG="*) ;;
  *) printf '%s\n' "$first" >> "$LEAK"; IFS= read -r first ;;
esac
eval "$first"
export NG
exec sh
SH
    export TAG LEAK="$LOG.leak"
    drain() { timeout "$1" sh -c 'while IFS= read -r l; do echo "$l" >> "$1"; done' x "$LOG.leak"; }
    ask() { printf '%s' "$PROMPT" >&2; IFS= read -r pw; }
    case $SC in
      sudo_ignore)
        # prima sessione normale; dalle riconnessioni NOPASSWD e' comparso: sudo non chiede e non legge nulla
        if [ "$N" -ge 1 ]; then exec sh "$LOG.shell"; fi
        ask
        [ "$pw" = "$PW" ] || exit 1
        exec sh "$LOG.shell"
        ;;
      sudo_wrong)
        ask
        echo 'Sorry, try again.' >&2
        printf '%s' "$PROMPT" >&2
        drain 3
        exit 1
        ;;
      sudo_right)
        ask
        if [ "$pw" != "$PW" ]; then echo 'Sorry, try again.' >&2; printf '%s' "$PROMPT" >&2; drain 3; exit 1; fi
        exec sh "$LOG.shell"
        ;;
      sudo_slow)
        ask
        if [ "$N" -ge 1 ]; then
          # riconnessione: la password e' stata letta ma il marcatore non arriva per 30 s; chi scrive righe viene registrato
          drain 30
          exit 1
        fi
        exec sh "$LOG.shell"
        ;;
    esac
    exit 1
    ;;
esac

# ---- server ostile ---------------------------------------------------------------------------
IFS= read -r L
NG=${L#NG=\'}
NG=${NG%\'}
case $SC in
  ready_root) printf '%s READY 000 4096 100 0 1,%s\n' "$NG" "$PARAM" ;;
  ready_neg) printf '%s READY 000 -1 -5 -1 -1,-2,x,\n' "$NG" ;;
  ready_huge) printf '%s READY 000 18446744073709551615 18446744073709551615 4294967296 99999999999999999999,4294967295,0\n' "$NG" ;;
  escapes_connect)
    printf 'boom \033]0;PWNED\007\033[2J\033[31mred\n' >&2
    exit 1
    ;;
  *) printf '%s READY 000 4096 100 1000 -\n' "$NG" ;;
esac

rec() { printf '%s %s\n' "$NG" "$1"; }
endrec() { printf '\n%s ! %s\n' "$NG" "${1:-0}"; }
lines() { i=0; while [ "$i" -lt "$1" ]; do printf '%s\n' "$2"; i=$((i + 1)); done; }
good() { # verb n
  case $1 in
    j) rec J; lines "$2" 12345; endrec 0 ;;
    a | r) rec O; lines "$2" ok; endrec 0 ;;
    x) rec X; endrec 1; rec Y; printf 'systemctl: Failed to do it\n'; endrec 0 ;;
    *) rec "$(printf '%s' "$1" | tr a-z A-Z)"; endrec 0 ;;
  esac
}

reply() {
  first=${REQ%%
*}
  verb=${first%% *}
  w=$(printf '%s\n' "$first" | wc -w)
  case $verb in
    j) n=$((w - 1)) ;;
    a) n=$(((w - 2) / 2)) ;;
    r) n=1 ;;
    *) n=0 ;;
  esac
  act=
  case $verb in
    a | r | j) act=1 ;;
    x) case $first in *systemctl*) act=1 ;; esac ;;
  esac
  [ -n "$act" ] && [ "$LOG" != - ] && printf 'REQ %s\n' "$first" >> "$LOG"
  noz=
  printf '%s P\n' "$NG"
  case $first in
    *kernel/hostname*)
      if [ "$SC" = escapes ]; then
        rec F; printf 'evil\033]0;PWNED\007\033[2Jhost\n'; endrec 0
        printf '%s Z %s\n' "$NG" "$seq"
        return
      fi
      ;;
  esac
  if [ -z "$act" ]; then
    good "$verb" 0
  else
    case $SC in
      good) good "$verb" "$n" ;;
      badnonce) printf 'ngtop-deadbeef O\nok\n\nngtop-deadbeef ! 0\n' ;;
      dup) good "$verb" "$n"; good "$verb" "$n" ;;
      missing) ;;
      unasked) good "$verb" $((n + 3)) ;;
      fewer) [ "$n" -gt 0 ] && good "$verb" $((n - 1)) ;;
      rcbad) rec O; printf 'ok\n'; printf '\n%s ! notanumber\n' "$NG" ;;
      negative)
        case $verb in
          j) rec J; printf '%s\n' -5 99999999999999999999999 0 -0 1e3 ' 7 '; endrec 0 ;;
          *) rec O; printf '%s\n' -1 ok99999999999999999999 ''; endrec 0 ;;
        esac
        ;;
      huge40) rec J; head -c 40000000 /dev/zero | tr '\0' a; printf '\n'; endrec 0 ;;
      huge140) rec J; head -c 140000000 /dev/zero | tr '\0' a; printf '\n'; endrec 0 ;;
      endless) rec J; yes | tr -d '\n' ;;
      nozed) good "$verb" "$n"; noz=1 ;;
      escapes)
        case $verb in
          x) rec X; endrec 1; rec Y; printf 'boom \033]0;PWNED\007 \033[2J\033[31m end\n'; endrec 0 ;;
          j) good j "$n" ;;
          *) rec O; lines "$n" "$(printf 'ok\033[2J\007')"; endrec 0 ;;
        esac
        ;;
      escapes_stderr)
        printf 'Permission denied \033]0;PWNED\007\033[2J\033[31m(publickey)\n' >&2
        exit 0
        ;;
      drop)
        case $verb in
          x) case $first in *no-block*) ;; *) good x 0; printf '%s Z %s\n' "$NG" "$seq"; return ;; esac ;;
        esac
        rec O; printf 'ok\n'
        exit 0
        ;;
      *) good "$verb" "$n" ;;
    esac
  fi
  [ -n "$noz" ] || printf '%s Z %s\n' "$NG" "$seq"
}

# salta lo script dell'agente: la prima riga "p" apre la prima richiesta
while IFS= read -r line; do [ "$line" = p ] && break; done
REQ=
while :; do
  case $line in
    p) ;;
    "z "*)
      seq=${line#z }
      reply
      REQ=
      ;;
    *)
      REQ="$REQ$line
"
      ;;
  esac
  IFS= read -r line || exit 0
done
