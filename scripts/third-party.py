#!/usr/bin/env python3
# Copyright (c) 2026 Nicola Fiorillo
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
"""Generates THIRD-PARTY-LICENSES in the project root.

Lists every crate compiled into the static (musl) binary, with name, version,
SPDX licence and the text of the LICENSE*/COPYING*/NOTICE* files shipped in its
source. The crate list comes from `cargo tree -e normal` (no dev-dependencies,
no crates for other platforms); the licence data from `cargo metadata`.

Usage: scripts/third-party.py
"""
import json
import os
import re
import subprocess
import sys

TARGET = "x86_64-unknown-linux-musl"
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
# Licences considered "usual": anything else is reported on stderr.
USUAL = ("MIT", "Apache-2.0", "BSD", "ISC", "Unicode", "Zlib", "MPL")
COPY_RE = re.compile(r"^\s*copyright\s*(\(c\)|©|\d{4})", re.I)
FILE_RE = re.compile(r"^(LICEN[CS]E|COPYING|NOTICE|UNLICEN[CS]E)", re.I)

MIT_BODY = """Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE."""

# Componenti compilati nel binario ma che non sono crate di crates.io: (nome, versione, licenza,
# sorgente, copyright, testi). I testi MIT sono quelli standard (musl: file COPYRIGHT del progetto).
FIXED = [
    ("musl libc", "1.2.x (linked statically by the Rust musl target)", "MIT",
     "https://musl.libc.org", "Copyright (c) 2005-2020 Rich Felker, et al.",
     "Copyright (c) 2005-2020 Rich Felker, et al.\n\n" + MIT_BODY),
    ("Rust standard library", "(from the Rust toolchain)", "MIT OR Apache-2.0",
     "https://github.com/rust-lang/rust", "Copyright (c) The Rust Project Developers",
     "Copyright (c) The Rust Project Developers\n\n" + MIT_BODY),
]


def run(*args):
    env = dict(os.environ, PATH=os.path.expanduser("~/.cargo/bin") + ":" + os.environ["PATH"])
    return subprocess.run(args, cwd=ROOT, env=env, check=True, capture_output=True, text=True).stdout


def main():
    meta = json.loads(run("cargo", "metadata", "--format-version", "1", "--locked"))
    by_key = {(p["name"], p["version"]): p for p in meta["packages"]}
    tree = run("cargo", "tree", "-e", "normal", "--target", TARGET, "--prefix", "none", "--locked")
    used = set()
    for line in tree.splitlines():
        m = re.match(r"^(\S+) v(\S+)", line)
        if m:
            used.add((m.group(1), m.group(2)))
    root = meta["resolve"]["root"]
    root_pkg = next(p for p in meta["packages"] if p["id"] == root)
    used.discard((root_pkg["name"], root_pkg["version"]))

    entries = []
    unusual = []
    texts = {}  # chiave normalizzata -> (id, nome file, testo canonico)
    per_crate = []
    for key in sorted(used):
        p = by_key.get(key)
        if p is None:
            print("warning: %s %s not in cargo metadata" % key, file=sys.stderr)
            continue
        lic = p.get("license") or ("(see %s)" % p["license_file"] if p.get("license_file") else "UNKNOWN")
        entries.append((p, lic))
        if not any(u in lic for u in USUAL):
            unusual.append((p["name"], p["version"], lic))
        d = os.path.dirname(p["manifest_path"])
        files = sorted(f for f in os.listdir(d) if FILE_RE.match(f) and os.path.isfile(os.path.join(d, f)))
        ids, copyrights = [], []
        for f in files:
            with open(os.path.join(d, f), encoding="utf-8", errors="replace") as fh:
                text = fh.read().strip("\n")
            lines = text.split("\n")
            for ln in lines:
                if COPY_RE.match(ln) and ln.strip() not in copyrights:
                    copyrights.append(ln.strip())
            # Il testo canonico non ha le righe di copyright, che restano per crate.
            body = "\n".join("[copyright notice: see the entry of each crate]" if COPY_RE.match(ln) else ln for ln in lines)
            norm = re.sub(r"\s+", " ", body).strip()
            if norm not in texts:
                texts[norm] = ("T%d" % (len(texts) + 1), f, body)
            ids.append(texts[norm][0])
        per_crate.append((p, lic, ids, copyrights))
    for name, ver, lic, src, cr, text in FIXED:
        lines = text.split("\n")
        body = "\n".join("[copyright notice: see the entry of each crate]" if COPY_RE.match(ln) else ln for ln in lines)
        norm = re.sub(r"\s+", " ", body).strip()
        if norm not in texts:
            texts[norm] = ("T%d" % (len(texts) + 1), "built-in MIT text", body)
        fake = {"name": name, "version": ver, "repository": src}
        per_crate.append((fake, lic, [texts[norm][0]], [cr]))

    out = [
        "THIRD-PARTY LICENCES",
        "====================",
        "",
        "ngtop is distributed as a single static binary. It contains the Rust crates",
        "listed below, each under its own licence. Part 1 lists the crates (name,",
        "version, SPDX licence, copyright notices, licence texts that apply). Part 2",
        "gives each distinct licence text once. Generated by scripts/third-party.py from",
        "`cargo tree -e normal --target %s`." % TARGET,
        "",
        "%d components (%d crates plus musl libc and the Rust standard library), %d distinct licence texts." % (len(per_crate), len(per_crate) - len(FIXED), len(texts)),
        "",
        "PART 1 - CRATES",
        "===============",
    ]
    for p, lic, ids, copyrights in per_crate:
        out += ["", "%s %s" % (p["name"], p["version"]), "  License: %s" % lic]
        if p.get("repository"):
            out.append("  Source: %s" % p["repository"])
        for c in copyrights:
            out.append("  %s" % c)
        if ids:
            out.append("  Licence text: %s" % ", ".join(ids))
        else:
            out.append("  (no licence file in the crate; the licence is as stated above)")
    out += ["", "PART 2 - LICENCE TEXTS", "======================"]
    for tid, fname, body in texts.values():
        users = [p["name"] for p, _, ids, _ in per_crate if tid in ids]
        out += ["", "=" * 78, "%s (from %s)" % (tid, fname), "Used by: %s" % ", ".join(users), "", body]
    with open(os.path.join(ROOT, "THIRD-PARTY-LICENSES"), "w", encoding="utf-8") as fh:
        fh.write("\n".join(out) + "\n")
    print("THIRD-PARTY-LICENSES: %d crates" % len(entries))
    for n, v, lic in unusual:
        print("UNUSUAL LICENCE: %s %s - %s" % (n, v, lic), file=sys.stderr)


main()
