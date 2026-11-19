# Third-party notices

Cargo's current dependency metadata lists these runtime dependencies:

| Package | Version | Declared license | License text in this folder |
|---|---:|---|---|
| `pyrrhic-rs` | 0.2.0 | MIT | `licenses/pyrrhic-rs-License.txt` |
| `libc` | 0.2.191 | MIT OR Apache-2.0 | `licenses/libc-MIT.txt`, `licenses/libc-Apache-2.0.txt` |
| `memmap2` | 0.9.12 | MIT OR Apache-2.0 | `licenses/memmap2-MIT.txt`, `licenses/memmap2-Apache-2.0.txt` |

These notices cover the current Rust runtime dependency graph, not all files in the workspace.

Valox's own source is licensed under **MIT OR Apache-2.0**; see `LICENSE-MIT` and `LICENSE-APACHE`. Those licenses apply to Valox code only.

The NNUE file `networks/v60-7f587dfb.nnue` is Reckless v60 and is **not** covered by the Valox license. The Reckless engine is published under AGPL-3.0, but the separate RecklessNetworks repository does not identify a model-specific license for the weight files. Obtain written permission or replace the model before distributing a binary or source archive that contains it. Public source archives created by `tools/package-source.py` exclude `.nnue` files. `tools/build-release.py` defaults to a weight-free build and refuses to package an embedded network without `--allow-network-redistribution`.

A weight-free binary can load a compatible model at runtime through the UCI `EvalFile` option, the `--evalfile` command-line argument, or `VALOX_EVALFILE`. The file is then supplied by the user and is not part of the Valox archive. Upstream location for the v60 file:

```text
https://github.com/codedeliveryservice/RecklessNetworks/releases/download/networks/v60-7f587dfb.nnue
SHA-256 7F587DFB1FE5D74D53909328AFA6FD51650C8C7F45907602DB7FBB1E52948C61
```

This notice is an inventory, not a grant of rights.
