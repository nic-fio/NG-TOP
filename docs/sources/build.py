"""Generator of ngtop's manuals, in the common style of the manuals of nic-fio's
projects (AMS, EFI_PARTITION_MANAGER, HOSTER, MTERM, NESH, PHONESTRA, REMOTIX, SCRAPER):
the generator is the one of REMOTIX, adapted to ngtop (Rust sources, real screenshots).

It produces self-contained HTML files (no external files), in English:
  docs/Technical Manual.html   from the chapters in technical/chNN_*.py
  docs/User Manual.html        from the chapters in user/chNN_*.py

Each chapter exposes CHAPTER = (title, [(section_title, html), ...]).
The placeholders «FIG» and «TAB» in the captions become «Figure N.M» and
«Table N.M», numbered per chapter; rif("Title of a section") becomes the
link to that section. The style is style.css and the script (search in the
sidebar, Copy button on command blocks) is manual.js: they are the
common canon, identical byte for byte in all the manuals, and are not modified
here; ngtop's only additions live in EXTRA_CSS, below.
The file map and the numbers table are counted from the sources at every generation.

    python3 docs/sources/build.py [user|technical]           regenerates the manuals (default: both)
    python3 docs/sources/build.py --check [user|technical]   checks that they are aligned with the code
    python3 docs/sources/build.py --refresh-doc              rewrites doc.json from `dist/ngtop --doc-json`

It needs the pygments library (package python3-pygments).

What is specific to ngtop
-------------------------
snap(screen, cap, size="120x36", roots="demo", title="", overlay=None, select_pid=None)
    A figure with a REAL screen of ngtop. It runs
        dist/ngtop --snapshot WxH --section N --ansi --roots tests/fixtures/<roots>
    on a copy of the demonstration machine (tests/fixtures/demo: a "workstation" with a
    Ryzen 9 7900X, 32 GB, a Radeon RX 6800 and an iGPU; no data of the author's PC), while a small
    thread makes the copy's counters move (docs/sources/build.py: DemoAnimator, driven by
    tests/fixtures/demo/animate.json), so CPU, network, disks and GPU show believable activity. The
    ANSI truecolor output becomes an inline SVG (braille, block and box characters are drawn as
    shapes, so they do not depend on the reader's font) inside a terminal-window frame.
    screen is one of: overview, gpu, processes, network, services, compact (= overview at 80x24).
    The SVGs are cached in docs/sources/snaps/*.svg and regenerated only when the binary or the
    fixture change; if the binary is missing the cached SVG is used (marked stale in --check).
    A screen that cannot be produced shows a visible "GENERATED: ..." box, and --check fails.
    overlay (help, signal, confirm, filter) opens that window (`--overlay`); select_pid selects that
    process first (`--select-pid`; for overlay="signal" the default is firefox, PID 2210). With --roots the
    services screen uses the binary's fake systemd backend, so no real data appears.

keys_table(section) and options_table()
    The reference of keys and options, from the hidden option `ngtop --doc-json` of the binary
    (so the manual and the program cannot diverge). The output is kept in docs/sources/doc.json
    and rewritten whenever the binary changes (`--check` fails if doc.json does not match the
    binary; `build.py --refresh-doc` rewrites it by hand). If there is no binary the file is used;
    if there is neither, they show a visible placeholder "GENERATED: keys of <section>" and
    --check reports it as missing. Format of the JSON:
        {"version": "1.0.0",
         "sections": [{"name": "Overview", "keys": [["F1", "Help"], ...],   # bottom bar
                       "help": [["Up Down PgUp PgDn", "select"], ...]},     # lines of the F1 help
                      ... Overview, GPU, Processes, Network & Disks, Services],
         "compact_keys": [...], "filter_keys": [...], "global_keys": [["q F10 Ctrl-C", "quit"], ...],
         "options": [{"flag": "-d, --delay MS", "help": "..."}, ...]}
    keys_table("Overview") merges "help" (first) and the bar "keys" not already covered by it or
    by the global keys, in one table Key / Action; keys_table("global"), keys_table("compact")
    and keys_table("filter") show those lists; options_table() shows "options".

--check fails if: a manual is not regenerated from the sources; a rif() points to a missing
section (this fails at generation); there is Italian text (running text and the text of diagrams
and screenshots); the file map does not cover every source (this fails at generation); a cited
file or function does not exist; a GENERATED placeholder is still there; a cached screenshot is
older than the binary or the fixture; doc.json differs from the binary's `--doc-json`.
"""
import base64
import hashlib
import html
import importlib.util
import json
import math
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import unicodedata

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent.parent
DATE = "October 2026"
NAME = "ngtop"
# ngtop's version: the one of Cargo.toml.
VERSION = re.search(r'^version\s*=\s*"([0-9.]+)"', (ROOT / "Cargo.toml").read_text(), re.M).group(1)

MANUALS = {
    "technical": dict(dir="technical", file="Technical Manual.html", title="Technical Manual — ngtop",
                      h1="Technical Manual"),
    "user": dict(dir="user", file="User Manual.html", title="User Manual — ngtop", h1="User Manual"),
}
ALIASES = {"tecnico": "technical", "utente": "user"}

# After the canon (style.css), only the rules for elements that exist only in the project manuals
# (terminals, tree, pills, glossary: the same additions as REMOTIX/Phonestra) and ngtop's screenshots.
EXTRA_CSS = """
/* ngtop: terminals (title, prompt, program lines, errors), commands inside procedures,
   folder tree, status pills, glossary, group rows in tables, screenshots */
.steps .code-w{margin:10px 0 4px}
.code .pr{color:#93c5fd} .code .cmd{color:#fff;font-weight:700} .code .am{color:#fdba74;font-weight:600}
.code .er{color:#fca5a5} .code .dim{color:#94a3b8}
.code-t{display:block;font-family:Outfit,Segoe UI,Arial,sans-serif;font-size:11px;font-weight:700;letter-spacing:.12em;text-transform:uppercase;color:#64748b;margin:16px 0 -8px}
.tree{background:transparent;border:0;padding:4px 8px;font-family:ui-monospace,Menlo,Consolas,monospace;font-size:13px;line-height:1.6;color:#0f172a;overflow-x:auto;margin:0;text-align:left}
.tree .d{color:#0050C0;font-weight:700} .tree .m{color:#64748b}
.pill{display:inline-block;font-size:12px;font-weight:700;border-radius:999px;padding:.1em .65em;white-space:nowrap}
.p-ok{background:#dcfce7;color:#166534} .p-wait{background:#dbeafe;color:#1e40af} .p-snooze{background:#fef3c7;color:#92400e}
.p-off{background:#e2e8f0;color:#475569} .p-info{background:#f1f5f9;color:#334155;border:1px solid #cbd5e1}
.gloss{margin:12px 0} .gloss dt{font-weight:700;color:#003a90;font-size:15px;margin-top:12px} .gloss dd{margin:2px 0 0;color:#334155;font-size:14px}
.tbl-group td{background:#eef2f7 !important;font-weight:700;color:#003a90}
.snap svg{display:block;margin:0 auto;width:100%;height:auto}
.snap .cap{margin-top:10px}
.gen-missing{margin:16px 0;padding:14px 16px;border:2px dashed #d97706;border-radius:10px;background:#fffbeb;color:#92400e;font-weight:700;font-family:ui-monospace,Menlo,Consolas,monospace;font-size:13px}
"""

# ── Text ────────────────────────────────────────────────────────────────
def esc(t):
    return html.escape(t, quote=False)


def c(t):
    return f"<code>{esc(t)}</code>"


def sysf(t):
    """A file or unit of the machine, not of the repository (/proc/stat, config.toml):
    shown as code, but the check does not look for it among the sources."""
    return f'<code class="sys">{esc(t)}</code>'


def ui(label):
    """Exact label of a screen element or a key. Long labels (whole messages) wrap,
    so as not to overflow the tables."""
    stile = ' style="white-space:normal"' if len(label) > 40 else ""
    return f'<span class="ui"{stile}>{esc(label)}</span>'


def key(*keys):
    return "+".join(f"<kbd>{esc(k)}</kbd>" for k in keys)


def pill(text, kind):
    return f'<span class="pill p-{kind}">{esc(text)}</span>'


def p(t, lead=False):
    return f'<p class="lead">{t}</p>' if lead else f"<p>{t}</p>"


def h4(t):
    return f"<h4>{t}</h4>"


def ul(items):
    return '<ul class="ul">' + "".join(f"<li>{i}</li>" for i in items) + "</ul>"


def ol(items):
    return '<ol class="ol">' + "".join(f"<li>{i}</li>" for i in items) + "</ol>"


def steps(items):
    return '<ol class="steps">' + "".join(f"<li>{i}</li>" for i in items) + "</ol>"


def note(t, title="Note."):
    return f'<div class="callout c-info"><div class="callout-i">i</div><div><b>{title}</b> {t}</div></div>'


def warn(t, title="Warning."):
    return f'<div class="callout c-warn"><div class="callout-i">!</div><div><b>{title}</b> {t}</div></div>'


def tip(t, title="Tip."):
    return f'<div class="callout c-ok"><div class="callout-i">+</div><div><b>{title}</b> {t}</div></div>'


def rif(titolo):
    """Link to a section of the manual, by title; number() resolves it."""
    return f"«RIF:{titolo}»"


def table(head, rows, cap, tid=None):
    """rows: lists of cells; a single string acts as a group row across the full width."""
    th = "".join(f"<th>{h}</th>" for h in head)
    tr = []
    for r in rows:
        if isinstance(r, str):
            tr.append(f'<tr class="tbl-group"><td colspan="{len(head)}">{r}</td></tr>')
        else:
            tr.append("<tr>" + "".join(f"<td>{x}</td>" for x in r) + "</tr>")
    idattr = f' id="{tid}"' if tid else ""
    return (f'<figure class="tbl-wrap"><table class="tbl"{idattr}><thead><tr>{th}</tr></thead>'
            f'<tbody>{"".join(tr)}</tbody></table><figcaption class="cap">{cap}</figcaption></figure>')


def dl(pairs, cls="deflist"):
    return f'<dl class="{cls}">' + "".join(f"<dt>{a}</dt><dd>{b}</dd>" for a, b in pairs) + "</dl>"


def code(text, lang="bash", title=""):
    from pygments import lex
    from pygments.lexers import (BashLexer, JavaLexer, JsonLexer, PythonLexer, RustLexer, TextLexer, TOMLLexer)
    from pygments.token import Comment, Keyword, String
    lx = {"bash": BashLexer, "java": JavaLexer, "json": JsonLexer, "python": PythonLexer, "rust": RustLexer,
          "toml": TOMLLexer, "text": TextLexer}[lang]()
    out = []
    for tok, val in lex(text.strip("\n"), lx):
        e = html.escape(val, quote=True)
        cls = "cm" if tok in Comment else "st" if tok in String else "k" if tok in Keyword else None
        if cls and val.strip():
            e = "\n".join(f'<span class="{cls}">{x}</span>' if x else x for x in e.split("\n"))
        out.append(e)
    head = f'<span class="code-t">{esc(title)}</span>' if title else ""
    return head + '<pre class="code"><code>' + "".join(out).rstrip("\n") + "</code></pre>"


PROMPT = re.compile(r"^([$#](?: |$))(.*)$")


def term(text, title=""):
    """A terminal session: prompts and commands, program lines, errors, comments."""
    lines = []
    for line in text.strip("\n").split("\n"):
        m = PROMPT.match(line)
        if m:
            cmd, _, comment = m.group(2).partition("   #")
            lines.append(f'<span class="pr">{esc(m.group(1))}</span><span class="cmd">{esc(cmd)}</span>'
                         + (f'<span class="dim">   #{esc(comment)}</span>' if comment else ""))
        elif line.startswith("error") or line.startswith("Error"):
            lines.append(f'<span class="er">{esc(line)}</span>')
        elif line.startswith("["):
            lines.append(f'<span class="am">{esc(line)}</span>')
        else:
            lines.append(esc(line))
    head = f'<span class="code-t">{esc(title)}</span>' if title else ""
    return head + '<pre class="code"><code>' + "\n".join(lines) + "</code></pre>"


def tree(lines, cap):
    """Folder tree: lines 'path  # comment'."""
    out = []
    for line in lines:
        path, _, comment = line.partition("  #")
        m = re.match(r"^([\s│├└─]*)(.*)$", path)
        pre, name = m.group(1), m.group(2)
        cls = "d" if name.rstrip().endswith("/") else ""
        out.append(esc(pre) + (f'<span class="{cls}">{esc(name)}</span>' if cls else esc(name))
                   + (f'<span class="m">  #{esc(comment)}</span>' if comment else ""))
    return f'<figure class="fig"><pre class="tree">' + "\n".join(out) + f'</pre><figcaption class="cap">{cap}</figcaption></figure>'


# ── SVG figures in the style of IR_Service ─────────────────────────────
COL = {"dark": "#475569", "blue": "#0050C0", "light": "#3b82f6", "navy": "#003a90", "soft": "#eef2f7",
       "green": "#16a34a", "amber": "#d97706", "grey": "#94a3b8", "white": "#ffffff"}
FONT = 'font-family="Outfit,Segoe UI,Arial,sans-serif"'


def box(x, y, w, h, title, sub="", color="blue", size=13):
    fill = COL[color]
    light = color in ("soft", "white")
    fg = "#334155" if light else "#fff"
    stroke = ' stroke="#cbd5e1"' if light else ""
    cy = y + h / 2
    s = f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="9" fill="{fill}"{stroke}/>'
    if sub:
        s += (f'<text x="{x + w / 2}" y="{cy - 7}" text-anchor="middle" dominant-baseline="middle" '
              f'font-size="{size}" font-weight="700" fill="{fg}">{esc(title)}</text>'
              f'<text x="{x + w / 2}" y="{cy + 12}" text-anchor="middle" font-size="10.5" fill="{fg}" opacity="0.88">{esc(sub)}</text>')
    else:
        s += (f'<text x="{x + w / 2}" y="{cy}" text-anchor="middle" dominant-baseline="middle" '
              f'font-size="{size}" font-weight="700" fill="{fg}">{esc(title)}</text>')
    return s


def diamond(cx, cy, w, h, text, color="amber"):
    pts = f"{cx},{cy - h / 2} {cx + w / 2},{cy} {cx},{cy + h / 2} {cx - w / 2},{cy}"
    return (f'<polygon points="{pts}" fill="{COL[color]}"/>'
            f'<text x="{cx}" y="{cy}" text-anchor="middle" dominant-baseline="middle" font-size="12.5" '
            f'font-weight="700" fill="#fff">{esc(text)}</text>')


def _head(x2, y2, ang, color):
    a1 = (x2 - 8 * math.cos(ang) + 5 * math.sin(ang), y2 - 8 * math.sin(ang) - 5 * math.cos(ang))
    a2 = (x2 - 8 * math.cos(ang) - 5 * math.sin(ang), y2 - 8 * math.sin(ang) + 5 * math.cos(ang))
    return f'<path d="M{a1[0]:.1f} {a1[1]:.1f} L{x2} {y2} L{a2[0]:.1f} {a2[1]:.1f} Z" fill="{color}"/>'


def arrow(x1, y1, x2, y2, color="#0050C0", dash=False, label="", lx=None, ly=None):
    ang = math.atan2(y2 - y1, x2 - x1)
    d = ' stroke-dasharray="5 4"' if dash else ""
    s = (f'<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{color}" stroke-width="2.2"{d}/>'
         + _head(x2, y2, ang, color))
    if label:
        s += text((x1 + x2) / 2 if lx is None else lx, (y1 + y2) / 2 - 7 if ly is None else ly, label, 11, "#334155")
    return s


def path(points, color="#0050C0", dash=False, label="", lx=0, ly=0):
    """Polyline with a final arrow: points = [(x, y), ...]."""
    d = " ".join(("M" if i == 0 else "L") + f"{x} {y}" for i, (x, y) in enumerate(points))
    dd = ' stroke-dasharray="5 4"' if dash else ""
    (xa, ya), (xb, yb) = points[-2], points[-1]
    s = (f'<path d="{d}" fill="none" stroke="{color}" stroke-width="2.2"{dd}/>'
         + _head(xb, yb, math.atan2(yb - ya, xb - xa), color))
    if label:
        s += text(lx, ly, label, 11, "#334155")
    return s


def text(x, y, t, size=12, color="#334155", weight="400", anchor="middle", halo=True):
    """Label; the light outline sets it apart from the lines it crosses."""
    h = ' stroke="#f8fafc" stroke-width="4" stroke-linejoin="round" paint-order="stroke"' if halo else ""
    return (f'<text x="{x}" y="{y}" text-anchor="{anchor}" font-size="{size}" font-weight="{weight}" '
            f'fill="{color}"{h}>{esc(t)}</text>')


def zone(x, y, w, h, title, color="#e8eef7"):
    return (f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="12" fill="{color}" stroke="#d5deea"/>'
            + text(x + 14, y + 20, title, 11.5, "#003a90", "700", "start", False))


def fig(body, width, height, cap, title=""):
    """SVG figure; as in IR's manuals, the caption is inside the drawing, at the bottom."""
    t = text(20, 28, title, 12, "#003a90", "700", "start") if title else ""
    didascalia = html.unescape(re.sub(r"<[^>]+>", "", cap))
    height += 30
    c = text(width / 2, height - 16, didascalia, 12, "#475569", "400", "middle", False)
    return (f'<figure class="fig"><svg viewBox="0 0 {width} {height}" xmlns="http://www.w3.org/2000/svg" {FONT} '
            f'role="img"><rect width="{width}" height="{height}" fill="#f8fafc" rx="12"/>{t}{body}{c}</svg></figure>')


def flow(nodes, cap, title="", width=900):
    """Horizontal flow: nodes = [(title, subtitle, colour)]."""
    n = len(nodes)
    gap = 36
    w = (width - 40 - gap * (n - 1)) / n
    top = 50 if title else 26
    h = 64
    s = []
    for i, (t, sub, col) in enumerate(nodes):
        x = 20 + i * (w + gap)
        s.append(box(x, top, w, h, t, sub, col))
        if i < n - 1:
            s.append(arrow(x + w + 2, top + h / 2, x + w + gap - 2, top + h / 2))
    return fig("".join(s), width, top + h + 26, cap, title)


def seq(actors, events, cap, title="", width=900):
    """Sequence diagram.
    actors = [(name, subtitle, colour)];
    events = (from, to, text[, dashed]) for a message, ("note", i, text) for a note,
             ("sep", text) for a separator with text."""
    n = len(actors)
    top = 50 if title else 22
    colw = (width - 40) / n
    xs = [20 + colw * i + colw / 2 for i in range(n)]
    bw, bh = min(colw - 24, 190), 50
    y = top + bh + 34
    body = []
    for ev in events:
        if ev[0] == "note":
            _, i, t = ev
            tw = max(len(t) * 6.6 + 24, 90)
            body.append(f'<rect x="{xs[i] - tw / 2}" y="{y - 15}" width="{tw}" height="26" rx="6" fill="#fff7ed" stroke="#fdba74"/>')
            body.append(text(xs[i], y + 3, t, 11, "#9a3412", "600", "middle", False))
            y += 40
        elif ev[0] == "sep":
            body.append(f'<line x1="24" y1="{y - 4}" x2="{width - 24}" y2="{y - 4}" stroke="#cbd5e1" stroke-dasharray="2 4"/>')
            tw = len(ev[1]) * 6.4 + 22
            body.append(f'<rect x="{width / 2 - tw / 2}" y="{y - 15}" width="{tw}" height="22" rx="11" fill="#f8fafc" stroke="#cbd5e1"/>')
            body.append(text(width / 2, y + 0, ev[1], 11, "#475569", "600", "middle", False))
            y += 36
        else:
            a, b, t = ev[0], ev[1], ev[2]
            dash = len(ev) > 3 and ev[3]
            color = "#475569" if dash else "#0050C0"
            if a == b:
                x = xs[a]
                body.append(f'<path d="M{x} {y - 8} h34 v18 h-30" fill="none" stroke="{color}" stroke-width="2"/>'
                            + _head(x + 2, y + 10, math.pi, color))
                body.append(text(x + 42, y + 5, t, 11, "#334155", "400", "start"))
                y += 42
            else:
                x1, x2 = xs[a], xs[b]
                off = 5 if x2 > x1 else -5
                body.append(arrow(x1 + off, y, x2 - off, y, color, dash))
                body.append(text((x1 + x2) / 2, y - 8, t, 11, "#334155"))
                y += 42
    height = y + 10
    head = []
    for i, (name, sub, color) in enumerate(actors):
        head.append(f'<line x1="{xs[i]}" y1="{top + bh}" x2="{xs[i]}" y2="{height - 14}" stroke="#94a3b8" stroke-width="1.5" stroke-dasharray="4 4"/>')
        head.append(box(xs[i] - bw / 2, top, bw, bh, name, sub, color))
    return fig("".join(head + body), width, height, cap, title)


# ── Sources: file map and numbers ────────────────────────────────────────
# The product files the manual describes, with their line counts. Not the fixtures (hundreds of
# tiny files: the manual describes them by folder), not the generated copies (site/public), not
# the build products (target/, dist/), not the images and the cached screenshots.
SOURCE_PATTERNS = [
    ("src", (".rs", ".sh")),
    ("tests", (".rs",)),
    ("scripts", None),
    ("site/landing", (".html", ".css", ".js")),
    ("docs/sources", (".py", ".css", ".js")),
]
SOURCE_FILES = ["build.sh", "Cargo.toml", "site/publish.sh", "site/licence-page.py"]


def sorgenti():
    """{path: lines} of the sources, relative to the repository root."""
    found = []
    for base, exts in SOURCE_PATTERNS:
        for f in sorted((ROOT / base).rglob("*")):
            rel = f.relative_to(ROOT).as_posix()
            if f.is_file() and "__pycache__" not in f.parts and "/fixtures/" not in rel \
                    and "/snaps/" not in rel and (exts is None or f.suffix in exts):
                found.append(rel)
    found += [f for f in SOURCE_FILES if (ROOT / f).is_file()]
    return {f: (ROOT / f).read_bytes().count(b"\n") for f in dict.fromkeys(found)}


def righe(n):
    return f"{n:,}"


def file_map(groups, cap):
    """File map: groups = [(group, [(path, role)])]. Fails if a
    source is missing from the map or if the map cites a file that no longer exists."""
    tutti = sorgenti()
    citati = [f for _, rows in groups for f, _ in rows]
    mancano = sorted(set(tutti) - set(citati))
    in_excess = sorted(set(citati) - set(tutti))
    if mancano or in_excess:
        raise SystemExit("the manual's file map needs updating (technical/ chapter with file_map()):"
                         + "".join(f"\n  missing {f}" for f in mancano)
                         + "".join(f"\n  no longer exists {f}" for f in in_excess))
    out = []
    for gruppo, rows in groups:
        out.append(gruppo)
        for f, role in rows:
            out.append([c(f), righe(tutti[f]), role])
    return table(["File", "Lines", "Role"], out, cap, "mappa-file")


def numeri(parti, cap):
    """Table «the project in numbers»: parti = [(name, where, contents, rule)];
    each file goes into the first part whose rule accepts it."""
    conti = [0] * len(parti)
    for f, n in sorgenti().items():
        for i, (_, _, _, regola) in enumerate(parti):
            if regola(f):
                conti[i] += n
                break
    rows = [[nome, dove, righe(conti[i]), cont] for i, (nome, dove, cont, _) in enumerate(parti)]
    rows.append(["<b>Total</b>", "", f"<b>{righe(sum(conti))}</b>", ""])
    return table(["Part", "Where", "Lines", "Contents"], rows, cap)


def conta(estensione):
    """Lines of all the files of one type (for the cover of the first chapter)."""
    return sum(n for f, n in sorgenti().items() if f.endswith(estensione))


# ── Generated content: keys, options ─────────────────────────────────────
DOC_JSON = HERE / "doc.json"
GENERATED = []      # problems found while generating (missing screenshots, missing doc.json…)


def gen_missing(what):
    """A visible placeholder for content that is generated from the program and is not available."""
    GENERATED.append(what)
    return f'<div class="gen-missing">GENERATED: {esc(what)}</div>'


CHECKING = False
DOC_STALE = []
_doc_fresh = []


def _binary_doc():
    """The output of `ngtop --doc-json` (once per run), or None if the binary does not have it."""
    if not _doc_fresh:
        data = None
        if binary_supports("--doc-json"):
            out = subprocess.run([str(BINARY()), "--doc-json"], capture_output=True, text=True)
            if out.returncode == 0:
                data = json.loads(out.stdout)
        _doc_fresh.append(data)
    return _doc_fresh[0]


def _write_doc(data):
    DOC_JSON.write_text(json.dumps(data, indent=1, ensure_ascii=False, sort_keys=True) + "\n", encoding="utf-8")


def doc_data():
    """The program's keys and options: from the binary (rewriting doc.json if it changed, except
    while checking) or, without the binary, from doc.json; None if there is neither."""
    fresh = _binary_doc()
    old = json.loads(DOC_JSON.read_text(encoding="utf-8")) if DOC_JSON.is_file() else None
    if fresh is not None:
        if old != fresh:
            if CHECKING:
                if "doc.json" not in DOC_STALE:
                    DOC_STALE.append("doc.json")
            else:
                _write_doc(fresh)
        return fresh
    return old


def _tokens(k):
    out = []
    for t in k.replace("←→", "← →").split():
        out += [t] if t in ("/", "+/-") else t.split("/")
    return {x for x in out if x}


def _kbds(k):
    return " ".join(f"<kbd>{esc(t)}</kbd>" for t in k.replace("←→", "← →").split())


def _cap1(t):
    return t[:1].upper() + t[1:]


def keys_table(section, cap=None):
    """Table Key / Action of a screen ("Overview", "GPU", "Processes", "Network & Disks", "Services")
    or of "global", "compact" (80x24 format) or "filter" (while typing a filter)."""
    d = doc_data()
    if d is None:
        return gen_missing(f"keys of {section}")
    extra = {"global": ("global_keys", "Keys valid on every screen"),
             "compact": ("compact_keys", "Keys shown in the compact format"),
             "filter": ("filter_keys", "Keys while typing a filter")}
    if section in extra:
        rows = [[_kbds(k), esc(_cap1(h))] for k, h in d.get(extra[section][0], [])]
        title = extra[section][1]
    else:
        match = [x for x in d.get("sections", []) if x["name"] == section]
        if not match:
            return gen_missing(f"keys of {section} (no such section in the program)")
        sec = match[0]
        covered = set()
        for k, _ in sec.get("help", []) + d.get("global_keys", []):
            covered |= _tokens(k)
        pairs = list(sec.get("help", []))
        pairs += [(k, h) for k, h in sec.get("keys", []) if not (_tokens(k) & covered)]
        rows = [[_kbds(k), esc(_cap1(h))] for k, h in pairs]
        title = f"Keys of the {section} screen"
    return table(["Key", "Action"], rows, cap or f"«TAB» — {title}")


def options_table(cap=None):
    """Table of the command-line options."""
    d = doc_data()
    if d is None:
        return gen_missing("command-line options")
    rows = [[c(o["flag"]), esc(_cap1(o["help"]))] for o in d.get("options", [])]
    return table(["Option", "Meaning"], rows, cap or "«TAB» — Command-line options")


def binary_supports(option):
    b = BINARY()
    return bool(b) and option.encode() in b.read_bytes()


def refresh_doc():
    """Rewrites doc.json from the binary's hidden option --doc-json."""
    data = _binary_doc()
    if data is None:
        raise SystemExit("dist/ngtop does not have the hidden option --doc-json")
    _write_doc(data)
    print("written", DOC_JSON.relative_to(ROOT))


# ── Real screens of ngtop (snap) ─────────────────────────────────────────
SNAP_VERSION = 1                       # raise it when the SVG conversion changes
SNAP_DIR = HERE / "snaps"
FIXTURES = ROOT / "tests" / "fixtures"
SCREENS = {"overview": (1, None), "gpu": (2, None), "processes": (3, None), "network": (4, None),
           "services": (5, None), "compact": (1, None)}
CW, CH = 7.8, 16.5                     # size of a terminal cell in the SVG
DEF_FG, DEF_BG = "#c9d1d9", "#0b0f14"  # the colours of the interface (theme.rs: FG, DARK)
MONO = "ui-monospace,'DejaVu Sans Mono',Menlo,Consolas,'Liberation Mono',monospace"
DEFAULT_SIGNAL_PID = 2210             # firefox in the demo fixture
STALE = []                             # cached screenshots older than the binary / fixture


def BINARY():
    b = ROOT / "dist" / "ngtop"
    return b if b.is_file() else None


_hash_cache = {}


def _file_hash(path):
    st = path.stat()
    k = (str(path), st.st_mtime_ns, st.st_size)
    if k not in _hash_cache:
        _hash_cache[k] = hashlib.sha256(path.read_bytes()).hexdigest()
    return _hash_cache[k]


def _tree_hash(d):
    h = hashlib.sha256()
    for f in sorted(d.rglob("*")):
        rel = f.relative_to(d).as_posix()
        if f.is_symlink():
            h.update(f"L {rel} {os.readlink(f)}\n".encode())
        elif f.is_file():
            h.update(f"F {rel} {_file_hash(f)}\n".encode())
    return h.hexdigest()


class DemoAnimator(threading.Thread):
    """Makes a copy of the demo fixture move while ngtop takes its snapshot: it rewrites, every
    few tens of ms, the cumulative counters (CPU ticks, bytes, sectors, GPU engine time) adding
    to them the rates of animate.json. Files are replaced atomically."""

    HZ = 100

    def __init__(self, root):
        super().__init__(daemon=True)
        import random
        self.rng = random.Random(1)
        self.root = pathlib.Path(root)
        self.cfg = json.loads((self.root / "animate.json").read_text())
        self.halt = threading.Event()
        r = self.root
        self.cpu = []                   # [label, [fields as float]]
        for line in (r / "proc/stat").read_text().splitlines():
            f = line.split()
            if f and f[0].startswith("cpu"):
                self.cpu.append([f[0], [float(x) for x in f[1:]]])
        self.procs = {}                 # pid -> (head, tokens, rate)
        for pid, rate in self.cfg["proc_cpu"].items():
            t = (r / f"proc/{pid}/stat").read_text()
            i = t.rfind(")") + 1
            tok = t[i:].split()
            self.procs[pid] = [t[:i], tok, rate, [float(tok[11]), float(tok[12])]]
        self.net = {}
        for line in (r / "proc/net/dev").read_text().splitlines():
            if ":" in line and not line.startswith(("Inter", " face")):
                name, rest = line.split(":", 1)
                self.net[name.strip()] = [float(x) for x in rest.split()]
        self.disk = {}
        for line in (r / "proc/diskstats").read_text().splitlines():
            f = line.split()
            if len(f) > 13:
                self.disk[f[2]] = f[:3], [float(x) for x in f[3:]]
        self.gpu = {}
        for rel, eng in self.cfg["gpu_fdinfo"].items():
            self.gpu[rel] = [(r / f"proc/{rel}").read_text(), eng, 0.0]

    @staticmethod
    def put(path, text):
        tmp = path.with_name(path.name + ".tmp")
        tmp.write_text(text)
        os.replace(tmp, path)

    def step(self, dt):
        rnd, r, T = self.rng.uniform, self.root, dt * self.HZ
        # CPU: user nice system idle iowait irq softirq …
        tot = [0.0] * 10
        for n, (label, f) in enumerate(self.cpu[1:]):
            busy = min(0.99, max(0.02, self.cfg["cpu_busy"][n] * rnd(0.6, 1.3)))
            f[0] += T * busy * 0.8
            f[2] += T * busy * 0.2
            f[3] += T * (1 - busy)
            for k in range(len(f)):
                tot[k] += f[k]
        self.cpu[0][1] = tot
        self.put(r / "proc/stat", "".join(f"{label} " + " ".join(str(int(x)) for x in f) + "\n"
                                           for label, f in self.cpu) + "intr 1\nbtime 1\n")
        # processes
        for pid, (head, tok, rate, acc) in self.procs.items():
            if rate <= 0:
                continue
            add = T * rate * rnd(0.7, 1.3)
            acc[0] += add * 0.8
            acc[1] += add * 0.2
            tok[11], tok[12] = str(int(acc[0])), str(int(acc[1]))
            self.put(r / f"proc/{pid}/stat", head + " " + " ".join(tok) + "\n")
        # network
        lines = ["Inter-|   Receive", " face |bytes    packets"]
        for name, f in self.net.items():
            rx, tx = self.cfg["net"].get(name, [0, 0])
            f[0] += rx * dt * rnd(0.3, 1.7)
            f[1] += rx * dt / 1200
            f[8] += tx * dt * rnd(0.3, 1.7)
            f[9] += tx * dt / 900
            lines.append(f"{name:>6}: " + " ".join(str(int(x)) for x in f))
        self.put(r / "proc/net/dev", "\n".join(lines) + "\n")
        # disks
        out = []
        for name, (head, f) in self.disk.items():
            if name in self.cfg["disk"]:
                rd, wr = self.cfg["disk"][name]
                rb, wb = rd * dt * rnd(0.4, 1.6), wr * dt * rnd(0.4, 1.6)
                f[2] += rb / 512
                f[0] += rb / 65536
                f[3] += rb / 65536 * 0.25
                f[6] += wb / 512
                f[4] += wb / 65536
                f[7] += wb / 65536 * 0.25
                f[9] += dt * 1000 * (0.3 if name.startswith("nvme") else 0.45)
            out.append(" ".join(head) + " " + " ".join(str(int(x)) for x in f))
        self.put(r / "proc/diskstats", "\n".join(out) + "\n")
        # GPU
        for rel, (txt, eng, acc) in list(self.gpu.items()):
            def bump(m):
                name, ns = m.group(1), float(m.group(2))
                return f"drm-engine-{name}:\t{int(ns + eng.get(name, 0) * dt * 1e9 * rnd(0.7, 1.3))} ns"
            new = re.sub(r"drm-engine-(\w+):\t(\d+) ns", bump, txt)
            self.gpu[rel][0] = new
            self.put(r / f"proc/{rel}", new)
        lo, hi = self.cfg["gpu_busy"]
        d = r / "sys/class/drm/card0/device"
        self.put(d / "gpu_busy_percent", f"{int(rnd(lo, hi))}\n")
        lo, hi = self.cfg["gpu_power_w"]
        self.put(d / "hwmon/hwmon2/power1_average", f"{int(rnd(lo, hi) * 1e6)}\n")

    def run(self):
        last = time.monotonic()
        while not self.halt.wait(0.03):
            now = time.monotonic()
            try:
                self.step(now - last)
            except (OSError, ValueError):
                pass
            last = now


def run_ngtop(screen, size, roots, extra, overlay=None, select_pid=None):
    """Runs the binary on a living copy of the fixture; returns the ANSI text."""
    n = SCREENS[screen][0]
    SNAP_DIR.mkdir(exist_ok=True)
    work = pathlib.Path(tempfile.mkdtemp(prefix=".work-", dir=SNAP_DIR))
    try:
        copy = work / roots
        shutil.copytree(FIXTURES / roots, copy, symlinks=True)
        anim = DemoAnimator(copy) if (copy / "animate.json").is_file() else None
        if anim:
            anim.start()
            time.sleep(0.2)
        try:
            cmd = [str(BINARY()), "--snapshot", size, "--section", str(n), "--ansi", "--roots", str(copy)] + extra
            if select_pid:
                cmd += ["--select-pid", str(select_pid)]
            if overlay:
                cmd += ["--overlay", overlay]
            res = subprocess.run(cmd, capture_output=True, text=True, timeout=120, cwd=ROOT)
        finally:
            if anim:
                anim.halt.set()
                anim.join()
        if res.returncode != 0:
            raise SystemExit(f"{' '.join(cmd)}: exit {res.returncode}\n{res.stderr}")
        return res.stdout
    finally:
        shutil.rmtree(work, ignore_errors=True)


SGR = re.compile(r"\x1b\[([0-9;]*)m")


def parse_ansi(text):
    """Rows of cells (char, fg, bg, bold) from truecolor ANSI text; fg/bg are '#rrggbb' or None."""
    rows = []
    for line in text.split("\n"):
        fg = bg = None
        bold = False
        row, pos = [], 0
        for m in SGR.finditer(line):
            row += [(ch, fg, bg, bold) for ch in line[pos:m.start()]]
            pos = m.end()
            a = [int(x) if x else 0 for x in m.group(1).split(";")]
            i = 0
            while i < len(a):
                v = a[i]
                if v == 0:
                    fg = bg = None
                    bold = False
                elif v == 1:
                    bold = True
                elif v in (38, 48) and i + 4 < len(a) + 0 and a[i + 1] == 2:
                    col = "#%02x%02x%02x" % tuple(a[i + 2:i + 5])
                    if v == 38:
                        fg = col
                    else:
                        bg = col
                    i += 4
                elif v == 39:
                    fg = None
                elif v == 49:
                    bg = None
                i += 1
        row += [(ch, fg, bg, bold) for ch in line[pos:]]
        rows.append(row)
    while rows and not rows[-1]:
        rows.pop()
    return rows


# Box-drawing characters: which sides of the cell the line goes to (up, down, left, right), rounded corners.
BOX = {"─": "lr", "│": "ud", "┌": "dr", "┐": "dl", "└": "ur", "┘": "ul", "├": "udr", "┤": "udl", "┬": "dlr",
       "┴": "ulr", "┼": "udlr", "╭": "dr!", "╮": "dl!", "╰": "ur!", "╯": "ul!", "═": "lr", "║": "ud",
       "╔": "dr", "╗": "dl", "╚": "ur", "╝": "ul", "╠": "udr", "╣": "udl", "╦": "dlr", "╩": "ulr", "╬": "udlr"}
BLOCK_H = {"▁": 1, "▂": 2, "▃": 3, "▄": 4, "▅": 5, "▆": 6, "▇": 7, "█": 8}
BRAILLE_BITS = [(0, 0, 0x01), (0, 1, 0x02), (0, 2, 0x04), (1, 0, 0x08), (1, 1, 0x10), (1, 2, 0x20), (0, 3, 0x40), (1, 3, 0x80)]


def _n(v):
    s = f"{v:.1f}"
    return s[:-2] if s.endswith(".0") else s


def screen_svg(rows, cols):
    """The cells as SVG shapes and text (group of cols×rows cells, origin 0,0)."""
    rects, paths, strokes, texts = {}, {}, {}, []
    for y, row in enumerate(rows):
        # background runs
        x = 0
        while x < len(row):
            bg = row[x][2]
            e = x
            while e < len(row) and row[e][2] == bg:
                e += 1
            if bg:
                rects.setdefault(bg, []).append(f"M{_n(x * CW)} {_n(y * CH)}h{_n((e - x) * CW + 0.4)}v{_n(CH + 0.4)}h-{_n((e - x) * CW + 0.4)}z")
            x = e
        # glyph cells: shapes are drawn as paths, the rest is grouped in text runs
        cells = []
        for x, (ch, fg, bg, bold) in enumerate(row):
            o = ord(ch)
            col = fg or DEF_FG
            x0, y0 = x * CW, y * CH
            if 0x2800 <= o <= 0x28FF:
                bits = o - 0x2800
                for dx, dy, bit in BRAILLE_BITS:
                    if bits & bit:
                        w_, h_ = CW * 0.30, CH * 0.15
                        px = x0 + CW * (0.17 + 0.46 * dx)
                        py = y0 + CH * (0.08 + 0.235 * dy)
                        paths.setdefault(col, []).append(f"M{_n(px)} {_n(py)}h{_n(w_)}v{_n(h_)}h-{_n(w_)}z")
                ch = " "
            elif ch in BLOCK_H:
                h_ = CH * BLOCK_H[ch] / 8
                paths.setdefault(col, []).append(f"M{_n(x0)} {_n(y0 + CH - h_)}h{_n(CW + 0.3)}v{_n(h_ + 0.3)}h-{_n(CW + 0.3)}z")
                ch = " "
            elif ch in "■▪":
                s_ = CW * 0.62
                paths.setdefault(col, []).append(f"M{_n(x0 + (CW - s_) / 2)} {_n(y0 + (CH - s_) / 2)}h{_n(s_)}v{_n(s_)}h-{_n(s_)}z")
                ch = " "
            elif ch in "▀▌▐":
                ox, w_, h_ = {"▀": (0, CW, CH / 2), "▌": (0, CW / 2, CH), "▐": (CW / 2, CW / 2, CH)}[ch]
                paths.setdefault(col, []).append(f"M{_n(x0 + ox)} {_n(y0)}h{_n(w_ + 0.3)}v{_n(h_ + 0.3)}h-{_n(w_ + 0.3)}z")
                ch = " "
            elif ch in BOX:
                sides = BOX[ch].rstrip("!")
                cx, cy, r = x0 + CW / 2, y0 + CH / 2, min(CW, CH) / 2
                d = []
                if BOX[ch].endswith("!"):
                    v, hz = ("d" if "d" in sides else "u"), ("r" if "r" in sides else "l")
                    sy, sx = (1 if v == "d" else -1), (1 if hz == "r" else -1)
                    ey, ex = (y0 + CH if v == "d" else y0), (x0 + CW if hz == "r" else x0)
                    d.append(f"M{_n(cx)} {_n(ey)}V{_n(cy + sy * r)}Q{_n(cx)} {_n(cy)} {_n(cx + sx * r)} {_n(cy)}H{_n(ex)}")
                else:
                    if "u" in sides:
                        d.append(f"M{_n(cx)} {_n(y0)}V{_n(cy)}")
                    if "d" in sides:
                        d.append(f"M{_n(cx)} {_n(cy)}V{_n(y0 + CH)}")
                    if "l" in sides:
                        d.append(f"M{_n(x0)} {_n(cy)}H{_n(cx)}")
                    if "r" in sides:
                        d.append(f"M{_n(cx)} {_n(cy)}H{_n(x0 + CW)}")
                strokes.setdefault(col, []).append("".join(d))
                ch = " "
            elif unicodedata.east_asian_width(ch) in "WF":
                ch = "?"
            cells.append((ch, fg, bold))
        i = 0
        while i < len(cells):
            if cells[i][0] == " ":
                i += 1
                continue
            style = cells[i][1:]
            j = i + 1
            while j < len(cells):
                if cells[j][0] != " " and cells[j][1:] == style:
                    j += 1
                elif cells[j][0] == " " and j + 1 < len(cells) and cells[j + 1][0] != " " and cells[j + 1][1:] == style:
                    j += 2
                else:
                    break
            body = "".join(ch for ch, _, _ in cells[i:j]).rstrip()
            fg, bold = style
            texts.append(
                f'<text x="{_n(i * CW)}" y="{_n(y * CH + CH * 0.76)}" fill="{fg or DEF_FG}"'
                f'{" font-weight=\"700\"" if bold else ""} textLength="{_n(len(body) * CW)}" lengthAdjust="spacingAndGlyphs"'
                f' xml:space="preserve">{esc(body)}</text>')
            i = j
    out = []
    for col, d in rects.items():
        out.append(f'<path fill="{col}" d="{"".join(d)}"/>')
    for col, d in paths.items():
        out.append(f'<path fill="{col}" d="{"".join(d)}"/>')
    for col, d in strokes.items():
        out.append(f'<path fill="none" stroke="{col}" stroke-width="1.15" stroke-linecap="butt" d="{"".join(d)}"/>')
    out += texts
    return "".join(out)


def snap_cell_svg(screen, size, roots, overlay=None, select_pid=None):
    """The cached SVG fragment of a screen: (cols, rows, fragment). Regenerated if the binary or the fixture changed."""
    cols, rows_n = (int(x) for x in size.split("x"))
    name = SNAP_DIR / f"{screen}_{size}_{roots}{'_' + overlay if overlay else ''}{'_p' + str(select_pid) if select_pid else ''}.svg"
    extra = SCREENS[screen][1]
    b = BINARY()
    key = None
    if b is not None:
        key = hashlib.sha256("|".join([_file_hash(b), _tree_hash(FIXTURES / roots), str(SNAP_VERSION), screen, size, overlay or "", str(select_pid or "")]).encode()).hexdigest()[:20]
    cached = name.read_text() if name.is_file() else None
    m = re.match(r"<!-- key=(\w+) cols=(\d+) rows=(\d+) -->\n", cached or "")
    if cached and m and m.group(1) == key:
        return int(m.group(2)), int(m.group(3)), cached[m.end():]
    if b is not None and (extra is None or binary_supports(extra)):
        ansi = run_ngtop(screen, size, roots, [extra] if extra else [], overlay, select_pid)
        rows = parse_ansi(ansi)
        frag = screen_svg(rows, cols)
        SNAP_DIR.mkdir(exist_ok=True)
        name.write_text(f"<!-- key={key} cols={cols} rows={len(rows)} -->\n{frag}")
        return cols, len(rows), frag
    if cached and m:                       # no binary: the old drawing, flagged as stale
        STALE.append(f"{name.name}")
        return int(m.group(2)), int(m.group(3)), cached[m.end():]
    return None


def snap(screen, cap, size="120x36", roots="demo", title="", overlay=None, select_pid=None):
    """Figure with a REAL screen of ngtop (see the docstring of this file)."""
    if screen not in SCREENS:
        raise SystemExit(f"snap(): unknown screen «{screen}» (one of {', '.join(SCREENS)})")
    if screen == "compact":
        size = "80x24"
    if overlay not in (None, "help", "signal", "confirm", "filter"):
        raise SystemExit(f"snap(): unknown overlay «{overlay}»")
    if select_pid is None and overlay == "signal":
        select_pid = DEFAULT_SIGNAL_PID     # a believable user process (firefox), not PID 1
    got = snap_cell_svg(screen, size, roots, overlay, select_pid)
    if got is None:
        why = f"screen «{screen}» {size}{' overlay ' + overlay if overlay else ''} (build dist/ngtop first)"
        return gen_missing(why) + f'<figcaption class="cap">{cap}</figcaption>'
    cols, rows_n, frag = got
    pad, bar = 14, 34
    w = cols * CW + 2 * pad
    h = rows_n * CH + bar + pad + 8
    ttl = title or f"ngtop — {screen if screen != 'compact' else 'overview'}{' — ' + overlay if overlay else ''} — {cols}×{rows_n}"
    dots = "".join(f'<circle cx="{22 + 18 * i}" cy="{bar / 2}" r="5.5" fill="{c_}"/>' for i, c_ in enumerate(("#ff5f57", "#febc2e", "#28c840")))
    svg = (f'<svg viewBox="0 0 {_n(w)} {_n(h)}" xmlns="http://www.w3.org/2000/svg" role="img" '
           f'aria-label="{esc(ttl)}" style="max-width:{int(w)}px;font-family:{MONO};font-size:13px">'
           f'<rect x="0.5" y="0.5" width="{_n(w - 1)}" height="{_n(h - 1)}" rx="11" fill="{DEF_BG}" stroke="#30363d"/>'
           f'<path d="M0.5 {bar}V11.5a11 11 0 0 1 11-11H{_n(w - 11.5)}a11 11 0 0 1 11 11V{bar}z" fill="#161b22"/>'
           f'{dots}<text x="{_n(w / 2)}" y="{bar / 2 + 4.5}" text-anchor="middle" fill="#8b949e" '
           f'style="font-family:Outfit,Segoe UI,Arial,sans-serif;font-size:12.5px">{esc(ttl)}</text>'
           f'<g transform="translate({pad} {bar + 8})">{frag}</g></svg>')
    return f'<figure class="fig snap">{svg}<figcaption class="cap">{cap}</figcaption></figure>'


# ── Cover ──────────────────────────────────────────────────────────────
def logo():
    """ngtop's logo (docs/sources/logo.svg: dark terminal square with a green-cyan chart line),
    embedded in the page as an image with the name next to it."""
    svg = (HERE / "logo.svg").read_text()
    dati = base64.b64encode(svg.encode()).decode()
    return f'<div class="cover-logo"><img src="data:image/svg+xml;base64,{dati}" alt="ngtop"></div>'


SEARCH = ('<div class="srch" id="srch" hidden><label class="toc-head" for="srch-q">Search</label><input type="search" '
          'id="srch-q" class="srch-q" placeholder="A word, an option…  ( / )" autocomplete="off" spellcheck="false">'
          '<div class="srch-info" id="srch-info" aria-live="polite"></div><ol class="srch-res" id="srch-res"></ol></div>')


# ── Assembly ─────────────────────────────────────────────────────────────
def load_chapters(kind):
    chapters = []
    sys.path.insert(0, str(HERE))
    for f in sorted((HERE / MANUALS[kind]["dir"]).glob("ch[0-9][0-9]_*.py")):
        spec = importlib.util.spec_from_file_location(f"{kind}_{f.stem}", f)
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        chapters.append(mod.CHAPTER)
    return chapters


def number(doc, sezioni):
    out, counters, cur = [], {}, 0
    for part in re.split(r'(<section class="chapter" id="ch\d+">)', doc):
        m = re.match(r'<section class="chapter" id="ch(\d+)">', part)
        if m:
            cur = int(m.group(1))
            counters = {"FIG": 0, "TAB": 0}
            out.append(part)
            continue

        def num(mm):
            k = mm.group(1)
            counters[k] += 1
            return f'{"Figure" if k == "FIG" else "Table"} {cur}.{counters[k]}'
        out.append(re.sub(r"«(FIG|TAB)»", num, part) if counters else part)

    def collega(mm):
        titolo = mm.group(1)
        if titolo not in sezioni:
            raise SystemExit(f"rif(«{titolo}»): no section has this title")
        sid, numero = sezioni[titolo]
        return f'<a href="#{sid}">{numero} “{esc(titolo)}”</a>'
    return re.sub(r"«RIF:([^»]+)»", collega, "".join(out))


def build(kind, outdir=ROOT / "docs"):
    meta = MANUALS[kind]
    css = (HERE / "style.css").read_text() + EXTRA_CSS
    js = (HERE / "manual.js").read_text()
    toc, body, sezioni = [], [], {}
    for ci, (ctitle, sections) in enumerate(load_chapters(kind), 1):
        toc.append(f'<li class="toc-ch"><a href="#ch{ci}"><span class="toc-n">{ci}</span><span class="toc-t">{esc(ctitle)}</span></a></li>')
        body.append(f'<section class="chapter" id="ch{ci}"><div class="ch-head"><span class="ch-kick">Chapter {ci}</span>'
                    f'<h2 class="h-ch">{esc(ctitle)}</h2></div>')
        sezioni[ctitle] = (f"ch{ci}", f"ch. {ci}")
        for si, (stitle, shtml) in enumerate(sections, 1):
            sid = f"ch{ci}s{si}"
            if stitle in sezioni:
                raise SystemExit(f"two sections are called «{stitle}»: rif() would not know which one to pick")
            sezioni[stitle] = (sid, f"{ci}.{si}")
            toc.append(f'<li class="toc-se"><a href="#{sid}"><span class="toc-n">{ci}.{si}</span><span class="toc-t">{esc(stitle)}</span></a></li>')
            body.append(f'<section class="sec" id="{sid}"><h3 class="h-sec">{ci}.{si} · {esc(stitle)}</h3>{shtml}</section>')
        body.append("</section>")
    doc = number("".join(body), sezioni)
    page = f"""<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1"><title>{meta["title"]}</title>
<style>{css}</style></head>
<body>
<div class="doc">
  <section class="cover cover-w">
    {logo()}
    <h1 class="cover-title">{meta["h1"]}</h1>
    <div class="cover-meta">
      <div><span>Version</span><b>{VERSION}</b></div>
      <div><span>Date</span><b>{DATE}</b></div>
    </div>
  </section>
  <div class="with-side">
    <aside class="side"><div class="side-inner">{SEARCH}<h2 class="toc-head">Contents</h2><ul class="toc-list" id="toc">{"".join(toc)}</ul></div></aside>
    <main class="main">{doc}</main>
  </div>
  <footer class="doc-foot">{NAME} · {meta["h1"]} · Version {VERSION} · {DATE} · © 2026 Nicola Fiorillo · PolyForm Noncommercial 1.0.0</footer>
</div>
<script>window.MANUAL_CODE_TERMS=[];</script>
<script>
{js}</script>
</body></html>
"""
    outfile = pathlib.Path(outdir) / meta["file"]
    outfile.write_text(page, encoding="utf-8")
    return outfile, len(page)


# ── Checks: the manual aligned with the code ────────────────────────────
def repo_files():
    """All the files of the repository that a manual may cite (no builds, no cached screenshots)."""
    out = set(sorgenti())
    for base, _ in (("tests", 0), ("mockup", 0), ("docs", 0), ("site", 0)):
        for f in (ROOT / base).rglob("*"):
            if f.is_file():
                out.add(f.relative_to(ROOT).as_posix())
    out |= {f.name for f in ROOT.iterdir() if f.is_file()}
    return out


def code_text(*cartelle, estensioni=(".rs", ".toml", ".sh", ".py", ".html", ".js", ".css")):
    """The text of the sources under those folders."""
    out = []
    for f in sorted(sorgenti()):
        if any(f == c_ or f.startswith(c_.rstrip("/") + "/") for c_ in cartelle) and pathlib.Path(f).suffix in estensioni:
            out.append((ROOT / f).read_text(errors="replace"))
    return "\n".join(out)


# Words that do not appear in an English text: two different ones in the same sentence
# flag it as Italian. Only code, keys and <pre> blocks are left out (real names
# from the sources). The manuals are entirely in English: no Italian labels in brackets,
# no Italian quotations, no Italian text in the diagrams or in the screenshots.
ITALIAN_WORDS = re.compile(
    r"\b(il|lo|gli|della|delle|degli|dello|nella|nelle|negli|sono|questo|questa|quando|perché|anche|però|oppure|"
    r"finché|ancora|sempre|niente|nessun|nessuna|dopo|ogni|tutti|tutte|viene|serve|deve|può|hanno|col|coi|dal|dai|"
    r"sul|sui|alla|alle|allo|che|non|una|del|con|si|è)\b", re.I)


ITALIAN_CHART_WORDS = re.compile(
    r"\b(il|gli|della|delle|dello|nella|sono|questo|quando|perché|anche|ogni|che|una|è|"
    r"panoramica|processi|rete|dischi|servizi|unità|attivo|attivi|inattivo|falliti|cerca|filtro|aiuto|esci|"
    r"sezioni|sezione|ordina|segnale|albero|memoria|disponibile|libera|scheda|tasti|errore|stato|salva|"
    r"avvia|ferma|riavvia|abilita|registro|utente|sistema|invia|chiudi|apri)\b", re.I)


def italian_sentences(pagina):
    """The sentences of a manual's running text, and the texts of its drawings and screenshots, that still look Italian."""
    t = re.sub(r'<style.*?</style>|<script.*?</script>|<pre.*?</pre>|<code>.*?</code>|<kbd>.*?</kbd>', " ",
               pagina, flags=re.S)
    frasi = [f.strip() for f in re.split(r"(?<=[.;:!?])\s+|\n", html.unescape(re.sub(r"<[^>]+>", "\n", t)))
             if len({w.lower() for w in ITALIAN_WORDS.findall(f)}) >= 2]
    # Diagram labels are short: one typically Italian word is enough.
    for svg in re.findall(r"<svg.*?</svg>", t, flags=re.S):
        for x in re.findall(r"<text[^>]*>(.*?)</text>", svg, flags=re.S):
            x = html.unescape(x)
            # Options, paths and code names (--delay, tests/ui.rs, MemTotal) are real names, not Italian.
            parole = re.sub(r"(?<!\w)(--?[\w-]+|[\w./-]*[_/.][\w./-]*)", " ", x)
            if ITALIAN_CHART_WORDS.search(parole):
                frasi.append("diagram: " + x)
    return frasi



# ── Texts of the program quoted in the manuals ───────────────────────────
# Texts that are quoted on purpose although they are not literals of the program (what a user
# types, examples, text of other programs): exact strings, after unescaping.
QUOTE_EXCEPTIONS = {
    "Tab bar", "Key bar",          # names of parts of the screen, not texts of the program
    "umount -l",                   # a command of the system
    "up 3d 06:09",                 # the uptime is formatted by a function (format "up {}")
    "gpu0",                        # built as format!("gpu{idx}")
}
_COMMANDS = {"cargo", "ngtop", "git", "sudo", "ssh", "python3", "ldd", "rustup", "ls", "cat", "chmod", "tar",
             "curl", "make", "systemctl", "journalctl", "kill", "nice", "renice", "top", "htop", "btop", "nvtop",
             "podman", "gh", "cp", "mv", "rm", "mkdir", "echo", "export", "cd", "which", "uname", "strace", "su"}
_WORD = re.compile(r"[A-Za-z][A-Za-z0-9_-]{2,}")


def _norm(t):
    return re.sub(r"\s+", " ", re.sub(r"\d+", "0", t)).strip()


def program_texts():
    """(normalised literals of src/ and of --doc-json, set of their words)."""
    lits = []
    for f in sorted((ROOT / "src").rglob("*.rs")):
        src = re.sub(r"'\\?\"'|^\s*//.*$", "", f.read_text(errors="replace"), flags=re.M)   # char literals, comment lines
        for m in re.finditer(r'"((?:[^"\\]|\\.)*)"', src):
            raw = m.group(1).replace("\\n", " ").replace("\\t", " ").replace('\\"', '"').replace("\\u{2014}", "—")
            lits += [x for x in re.split(r"\{[^}]*\}", raw) if x.strip()]
    d = doc_data() or {}

    def walk(x):
        if isinstance(x, str):
            lits.append(x)
        elif isinstance(x, list):
            for y in x:
                walk(y)
        elif isinstance(x, dict):
            for y in x.values():
                walk(y)
    walk(d)
    norm = [_norm(x) for x in lits]
    words = {w.lower() for x in norm for w in _WORD.findall(x)}
    return norm, words


def stale_quotes(page, code_too=True):
    """Texts in ui() labels and in <code> that imitate messages of the program but are not in the program:
    the whole text is a substring of a literal, or (variable parts) every word of three letters or more is."""
    lits, words = program_texts()
    blob = "\n".join(lits)
    cands = [html.unescape(re.sub(r"<[^>]+>", "", x)) for x in re.findall(r'<span class="ui"[^>]*>(.*?)</span>', page, re.S)]
    for x in (re.findall(r"<code>(.*?)</code>", page, re.S) if code_too else []):
        t = html.unescape(html.unescape(re.sub(r"<[^>]+>", "", x))).strip()
        if re.search(r"[×+²(){}|<>\[\]&;]|…", t) or (" " not in t and (re.search(r"[a-z][A-Z]|_|^[A-Z0-9]+$", t) or t[0].isupper() and re.search(r"[A-Z]", t[1:]))):
            continue
        if len(t) < 4 or t.startswith(("-", "/", "~", "$", "#", ".", "<", "[")) or "/" in t or "--" in t or "=" in t \
                or t.endswith(")") or "::" in t or re.search(r"\.\w{1,4}$", t) or t.split()[0] in _COMMANDS \
                or not (" " in t or t[0].isupper()):
            continue
        cands.append(t)
    if not code_too:      # technical manual: thread names (ngtop-xxx) and the like must exist in the program
        for x in re.findall(r"<code>(ngtop-[a-z0-9-]+)</code>", page):
            if x not in blob:
                cands.append(x)
    bad = []
    for t in dict.fromkeys(html.unescape(c_).strip() for c_ in cands):
        if t in QUOTE_EXCEPTIONS or not t:
            continue
        n = _norm(t)
        if n in blob:
            continue
        miss = [w for w in _WORD.findall(n) if w.lower() not in words]
        total = len(_WORD.findall(n))
        if total and len(miss) * 2 >= total or not total:
            bad.append(f"{t}" + (f"   [not in the program: {', '.join(miss)}]" if miss else ""))
    return bad


# Files of the machine ngtop runs on, not of the repository: they may be cited.
SYSTEM_FILES = {"config.toml", "passwd", "meminfo", "mountinfo", "diskstats", "cpuinfo", "loadavg", "uptime",
                "resolv.conf", "pci.ids", "fdinfo", "hostname", "localtime"}

# Spellings with brackets that are not functions.
NON_FUNCTION_NAMES = set()


def controlla(kinds=tuple(MANUALS)):
    errori, pagine = [], []
    with tempfile.TemporaryDirectory(dir=SNAP_DIR if SNAP_DIR.is_dir() else None) as tmp:
        for kind in kinds:
            meta = MANUALS[kind]
            GENERATED.clear()
            STALE.clear()
            fresco, _ = build(kind, tmp)
            pagina = fresco.read_text()
            if "<style>" + (HERE / "style.css").read_text() not in pagina:
                errori.append(f"{meta['file']}: the style does not start with the common canon (style.css)")
            if (HERE / "manual.js").read_text() not in pagina:
                errori.append(f"{meta['file']}: the common script is missing (manual.js)")
            pagine.append(re.sub(r"<script.*?</script>", "", pagina, flags=re.S))
            pubblicato = ROOT / "docs" / meta["file"]
            if not pubblicato.exists() or pubblicato.read_text() != pagina:
                errori.append(f"«docs/{meta['file']}» does not match the sources: python3 docs/sources/build.py")
            if VERSION not in pagina:
                errori.append(f"{meta['file']}: version {VERSION} does not appear")
            if '<html lang="en">' not in pagina:
                errori.append(f"{meta['file']}: the page does not declare lang=\"en\"")
            for frase in italian_sentences(pagina):
                errori.append(f"{meta['file']}: text still in Italian: {frase[:120]}")
            for what in dict.fromkeys(GENERATED):
                errori.append(f"{meta['file']}: generated content missing: {what}")
            for q in stale_quotes(pagina, code_too=(kind == "user")):
                errori.append(f"{meta['file']}: text not found in the program: {q}")
            for s in dict.fromkeys(STALE):
                errori.append(f"{meta['file']}: screenshot {s} is older than the binary or the fixture (build dist/ngtop and regenerate)")
            ids = set(re.findall(r'\bid="([^"]+)"', pagina))
            for a in sorted(set(re.findall(r'href="#([^"]+)"', pagina)) - ids):
                errori.append(f"{meta['file']}: internal link #{a} without a target")
    pagina = "\n".join(pagine)
    codici = [html.unescape(re.sub(r"<[^>]+>", "", x)) for x in re.findall(r"<code>(.*?)</code>", pagina, re.S)]

    if DOC_STALE:
        errori.append("doc.json does not match the binary (`ngtop --doc-json`): python3 docs/sources/build.py --refresh-doc")

    # Functions cited as «name()»: they must exist in the sources.
    words = set(re.findall(r"\b\w+\b", code_text("src", "tests", "scripts", "site", "docs/sources")))
    for x in codici:
        for nome in re.findall(r"^([A-Za-z_][\w:.]*)\(\)$", x.strip()):
            ultimo = re.split(r"::|\.", nome)[-1]
            if ultimo not in words and nome not in NON_FUNCTION_NAMES:
                errori.append(f"cited function that does not exist in the sources: {nome}()")

    # Cited files: they must exist in the repository (the name is compared, and the path if written in full).
    tenuti = repo_files()
    nomi = {f.rsplit("/", 1)[-1] for f in tenuti}
    for x in codici:
        for f in re.findall(r"(?<![\w/.-])((?:[\w.]+[\w.-]*/)*[\w]+[\w-]*\.(?:rs|toml|sh|py|html|js|css|md|json|lock))\b", x):
            if f in SYSTEM_FILES:
                continue
            if "/" in f and not f.startswith(("/", "~")):
                if f not in tenuti and not any(t.endswith("/" + f) for t in tenuti):
                    errori.append(f"cited file that does not exist: {f}")
            elif f not in nomi and f not in SYSTEM_FILES:
                errori.append(f"cited file that does not exist: {f}")
    return list(dict.fromkeys(errori))


def main():
    args = [ALIASES.get(a, a) for a in sys.argv[1:]]
    scelti = [a for a in args if a in MANUALS] or list(MANUALS)
    if "--refresh-doc" in args:
        refresh_doc()
    elif "--check" in args:
        global CHECKING
        CHECKING = True
        errori = controlla(scelti)
        for e in errori:
            print("manual:", e)
        if errori:
            sys.exit(1)
        print("manual aligned with the code")
    else:
        for kind in scelti:
            GENERATED.clear()
            f, n = build(kind)
            print("written", f.relative_to(ROOT), n, "bytes")
            for what in dict.fromkeys(GENERATED):
                print("  placeholder GENERATED:", what)


if __name__ == "__main__":
    # The chapters do `from build import ...`: they must share the state of THIS module
    # (placeholders found, stale screenshots), not of a second copy.
    sys.path.insert(0, str(HERE))
    import build as _b
    _b.main()
