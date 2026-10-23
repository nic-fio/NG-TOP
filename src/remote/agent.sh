# Copyright (c) 2026 Nicola Fiorillo
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
#
# Agente di ngtop sulla macchina remota: POSIX sh (dash, busybox ash), nessun file temporaneo.
# Il client invia "NG=<nonce>" e poi questo testo; infine una riga per richiesta, valutata con eval
# (il client quota ogni argomento). Risposta = sequenza di record:
#   <NG> <tipo> <argomento>\n<corpo grezzo>\n<NG> ! <rc>\n     (corpo = byte esatti, qualunque contenuto)
# Il corpo termina al primo "\n<NG> ": il nonce e' casuale e segreto, quindi non puo' comparire nei dati.
# Il "\n" prima del marcatore e' sempre aggiunto da noi (il client lo scarta): i file senza newline finale
# e i binari/NUL restano esatti. rc: 0 ok, altrimenti un errno (2 ENOENT, 13 EACCES, 21 EISDIR, 22 EINVAL...).
IFS=
LC_ALL=C
export LC_ALL
NL='
'
exec 3>&1
HAVE_TIMEOUT=
TK=
# timeout(1): GNU e busybox recenti; i busybox vecchi vogliono -t (non supportati: nessun tempo massimo remoto)
timeout 1 true >/dev/null 2>&1 && HAVE_TIMEOUT=1
[ -n "$HAVE_TIMEOUT" ] && timeout -k 1 1 true >/dev/null 2>&1 && TK=1
LSQ=
# ls -q: i caratteri non stampabili (newline compresi) diventano "?", cosi' un nome ostile non puo' falsificare righe;
# busybox non ha -q ma li sostituisce comunque
ls -q / >/dev/null 2>&1 && LSQ=-q
PAGESIZE=$(getconf PAGESIZE 2>/dev/null)
CLKTCK=$(getconf CLK_TCK 2>/dev/null)
case $PAGESIZE in ''|*[!0-9]*) PAGESIZE=4096 ;; esac
case $CLKTCK in ''|*[!0-9]*) CLKTCK=100 ;; esac
STATC=
stat -L -c '%u' / >/dev/null 2>&1 && STATC=1
STATF=
stat -f -c '%S' / >/dev/null 2>&1 && STATF=1

# tmo SECONDI CMD...: con tempo massimo se timeout(1) c'e'
tmo() {
  ng_s=$1; shift
  if [ -z "$HAVE_TIMEOUT" ]; then "$@"
  elif [ -n "$TK" ]; then timeout -k 1 "$ng_s" "$@"
  else timeout "$ng_s" "$@"; fi
}
e() { printf '\n%s ! %s\n' "$NG" "$1"; }
# classifica un errore di lettura: ritorna un errno
errc() {
  if [ ! -e "$1" ] && [ ! -L "$1" ]; then return 2
  elif [ -d "$1" ]; then return 21
  elif [ ! -r "$1" ]; then return 13
  else return 5; fi
}
p() { printf '%s P\n' "$NG"; }
z() { printf '%s Z %s\n' "$NG" "$1"; }

# f PATH: contenuto di un file
f() {
  printf '%s F %s\n' "$NG" "$1"
  if cat "$1" 2>/dev/null; then e 0; else errc "$1"; e $?; fi
}
# F GLOB: un record F per ogni file che corrisponde
F() {
  for ng_p in $1; do
    [ -e "$ng_p" ] || [ -L "$ng_p" ] || continue
    case $ng_p in *"$NL"*) continue ;; esac
    f "$ng_p"
  done
}
# t PATH: come f ma senza fork, con i builtin (solo per file di testo senza NUL: /proc, /sys).
# Costo: ~1 syscall per byte, ma niente fork+exec (4 ms qui) per file.
t() {
  printf '%s F %s\n' "$NG" "$1"
  if [ -r "$1" ] && [ ! -d "$1" ]; then
    ng_l=
    while IFS= read -r ng_l; do printf '%s\n' "$ng_l"; done 2>/dev/null < "$1"
    [ -n "$ng_l" ] && printf '%s' "$ng_l"
    e 0
  else errc "$1"; e $?; fi
}
T() {
  for ng_p in $1; do
    [ -e "$ng_p" ] || [ -L "$ng_p" ] || continue
    case $ng_p in *"$NL"*) continue ;; esac
    t "$ng_p"
  done
}
# B GLOB: molti file con possibili NUL (cmdline) con un solo wc e un solo cat: record B = "lunghezza percorso"
# per riga, record C = i byte concatenati, record Q = stessa misura rifatta dopo il cat. Il client accetta solo
# se B e Q coincidono e le somme tornano (altrimenti ripete il blocco).
B() {
  ng_pat=$1
  set -- $1
  [ -e "$1" ] || return 0
  printf '%s B %s\n' "$NG" "$ng_pat"
  wc -c "$@" 2>/dev/null
  e 0
  printf '%s C\n' "$NG"
  cat "$@" 2>/dev/null
  e 0
  # seconda misura dopo il cat: il client accetta solo se coincide riga per riga con la prima
  printf '%s Q\n' "$NG"
  wc -c "$@" 2>/dev/null
  e 0
}
# G ARG...: contenuto di molti file di TESTO con UN solo grep: ogni riga esce come "percorso:riga" (nessuna
# falsificazione possibile: il prefisso lo mette grep su ogni riga). Ogni argomento e' espanso come glob.
# File vuoti, illeggibili o inesistenti non producono righe. Niente fork per file.
G() {
  ng_n=$#
  for ng_pat; do set -- "$@" $ng_pat; done
  shift $ng_n
  printf '%s G\n' "$NG"
  grep -H '' "$@" 2>/dev/null
  e 0
}
# M ARG...: "ls -l" di molte cartelle (fd) con un solo processo; ogni argomento e' espanso come glob. Con una
# sola cartella stampa a mano l'intestazione (ls non lo fa). Il record porta il primo argomento.
M() {
  ng_n=$#
  for ng_pat; do set -- "$@" $ng_pat; done
  shift $ng_n
  # solo cartelle leggibili: una illeggibile non deve comparire come "vuota"
  ng_n=$#
  for ng_d; do
    [ -d "$ng_d" ] && [ -r "$ng_d" ] && [ -x "$ng_d" ] && set -- "$@" "$ng_d"
  done
  shift $ng_n
  [ $# -gt 0 ] || return 0
  printf '%s M\n' "$NG"
  [ $# = 1 ] && printf '%s:\n' "$1"
  ls -l $LSQ "$@" 2>/dev/null
  e 0
}
# l PATH: destinazione di un link
l() {
  printf '%s L %s\n' "$NG" "$1"
  if readlink "$1" 2>/dev/null; then e 0
  elif [ -L "$1" ]; then e 13
  elif [ -e "$1" ]; then e 22
  else e 2; fi
}
# K PADRE FOGLIA: link di PADRE/FOGLIA (due glob), un readlink per cartella. Record N (percorsi, NUL) e K
# (una destinazione per riga); se qualche readlink fallisce il client ne vede meno e rifa' con l.
K() {
  ng_leaf=$2
  for ng_d in $1; do
    [ -d "$ng_d" ] || continue
    set -- "$ng_d"/$ng_leaf
    [ -L "$1" ] || continue
    printf '%s N\n' "$NG"
    printf '%s\0' "$@"
    e 0
    printf '%s K\n' "$NG"
    readlink "$@" 2>/dev/null
    e 0
  done
}
# d PATH: nomi delle voci, ciascuno terminato da NUL
d() {
  printf '%s D %s\n' "$NG" "$1"
  if [ -d "$1" ] && [ -r "$1" ]; then
    for ng_n in "$1"/* "$1"/.[!.]* "$1"/..?*; do
      [ -e "$ng_n" ] || [ -L "$ng_n" ] || continue
      printf '%s\0' "${ng_n##*/}"
    done
    e 0
  else
    errc "$1"; ng_r=$?
    [ $ng_r = 5 ] && ng_r=20
    e $ng_r
  fi
}
D() {
  for ng_p in $1; do
    [ -d "$ng_p" ] || continue
    case $ng_p in *"$NL"*) continue ;; esac
    d "$ng_p"
  done
}
# u PATH: uid del proprietario
uf() { # ripiego senza stat -c: ls -lnLd, terzo campo
  ng_x=$(ls -lnLd "$1" 2>/dev/null) || return 1
  IFS=' 	'; set -- $ng_x; IFS=
  [ -n "$3" ] && echo "$3"
}
u() {
  printf '%s U %s\n' "$NG" "$1"
  if { [ -n "$STATC" ] && stat -L -c %u "$1" 2>/dev/null; } || { [ -z "$STATC" ] && uf "$1"; }; then e 0
  else errc "$1"; e $?; fi
}
# U ARG...: uid di molti percorsi (ogni argomento e' espanso come glob) con UN solo stat: record V con righe
# "uid percorso".
U() {
  ng_n=$#
  for ng_pat; do set -- "$@" $ng_pat; done
  shift $ng_n
  if [ -n "$STATC" ]; then
    printf '%s V\n' "$NG"
    stat -L -c '%u %n' "$@" 2>/dev/null
    e 0
  else
    for ng_p; do
      [ -e "$ng_p" ] || continue
      u "$ng_p"
    done
  fi
}
# W PERCORSO...: link di molti percorsi con UN solo ls (righe "... percorso -> destinazione")
W() {
  printf '%s W\n' "$NG"
  ls -ld $LSQ "$@" 2>/dev/null
  e 0
}
# H PERCORSO...: statvfs di molti mount con UN solo stat: righe "frsize blocchi disponibili inode liberi
# blocchi_liberi_totali percorso"
H() {
  if [ -n "$STATF" ]; then
    printf '%s H\n' "$NG"
    tmo 5 stat -f -c '%S %b %a %c %d %f %n' "$@" 2>/dev/null
    e 0
  else
    for ng_p; do s "$ng_p"; done
  fi
}
# h PATH: statvfs di UN filesystem di rete/FUSE con scadenza di 2 s (un mount appeso non deve bloccare l'agente);
# rc 110 = scaduto o senza timeout(1)
h() {
  printf '%s S %s\n' "$NG" "$1"
  if [ -z "$HAVE_TIMEOUT" ] || [ -z "$STATF" ]; then e 110; return; fi
  tmo 2 stat -f -c '%S %b %a %c %d %f' "$1" 2>/dev/null
  ng_r=$?
  if [ $ng_r = 0 ]; then e 0
  elif [ $ng_r = 124 ] || [ $ng_r = 137 ] || [ $ng_r = 143 ]; then e 110
  else errc "$1"; e $?; fi
}
# s PATH: "frsize blocchi disponibili inode inode_liberi blocchi_liberi_totali" (con df: ultimo = totali - usati)
s() {
  printf '%s S %s\n' "$NG" "$1"
  if [ -n "$STATF" ]; then
    if tmo 5 stat -f -c '%S %b %a %c %d %f' "$1" 2>/dev/null; then e 0; else errc "$1"; e $?; fi
  else
    if [ -e "$1" ]; then
      tmo 2 df -P -k "$1" 2>/dev/null | { IFS=' ' read -r ng_h; IFS=' ' read -r ng_a ng_b ng_c ng_d ng_rest
        [ -n "$ng_b" ] && printf '1024 %s %s 0 0 %s\n' "$ng_b" "$ng_d" "$((ng_b - ng_c))"; }
      e 0
    else e 2; fi
  fi
}
# x SECONDI ARGV...: esegue un comando; stdout nel corpo X, stderr nel record Y; rc = codice d'uscita
x() {
  ng_t=$1; shift
  printf '%s X\n' "$NG"
  ng_r=$( {
    if [ -n "$TK" ]; then ( exec timeout -k 2 "$ng_t" "$@" )
    elif [ -n "$HAVE_TIMEOUT" ]; then ( exec timeout "$ng_t" "$@" )
    else ( exec "$@" ); fi </dev/null
    printf '\n%s' "$?" >&2
  } 2>&1 1>&3 )
  ng_c=${ng_r##*"$NL"}
  ng_err=${ng_r%"$NL"*}
  e "$ng_c"
  printf '%s Y\n%s' "$NG" "$ng_err"
  e 0
}

ng_main() {
  printf '%s READY %s%s%s %s %s\n' "$NG" "${STATC:-0}" "${STATF:-0}" "${HAVE_TIMEOUT:-0}" "$PAGESIZE" "$CLKTCK"
  while read -r ng_line; do
    eval "$ng_line"
  done
}
ng_main
