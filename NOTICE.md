# Copyright and third-party notices

Required Notice: Copyright (c) 2026 Nicola Fiorillo (https://nicfio.it)

ngtop — a system monitor for the terminal — is released under the **PolyForm
Noncommercial License 1.0.0**. See [LICENSE.md](LICENSE.md). It is free for any
noncommercial use; for commercial use write to **ngtop@nicfio.it**.

## Third-party components

The `ngtop` binary is statically linked and includes open-source Rust libraries
(crates), each under its own licence (MIT, Apache-2.0 and similar). Their names,
versions and licence texts are listed in
[THIRD-PARTY-LICENSES](THIRD-PARTY-LICENSES), generated from the build by
`scripts/third-party.py`.

| Component | Licence |
|---|---|
| Rust crates compiled into `ngtop` | their own licences, see `THIRD-PARTY-LICENSES` |
| musl libc and the Rust standard library, statically linked | MIT; MIT OR Apache-2.0 — see `THIRD-PARTY-LICENSES` |

These components keep their own licences: the terms above apply to ngtop itself,
not to them.
