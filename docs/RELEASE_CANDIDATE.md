# Valox local release candidate

Built on 2026-11-12 from the workspace state in `C:\Users\Valox\Pictures\Valox Engine`.

| Field | Value |
|---|---|
| Engine | Valox 0.5.0 |
| Platform | Windows x86_64, MSVC |
| Rust compiler | `rustc 1.99.1 (58cf8f9c3 2026-10-13)` |
| Cargo | `cargo 1.99.1 (5f94df478 2026-08-27)` |
| Build command | `CARGO_TARGET_DIR=target\valox-delta-slices cargo build --release` (PowerShell: `$env:CARGO_TARGET_DIR='target\valox-delta-slices'; cargo build --release`) |
| Executable | `build\valox.exe` |
| Executable SHA-256 | `05133DF8DCB12304550F73670E9A33063C19B82FF6A9FFB906174872F3D09C81` |
| NNUE | Reckless v60, embedded at compile time |
| NNUE SHA-256 | `7F587DFB1FE5D74D53909328AFA6FD51650C8C7F45907602DB7FBB1E52948C61` |
| Source snapshot | `build\valox-source-20261112.zip` (53 files: engine source, build script, model, trainer source, scripts, roadmap, documentation, and third-party license texts; generated datasets/checkpoints and match binaries excluded) |
| Source snapshot SHA-256 | See `build\valox-source-20261112.zip.sha256` |

This is a local candidate record, not a public release. The source snapshot is an archive of workspace state based on Git commit `9e16ae0`, including local changes; it is not itself a Git commit. The workspace remote is `https://github.com/Valox321/VALOX.git`.

This candidate includes per-attacker NNUE threat-map reuse, direct attack-map generation, and specialized accumulator delta loops. All 18 Rust tests, Clippy with warnings denied, UCI standard/Chess960/stop checks, and differential perft over 100 standard plus 40 Chess960 positions passed. Against the initial build before these three NNUE optimizations, four interleaved depth-10 benchmark repetitions measured +74.3% median throughput, with identical node counts and best moves. The delta-loop change alone adds +1.9% NPS and scored 13.5/20 in a paired 20-game match (estimated +127 +/-134 Elo, 97.4% LOS); encouraging but not definitive. The direct-ray change alone scored 12.5/20 (+88.7 +/-89.2 Elo, 97.1% LOS) and is likewise inconclusive. Details, raw measurements, profiles, PGNs, and logs are in `tools/engine-match/results/NNUE-THREAT-CACHE-20261112.md`. Linux-target checks and GitHub Actions have not been rerun for this latest source change.

Before distributing or submitting this binary, resolve redistribution rights for the embedded Reckless v60 weights and keep the dependency notices current. Valox's source is licensed under MIT OR Apache-2.0; the weights are not covered by that license. Do not treat this candidate as cleared for redistribution until the network rights are resolved.

## Local build, 2026-11-19

This supersedes the executable fingerprint above for current local testing. It is still not a public release.

| Field | Value |
|---|---|
| Platform | Windows x86_64, MSVC |
| Build command | `cargo build --release` (release profile now sets `codegen-units = 1`) |
| Executable | `build\valox-hashpair-20261119.exe` |
| Executable SHA-256 | `362AF47FFD692F142A2E723D9B81C25149CB2B68DA611876A5F74A7CD1D0C782` |
| NNUE | Reckless v60, embedded at compile time |
| NNUE SHA-256 | `7F587DFB1FE5D74D53909328AFA6FD51650C8C7F45907602DB7FBB1E52948C61` |
| Benchmark reference | `--bench 10`: 389,894 nodes, best moves `g1f3`, `c1g5`, `e3c5`, `c4c5` |
| Source archive | `build\valox-source-20261119.zip` (64 files, no `.nnue` weights); SHA-256 in `build\valox-source-20261119.zip.sha256` |

Validation for this state:

- `cargo test --release`: 38 tests passed, including the full perft suite,
  Chess960 tests, and a test that checks the ordered capture/promotion list
  against the filtered full move list across nine FENs to depth 2.
- `cargo clippy --all-targets -- -D warnings`, the wasm check, and
  Linux-target `cargo check`/`cargo clippy` (x86_64-unknown-linux-gnu) pass.
- Differential perft against python-chess: 100 standard + 40 Chess960
  positions at depth 3, and 40 standard + 20 Chess960 at depth 4.
- NNUE checksums over the 300-position dataset: `45968` (evaluation) and
  `8871920` (update), unchanged throughout the throughput work.
- Bench depths 8-13 produce identical node counts and best moves.

Throughput work from changes 16-23 reduced `--bench 12` wall time from about
4.4-4.9 s to about 3.0-3.1 s at identical nodes. The final interleaved ABBA
comparison against the i16-accumulator build measured a median **-28.76% time**
(about **1.40x NPS**). The `codegen-units = 1` comparison used
`build\valox-cgu1-20261118.exe`. After `cargo fmt` and the license files, the
source built to `build\valox-licensed-20261118.exe`; four ABBA `--bench 12`
rounds measured a median -2.60% versus the `cgu1` artifact at identical nodes.
That difference is code layout, not a separate optimisation, and is not claimed
as a strength gain.

Change 24 adds capture-only generation for quiescence when not in check. Bench
depths 8-13 keep identical nodes and moves, and checksums are unchanged. Three
ABBA `--bench 12` series gave medians of -1.56%, -1.69% and -0.72%; a
bench-13 series gave -1.97% (15 of 21 rounds faster overall). A conservative
reading is about 1% less wall time. All numbers are throughput only; no Elo
claim is made.

Change 25 computes the full and the clock-independent hash from one pass over
the piece masks instead of two. Values are identical by construction; bench
depths 8-13 and checksums are unchanged. Six ABBA `--bench 12` rounds gave a
median -1.45% (6/6 faster, mean -1.92%); five bench-13 rounds gave -0.83%
(mean +0.47%, 3/5 faster). Conservative reading: about 1% less wall time.

## Local build, 2026-11-19 (threat-ray deltas)

This is the current pinned local build. It supersedes the hashpair executable
for testing; it is still not a public release.

| Field | Value |
|---|---|
| Platform | Windows x86_64, MSVC |
| Build command | `cargo build --release` |
| Executable | `build\valox-threat-ray-20261119.exe` |
| Executable SHA-256 | `D1DEBC3333B42E481E94B0A4A2169CFF632F9078FA6596DB8CDAB09A38D2E958` |
| NNUE | Reckless v60, embedded at compile time |
| NNUE SHA-256 | `7F587DFB1FE5D74D53909328AFA6FD51650C8C7F45907602DB7FBB1E52948C61` |
| Benchmark reference | `--bench 10`: 389,894 nodes; `--bench 13`: 1,957,764 nodes; best moves identical to the hashpair build |
| Source archive | `build\valox-source-20261119.zip` (64 files, no `.nnue` weights); SHA-256 in `build\valox-source-20261119.zip.sha256` |

Change 27 tightens the NNUE threat-feature delta. The old update rebuilt all
outgoing edges of every piece that geometrically attacked a changed square. The
new one rebuilds full outgoing edges only on changed attacker squares, emits a
single edge for unchanged leapers, and regenerates only the affected ray for
unchanged sliders. Accumulators are bit-identical: bench depths 8-13 match the
hashpair build, and `nnue-bench` reports the same `45968`/`8871920` checksums.
Eight ABBA `--bench 12` rounds gave a median **-7.58%** time (8/8 faster, mean
-9.38%); six bench-13 rounds gave -7.24% (6/6 faster, mean -8.33%). All 38
tests, fmt, Clippy, wasm and Linux-target checks, differential perft (100
standard + 40 Chess960 at depth 3) and the tactical suite (6/6 at depths 12 and
16) pass. Throughput only; no Elo claim.

Open blockers for any public release remain the redistribution rights for the
embedded Reckless v60 network. The Valox source itself is dual-licensed under
MIT OR Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`). A weight-free build can
already be distributed; see the runtime `EvalFile` section below.

## Runtime EvalFile build, 2026-11-19

The engine can load a compatible model at runtime, so the strong network no
longer has to be embedded in a distributed binary.

| Field | Value |
|---|---|
| Executable | `build\valox-runtime-evalfile-20261119.exe` |
| Size | 537,600 bytes |
| Executable SHA-256 | `09CCA536342F92B5CF1B1CCB088D82871D00B9FCC8114FFBE242244CEEA5843F` |
| Embedded NNUE | none |
| Runtime loading | UCI `EvalFile`, `--evalfile <path>`, `VALOX_EVALFILE` |
| Model validation | exact size 63,266,880 bytes; failed loads keep the previous model |
| `--bench 10` with v60 | 389,894 nodes; `g1f3`, `c1g5`, `e3c5`, `c4c5` |
| ABBA `--bench 12` vs embedded | median -0.18% time, mean -0.27%, 5/8 pairs faster; identical 1,216,373 nodes and moves |
| Source archive | `build\valox-source-20261119.zip` (64 files, 191,403 bytes, no `.nnue` weights) |
| Source archive SHA-256 | `C9E57D4E004B6A488C06248B87DE8A9700FEB83396C089D14F661A6E87CE5D76` |

The rebuilt source archive was extracted into an empty directory and tested
offline: 31 tests pass with the classical evaluator, and 38 tests pass when
`VALOX_NNUE_PATH` points at a synthetic same-layout network outside the
archive. It contains no `.nnue` entry.

The validation is performance-neutral within noise and is not claimed as a
speed gain. The weight-free binary starts with the classical evaluator. The
third-party v60 file is supplied by the user from the upstream RecklessNetworks
release; it is not part of any Valox archive. Its upstream URL and SHA-256 are
in `THIRD_PARTY_NOTICES.md`.

## Release tooling and Linux cross-build, 2026-11-19

`build.rs` now accepts `VALOX_NNUE_PATH` for a network outside the repository
and `VALOX_NO_NNUE=1` to produce a weight-free binary. The default remains
`networks/v60-7f587dfb.nnue`, so existing local builds are unchanged.

| Field | Value |
|---|---|
| Linux target | `x86_64-unknown-linux-musl`, static ELF, no dynamic loader |
| Linux cross-build | `cargo build --release --target x86_64-unknown-linux-musl` with `rust-lld` |
| Local Linux artifact | `build/valox-linux-x86_64-musl-20261119` (64,310,424 bytes, Reckless v60 embedded) |
| Linux artifact SHA-256 | `FA8FB875F99196DBA04534BF950055328877333186F16D71998847405C72F167` |
| Release tool | `tools/build-release.py` |
| Network gate | packaging a network build requires `--allow-network-redistribution` |
| Weight-free Windows package | `build/release/valox-0.5.0-windows-x86_64.zip` (218,735 bytes) |
| Weight-free package SHA-256 | `74767B75D09B22256B479A9E80C51CCB2F19C0D606970D8B27DD8FC2F3907` |
| Weight-free executable SHA-256 | `B66CDA444AA21721D4A3E8F9DA3582B6D4CD9ABB9827DBD9368F0CCBACC80DD5` |

The weight-free package contains no `.nnue` file and is the only artifact that
is currently safe to distribute. It can load the v60 model through `EvalFile`.
The Linux artifact embeds the Reckless network and is local-only until
permission is confirmed. The package was rebuilt from commit `abf772c` and
checked: the ZIP has no `.nnue` entry, and its `valox.exe` with
`--evalfile networks/v60-7f587dfb.nnue --bench 10` reports 389,894 nodes and
the expected best moves `g1f3`, `c1g5`, `e3c5`, `c4c5`.

`tools/make-synthetic-net.py` writes a 63,266,880-byte file with the v60 layout
and small deterministic weights. The `linux-nnue` CI job uses it to run all 38
tests through the NNUE path on Ubuntu, builds the static musl binary, and runs a
UCI smoke test and benchmark. It then builds a weight-free binary and verifies
that `--evalfile synthetic.nnue` reproduces the embedded build's nodes and best
moves, and that the UCI `EvalFile` option loads the file. This validates Linux,
runtime loading, and NNUE inference without distributing the third-party
weights; it does not measure playing strength. GitHub Actions run
[#11](https://github.com/Valox321/VALOX/actions/runs/44606262028) passed all
jobs, including `linux-nnue`, on commit `261a196`. Its runtime check reported
identical nodes (304,504 at depth 10) and best moves for the embedded synthetic
build and the weight-free build loaded with `--evalfile`.

The local real-network musl artifact was also booted in an Alpine Linux 6.18.60
kernel under QEMU TCG. It reported `--bench 10` = 389,894 nodes with best moves
`g1f3`, `c1g5`, `e3c5`, `c4c5`, identical to the Windows build, and
`id name Valox 0.5.0 NNUE` confirmed the embedded weights were active. The
emulated run validates the Linux runtime path without publishing the weights;
native tournament-host execution is the remaining deployment check.
`tools/linux-qemu-smoke.py` reproduces the run from a kernel, BusyBox, and QEMU.
