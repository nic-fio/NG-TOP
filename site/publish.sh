#!/usr/bin/env bash
# Copyright (c) 2026 Nicola Fiorillo
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# publish.sh - costruisce il sito di ngtop in site/public/ e lo pubblica su
# https://ngtop.nicfio.it (VPS dell'autore, utente "progetti", /srv/www/ngtop.nicfio.it).
#
#   site/publish.sh            costruisce e pubblica
#   site/publish.sh --build    solo costruzione (guardare prima site/public/)
#
# Il sito: la pagina (site/landing/) con dimensione e SHA-256 del binario inseriti al build,
# il terminale dimostrativo (ngterm.*, tui.js), i due manuali da docs/ (o una pagina "in
# preparazione"), licence.html e decisions.html (generate da LICENSE.md e docs/), download/
# (ngtop, SHA256SUMS, licenze), l'anteprima per i link (og.png), robots.txt e sitemap.xml.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
OUT=$ROOT/site/public
HOST=progetti@57.131.27.241
DEST=/srv/www/ngtop.nicfio.it
URL=https://ngtop.nicfio.it

rm -rf "$OUT"
mkdir -p "$OUT/download"

# binario: dist/ngtop, ricostruito con ./build.sh se manca o è più vecchio dei sorgenti
if [ ! -x "$ROOT/dist/ngtop" ] || [ -n "$(find "$ROOT/src" "$ROOT/Cargo.toml" "$ROOT/Cargo.lock" -newer "$ROOT/dist/ngtop" -print -quit)" ]; then
    "$ROOT/build.sh"
fi
DL=$OUT/download
cp "$ROOT/dist/ngtop" "$DL/ngtop"
(cd "$DL" && sha256sum ngtop > SHA256SUMS)
SHA=$(cut -d' ' -f1 "$DL/SHA256SUMS")
SIZE=$(stat -c %s "$DL/ngtop")
SIZE_TXT=$(python3 -c "print('%.1f MB' % ($SIZE/1048576))")
cp "$ROOT/LICENSE.md" "$DL/LICENSE.txt"
cp "$ROOT/NOTICE.md" "$DL/NOTICE.txt"
cp "$ROOT/THIRD-PARTY-LICENSES" "$DL/THIRD-PARTY-LICENSES.txt"
cp "$ROOT/THIRD-PARTY-LICENSES" "$OUT/third-party-licences.txt"

# pagina: dimensione e SHA-256 del binario si inseriscono al build, mai a mano
sed -e "s/@SIZE@/$SIZE_TXT/g" -e "s/@SHA256@/$SHA/g" "$ROOT/site/landing/index.html" > "$OUT/index.html"
cp "$ROOT/site/landing/ngterm.css" "$ROOT/site/landing/ngterm.js" "$ROOT/site/landing/tui.js" "$ROOT/site/og.png" "$OUT/"

# manuali: quelli veri se ci sono, altrimenti una pagina provvisoria con lo stesso nome
for m in "User Manual" "Technical Manual"; do
    if [ -f "$ROOT/docs/$m.html" ]; then
        cp "$ROOT/docs/$m.html" "$OUT/"
    else
        cat > "$OUT/$m.html" <<HTML
<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>$m — ngtop</title><meta name="robots" content="noindex">
<style>body{margin:0;min-height:100vh;display:grid;place-items:center;font:17px/1.6 system-ui,sans-serif;
background:linear-gradient(135deg,#22c1ee,#7b4ff0);color:#fff;text-align:center;padding:24px}
a{color:#fff;font-weight:600}</style></head><body><main><h1>The ngtop $m is being written</h1>
<p>It will be published here together with the first release.</p><p><a href="./">← Back to ngtop</a></p></main></body></html>
HTML
    fi
done
[ -d "$ROOT/docs/assets" ] && cp -r "$ROOT/docs/assets" "$OUT/"

# licenza e decisioni di progetto, come pagine nello stesso stile
python3 "$ROOT/site/licence-page.py" "$ROOT/LICENSE.md" > "$OUT/licence.html"
python3 "$ROOT/site/md-page.py" "$ROOT/docs/decisions-and-history.md" decisions.html \
    "ngtop design decisions and project history" \
    "Why ngtop is built the way it is: the design decisions D1 to D14, the history of the project and its known gaps." > "$OUT/decisions.html"

# URL canonico, anteprima per i link e un titolo adatto ai motori di ricerca per ogni pagina
SEO=$ROOT/site/seo-head.py
python3 "$SEO" "$OUT/index.html" "$URL/" \
    "ngtop — a terminal system monitor: CPU, memory, GPU, network, disks, processes, services" \
    "ngtop is a terminal system monitor for Linux: btop, nvtop and the Windows Task Manager in one static binary with nothing to install."
if [ -f "$ROOT/docs/User Manual.html" ]; then
python3 "$SEO" "$OUT/User Manual.html" "$URL/User%20Manual.html" \
    "ngtop User Manual — the terminal system monitor for Linux" \
    "How to use ngtop: sections, keys, mouse, signals, services, configuration, permissions and troubleshooting."
fi
if [ -f "$ROOT/docs/Technical Manual.html" ]; then
python3 "$SEO" "$OUT/Technical Manual.html" "$URL/Technical%20Manual.html" \
    "ngtop Technical Manual — architecture and internals" \
    "How ngtop is built: data sources in /proc and /sys, the interface layer, safety of signals, performance, static build and tests."
fi
python3 "$SEO" "$OUT/licence.html" "$URL/licence.html" \
    "ngtop licence: PolyForm Noncommercial 1.0.0" \
    "ngtop is free for any noncommercial use, under the PolyForm Noncommercial License 1.0.0; commercial use needs a licence from the author."
python3 "$SEO" "$OUT/decisions.html" "$URL/decisions.html" \
    "ngtop design decisions and project history" \
    "Why ngtop is built the way it is: the design decisions D1 to D14, the history of the project and its known gaps."

cat > "$OUT/robots.txt" <<TXT
User-agent: *
Allow: /
Sitemap: $URL/sitemap.xml
TXT
TODAY=$(date +%F)
{
    echo '<?xml version="1.0" encoding="UTF-8"?>'
    echo '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">'
    echo "  <url><loc>$URL/</loc><lastmod>$TODAY</lastmod></url>"
    for m in "User%20Manual.html" "Technical%20Manual.html" "licence.html" "decisions.html"; do
        echo "  <url><loc>$URL/$m</loc><lastmod>$TODAY</lastmod></url>"
    done
    echo '</urlset>'
} > "$OUT/sitemap.xml"

echo "costruito $OUT:"
(cd "$OUT" && find . -type f -printf '  %p  %s byte\n' | sort)

[ "${1:-}" = --build ] && exit 0

# il server diventa identico a site/public (niente rsync da nessuna delle due parti):
# prima si riceve tutto in una cartella temporanea, poi si sostituisce il contenuto,
# così una rete che cade a metà non lascia il sito vuoto
tar -C "$OUT" -cf - --mode=u=rwX,go=rX . | ssh "$HOST" "set -eu
  test -d '$DEST' || { echo 'cartella remota $DEST mancante (lanciare add-site.sh sulla VPS)' >&2; exit 1; }
  tmp=\$(mktemp -d)
  trap 'rm -rf \"\$tmp\"' EXIT
  tar -C \"\$tmp\" -xf - --no-same-owner
  test -s \"\$tmp/index.html\"
  chmod 755 \"\$tmp\"
  find '$DEST' -mindepth 1 -delete
  cp -R \"\$tmp\"/. '$DEST'/"
echo "pubblicato su $URL"
