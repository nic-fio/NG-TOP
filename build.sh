#!/bin/bash
# Copyright (c) 2026 Nicola Fiorillo
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Compila ngtop come eseguibile statico (musl) in dist/ngtop e verifica che non abbia dipendenze dinamiche.
set -euo pipefail
cd "$(dirname "$0")"
export PATH="$HOME/.cargo/bin:$PATH"
TARGET=x86_64-unknown-linux-musl
# niente percorsi della macchina di build nel binario (messaggi di panic delle crate)
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$HOME/.cargo/registry/src=/cargo/registry --remap-path-prefix=$HOME/.cargo/git=/cargo/git --remap-path-prefix=$HOME/.rustup=/rustup --remap-path-prefix=$PWD=/ngtop"
cargo build --release --target "$TARGET"
mkdir -p dist
rm -f dist/ngtop  # evita "Text file busy" se ngtop è in esecuzione
cp "target/$TARGET/release/ngtop" dist/ngtop
if ldd dist/ngtop 2>&1 | grep -qE "not a dynamic executable|statically linked"; then
  echo "OK: dist/ngtop is static ($(du -h dist/ngtop | cut -f1))"
else
  echo "ERROR: dist/ngtop has dynamic dependencies:"; ldd dist/ngtop; exit 1
fi
