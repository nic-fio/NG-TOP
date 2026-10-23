#!/bin/bash
# Copyright (c) 2026 Nicola Fiorillo
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Misure del monitoraggio remoto (byte, CPU server, CPU locale). Uso: scripts/measure-remote.sh [cartella-log]
exec "$(dirname "$0")/test-remote.sh" --measure "$@"
