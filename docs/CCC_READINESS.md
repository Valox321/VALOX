# Valox CCC preparation

Status last reviewed: 2026-11-19

This file tracks technical preparation. It does not claim an invitation, an official rating, or CCC approval.

The measurement-led engine-strength plan and current audit priorities are in
[`STRENGTH_ROADMAP.md`](../STRENGTH_ROADMAP.md).

## CCC entry path

Chess.com's CCC overview says the championship runs ongoing engine events, includes qualifying events for lesser-known engines, and plays both standard chess and Chess960: <https://www.chess.com/terms/computer-chess-championship>. The latest published event plan found, CCC25 (Nov. 2025), used an Entry League followed by qualifiers: <https://www.chess.com/blog/AndrewGrantEthereal/ccc25-blitz>.

The latest direct public contact guidance found is on the CCC24 Blitz page, updated Jan. 28, 2025. It says selection is subjective and at the tournament director's discretion, and invites engine authors to contact the director through Chess.com or the OpenBench Discord: <https://www.chess.com/blog/AndrewGrantEthereal/ccc24-blitz>. The CCC25 event plan does not provide a current application form. This is not a promise of consideration. Do not send a submission until Valox's model rights and release are cleared.

## Readiness checklist

- [x] Native Windows release build completes.
- [x] UCI handshake and `isready` work in a local protocol smoke check.
- [x] UCI options include `Hash`, `Threads`, `SyzygyPath`, `EvalFile`, `Clear Hash`, and `UCI_Chess960`.
- [x] Shredder-FEN castling rights and Chess960 rook-square move notation are implemented.
- [x] Standard perft checks pass through depth 4 from start position and Kiwipete, and depth 3 from four special-rule positions.
- [x] Five published Chess960 perft positions pass through depth 3; the first also passes depth 4.
- [x] Differential perft agrees with python-chess on 100 random standard and 40 random Chess960 positions at depth 3, plus 20 standard and 10 Chess960 positions at depth 4.
- [x] Chess960 castling tests cover a stationary king, stationary rook, rook on the king destination, and make/unmake restoration.
- [x] Configure Windows/Linux native and WebAssembly CI workflows.
- [x] Add native UCI smoke/stress steps to the Windows and Linux CI jobs.
- [x] Run the GitHub Actions workflow on Windows, Linux, and WebAssembly from the public source repository.
- [x] Run a 50-search no-crash UCI stress test with clock-managed searches, repeated interrupts, eight threads, and repeated `position`/`ucinewgame`/`isready`/quit cycles.
- [x] Extend the UCI lifecycle stress run to 100 searches on the pinned release executable; all depth, clock-managed, and interrupted searches completed, with repeated resets and clean exit.
- [x] Benchmark 1, 2, 4, and 8 threads and test a 5+0 match on this Windows host.
- [x] Produce a 100-game color-paired estimate against a known engine with a pinned binary, opening set, hardware, time control, and two simultaneous games.
- [x] Record compiler/toolchain, build command, NNUE identity, executable/model SHA-256, and a filtered source snapshot for the local candidate.
- [x] Inventory Rust dependency licenses and include their license texts in the source snapshot.
- [x] Prepare an unsent inquiry asking the CCC director about current intake and runtime requirements.
- [x] Add `VALOX_NNUE_PATH` and `VALOX_NO_NNUE` build controls so release builds can inject weights from outside the repository or produce a weight-free binary.
- [x] Add `tools/make-synthetic-net.py`, a deterministic same-layout network for exercising the NNUE code path without third-party weights.
- [x] Cross-build a static Linux x86_64 musl binary and verify the ELF type, machine, and absence of a dynamic loader.
- [x] Add a `linux-nnue` CI job that runs the 38-test NNUE suite on Ubuntu with the synthetic network, builds the static musl binary, and runs a UCI smoke test and benchmark.
- [x] Add `tools/build-release.py` with SHA-256 manifests and a hard gate: a binary containing the Reckless v60 network cannot be packaged without `--allow-network-redistribution`.
- [x] Produce a legally distributable weight-free Windows package that contains no `.nnue` file and can load an external model through `EvalFile`.
- [x] Add runtime NNUE loading through the `EvalFile` UCI option, `--evalfile`, and `VALOX_EVALFILE`.
- [x] Verify runtime-loaded v60 produces the same nodes and best moves as the embedded build; an eight-round ABBA `--bench 12` comparison was performance-neutral (median -0.18%, 5/8 pairs faster).
- [x] Extend the `linux-nnue` CI job to compare a weight-free musl build using `--evalfile` against the embedded synthetic-network build.
- [x] Confirm the new `linux-nnue` CI job passes in GitHub Actions.
- [x] Run the real-network Linux binary in a Linux kernel under QEMU TCG. `--bench 10` reports 389,894 nodes and the same best moves as Windows; `id name Valox 0.5.0 NNUE` confirms the embedded weights are active.
- [ ] Run the real-network binary on native Linux hardware at the tournament host. The static musl binary and the QEMU run cover the runtime path; this is a final deployment check.
- [x] Decide Valox's own source license. Valox source is dual-licensed under
  MIT OR Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`), and `Cargo.toml`
  declares `license = "MIT OR Apache-2.0"`. `tools/package-source.py` excludes
  `.nnue` files; a fresh extraction of the source archive builds and passes 31
  tests with the classical evaluator.
- [ ] Resolve permission/license terms for `networks/v60-7f587dfb.nnue` before distributing a binary that embeds it. A weight-free release can already ship because the model is supplied at runtime by the user. The Reckless engine is AGPL-3.0, but the separate RecklessNetworks repository contains model files without a model-specific license: <https://github.com/codedeliveryservice/RecklessNetworks>. A ready-to-send permission request is in [`NETWORK_PERMISSION_DRAFT.md`](NETWORK_PERMISSION_DRAFT.md).
- [ ] After rights and release artifacts are ready, ask the CCC tournament director whether Valox can enter a qualifying event.

## Linux and release preparation (2026-11-19)

The Windows host can now cross-build a static Linux binary. `cargo build
--release --target x86_64-unknown-linux-musl` with `rust-lld` produces a
64-bit x86_64 ELF with no dynamic loader. The local artifact is
`build/valox-linux-x86_64-musl-20261119` (64,310,424 bytes with the Reckless v60
network embedded), SHA-256
`FA8FB875F99196DBA04534BF950055328877333186F16D71998847405C72F167`. It is a
local candidate only and must not be distributed.

`tools/build-release.py` builds Windows and Linux binaries, injects a network
only through `VALOX_NNUE_PATH`, and packages binaries with license texts,
`BUILD-INFO.txt`, `SHA256SUMS`, and a ZIP checksum. Without a network it sets
`VALOX_NO_NNUE=1`, so a release cannot accidentally embed a local copy and the
resulting binary starts with the classical evaluator. Packaging a network build
requires `--allow-network-redistribution`; the script refuses otherwise. The
first legally distributable artifact is the weight-free Windows package
`build/release/valox-0.5.0-windows-x86_64.zip`, which contains no `.nnue` file
and supports runtime loading through `EvalFile`.

The synthetic network (`tools/make-synthetic-net.py`) has exactly the v60 byte
layout (63,266,880 bytes) but small deterministic values. It makes the 38 tests
run through the NNUE path on Linux, including incremental-versus-full-refresh
comparisons, without using or distributing the third-party weights. The
`linux-nnue` CI job runs those tests natively on Ubuntu, builds a static musl
binary with the synthetic network, then builds a second weight-free binary and
checks that `--evalfile synthetic.nnue` produces the same nodes and best moves
as the embedded build. A native Linux run with the real weights remains a
deployment check.

## Runtime EvalFile loading (2026-11-19)

The engine can now load a same-layout model at runtime through the UCI
`EvalFile` option, the `--evalfile` command-line argument, or the
`VALOX_EVALFILE` environment variable. The distributable weight-free Windows
package is `build/release/valox-0.5.0-windows-x86_64.zip` (218,735 bytes),
SHA-256 `74767B75D09B22256B479A9E80C51CCB2F19C0D606076970D8B27DD8FC2F3907`. Its
`valox.exe` has SHA-256
`B66CDA444AA21721D4A3E8F9DA3582B6D4CD9ABB9827DBD9368F0CCBACC80DD5`, contains
no weights, and starts with the classical evaluator. It was built from commit
`abf772c`.

Loading `networks/v60-7f587dfb.nnue` at runtime produces exactly the embedded
build's results:

```text
start: bestmove g1f3 nodes 129168
open: bestmove c1g5 nodes 193465
tactics: bestmove e3c5 nodes 48412
endgame: bestmove c4c5 nodes 18849
bench depth 10 nodes 389894
```

An eight-round ABBA comparison of the embedded build against the weight-free
build plus `--evalfile networks/v60-7f587dfb.nnue` at `--bench 12` kept
1,216,373 nodes and all best moves identical. The runtime build was 0.18% faster
at the median (mean -0.27%, 5/8 pairs faster), which is noise: the change is
treated as performance-neutral, not as a gain. The engine validates the file
size (63,266,880 bytes) and reports `info string NNUE loaded ...`; a failed load
leaves the previous model active. The upstream file and SHA-256 are documented
in `THIRD_PARTY_NOTICES.md`; it is supplied by the user and is not part of any
release archive.

## Real-network Linux runtime check (2026-11-19)

The local static musl artifact with the real Reckless v60 weights was booted in
an Alpine Linux 6.18.60 kernel under QEMU TCG (no Windows reboot, no weight
distribution). The initramfs contained only BusyBox, the Valox binary, and a
short init script; the kernel command line was
`console=ttyS0 rdinit=/init panic=-1`.

Observed output:

```text
start: bestmove g1f3 nodes 129168
open: bestmove c1g5 nodes 193465
tactics: bestmove e3c5 nodes 48412
endgame: bestmove c4c5 nodes 18849
bench depth 10 nodes 389894 time_ms 31990 nps 12187
VALOX_BENCH_EXIT=0
id name Valox 0.5.0 NNUE
bestmove b1c3
VALOX_UCI_EXIT=0
```

The node counts and all four best moves are identical to the Windows build, and
the UCI identification confirms the NNUE is active. The slower time is QEMU
instruction emulation, not a regression. This is emulated hardware, not native
Linux; the binary and code path are the same, so it closes the Linux runtime
risk apart from the final tournament-host deployment check. The check is
reproducible with `tools/linux-qemu-smoke.py` given a kernel, BusyBox, and
QEMU.

## Current evidence

- Current source (2026-11-19): `cargo test --release` passes 38 tests
  (including the full perft suite, Chess960 tests, and an ordered
  capture-generator parity test); Clippy with warnings denied, the wasm check,
  and Linux-target `cargo check`/`cargo clippy` all pass. Bench depths 8-13
  produce identical nodes and best moves; `--bench 10` is 389,894 nodes with
  `g1f3`, `c1g5`, `e3c5`, `c4c5`.
- Throughput campaign 2026-11-18/19: accepted changes 16-23 cut `--bench 12`
  wall time from about 4.4-4.9 s to about 3.0-3.1 s at identical node counts.
  The final ABBA comparison against the i16-accumulator build measured a median
  **-28.76% time** (about **1.40x NPS**). Release builds now use
  `codegen-units = 1`. Change 24 adds capture-only quiescence generation at
  identical nodes and moves; three ABBA `--bench 12` series gave medians of
  -1.56%, -1.69% and -0.72%, and a bench-13 series gave -1.97% (15 of 21 rounds
  faster). Change 25 computes both search hashes from one pass over the piece
  masks at identical values; six ABBA `--bench 12` rounds gave -1.45% (6/6
  faster), five bench-13 rounds -0.83%. Change 26 (caching the incremental
  position hash) was rejected at +6% to +9% time. Change 27 rebuilds only the
  actually affected NNUE threat edges; accumulators are bit-identical, and eight
  ABBA `--bench 12` rounds measured -7.58% (8/8 faster) with six bench-13
  rounds at -7.24% (6/6 faster). The current source artifact is
  `build/valox-threat-ray-20261119.exe`, SHA-256
  `D1DEBC3333B42E481E94B0A4A2169CFF632F9078FA6596DB8CDAB09A38D2E958`; the
  preceding hashpair artifact was
  `build/valox-hashpair-20261119.exe`, SHA-256
  `362AF47FFD692F142A2E723D9B81C25149CB2B68DA611876A5F74A7CD1D0C782`, the
  preceding capture-generator artifact was
  `build/valox-capture-gen-20261118.exe`, SHA-256
  `21053119F9FCA0309262DB6F5B7C0AA6CB656F65C4AF4C6E354CAA022045557E`, and the
  original `codegen-units = 1` measurement baseline was
  `build/valox-cgu1-20261118.exe`, SHA-256
  `C64429012896A34D4A6A729F5044FDC2214DBAE325262BC0712094612D45D03E`. These are
  throughput results, not Elo claims. Details:
  `docs/STRENGTH-CAMPAIGN-2026-11.md`.
- Earlier source (2026-11-17): `cargo test` passed 24 tests locally; formatting and Clippy with warnings denied passed, and the release build completed.
- Strength campaign 2026-11-17: AVX2 NNUE feature updates measure +17.5% NPS with identical nodes and best moves; the rewritten root-split SMP search reaches depth 13 instead of 10 at 3 s/move with four threads while searching 3.7x fewer nodes at fixed depth; stop checks run every 512 nodes; Chess960 `info` and Syzygy castling/en-passant guards are fixed. Details: `docs/STRENGTH-CAMPAIGN-2026-11.md`.
- GitHub Actions run [#13](https://github.com/Valox321/VALOX/actions/runs/44606745624) passed on commit `6dacd86`: Windows native, Ubuntu native, WebAssembly, and `linux-nnue` all green, including the runtime `EvalFile` equivalence check. This is the code state whose source archive fingerprint is recorded below; later commits only touch documentation and CI text.
- GitHub Actions run [#11](https://github.com/Valox321/VALOX/actions/runs/44606262028) passed on commit `261a196`: Windows native, Ubuntu native, WebAssembly, and `linux-nnue`. The Linux job generated the synthetic network, passed 38 native tests through the NNUE path, built the static musl binary, and verified that a weight-free binary loaded with `--evalfile synthetic.nnue` produces identical best moves and node counts (304,504 at depth 10) to the embedded build. The UCI `EvalFile` smoke test also passed. Run #10 had failed only in the workflow's text comparison because `grep` included `time_ms`/`nps`; that check now extracts the node prefix, with no engine change.
- GitHub Actions run [#8](https://github.com/Valox321/VALOX/actions/runs/44598540713) passed on commit `5ae51b4`: Windows native, Ubuntu native, WebAssembly, and the new `linux-nnue` job. The Linux job generated the synthetic same-layout network, passed all 38 tests through the NNUE path natively on Ubuntu, built the static x86_64 musl binary, verified static linkage, and passed a UCI smoke test and `--bench 10`. This validates Linux execution and the NNUE code path without the third-party weights.
- GitHub Actions run [#1](https://github.com/Valox321/VALOX/actions/runs/43240093039) passed on commit `e540d9b`: Windows native build, UCI handshake/stress checks, Linux native build/UCI stress, and WebAssembly compile all succeeded. The public source build uses the documented classical fallback because the unlicensed model weights are excluded.
- `cargo fmt --all -- --check`, Windows `cargo clippy --all-targets -- -D warnings`, Linux-target `cargo check --all-targets` and `cargo clippy --all-targets -- -D warnings`, native release build, and `cargo check --lib --target wasm32-unknown-unknown` pass locally.
- Fresh extraction of `build\valox-source-20261112.zip`: all 17 tests, Clippy with warnings denied, and offline release build pass. The archive now includes the root build script and strength roadmap.
- `cargo build --release`: passes (2026-11-12).
- Current source after the bitboard-hash optimization: `cargo test` passes 17
  tests; Clippy with warnings denied passes. UCI verification, tactical suite
  (6/6), and 100 standard plus 40 Chess960 differential perft positions at
  depth 3 pass. The fixed four-position depth-10 bench measured a +5.1% median
  NPS change over three runs. A 20-game paired A/B match against the previous
  Valox build scored 9.5/20; its +/-105 Elo interval is inconclusive. Full
  details and artifacts are in `docs/SEARCH-AND-NNUE-EXPERIMENTS-2026-11.md`.
- Current release candidate adds three NNUE hot-path changes. It passes 18
  tests, Clippy, standard/Chess960/stop UCI checks, and 100 standard + 40
  Chess960 differential-perft positions. Four interleaved fixed-depth bench
  runs measure +74.3% throughput over the pre-NNUE-optimization local binary
  at identical node counts and best moves. Two separate 20-game self-play A/B
  runs favored the faster candidates, but their Elo confidence intervals
  cross zero; neither establishes a strength gain. Full profiles, PGNs, logs,
  and raw benchmark data are in `tools/engine-match/results/` and the NNUE
  experiment report.
- Full-strength Stockfish browser match, 2026-11-12: Valox lost two games by
  mate (0-2) with a five-second per-move cap and no match clock. This is a
  diagnostic two-game sample, not an Elo estimate. Legal PGN:
  `tools/engine-match/results/valox-stockfish-fullstrength-browser-20261112.pgn`.
- `tools\engine-match\verify-uci.ps1`: passed with eight threads; checks UCI identification/options, readiness, standard search, Chess960 Shredder-FEN and rook-square castling input, `stop`, and `quit`.
- `python tools\engine-match\stress-uci.py build\valox.exe --games 100`: passed 100 searches with eight threads, alternating start position/opening moves and mixing depth-limited, clock-managed, and explicitly interrupted searches; repeated `ucinewgame`, `position`, `isready`, and clean exit all passed.
- `python tools\engine-match\differential-perft.py build\valox.exe --positions 100 --chess960 40 --depth 3 --seed 20261112` and a second run of 20 standard plus 10 Chess960 positions at depth 4: every leaf count matched python-chess.
- `python tools\tactical-suite.py --depth 12`: Valox 5/5; Stockfish reference 5/5.
- `build\valox.exe --bench 10`: 352,653 nodes in 9,237 ms (38,178 nodes/s) on this Windows host; this is a local performance baseline, not a strength estimate.
- `python tools\engine-match\bench-threads.py build\valox.exe --depth 10`: 1/2/4/8 threads measured 49,022/86,961/162,047/225,781 nps on the fixed start-position search. This is one noisy host sample at one depth; it records throughput, not playing strength.
- Latest 100-game paired match (2026-11-12) against Stockfish 19 with `UCI_LimitStrength=true`, `UCI_Elo=2000`, `Hash=256`, Syzygy WDL, `5+0`, one thread per engine, two concurrent games, and the bundled sequential opening set: Valox 68 wins, 28 draws, 4 losses (82/100 points, 82%). Cute Chess reports `+263.4 ±65.9` relative Elo and LOS 100%. Valox scored 43/50 as White and 39/50 as Black. Treat this as a local result for this configuration, not an official or opponent-independent Valox rating. The pinned Valox executable SHA-256 was `87EF90A88B2A629A357DE7ABB460216E5733B7130E7AC7BCE96F1C94E03951BF`.
- `python tools\engine-match\audit-pgn.py tools\engine-match\results\valox-stockfish-2000-20261112-145753.pgn --expected-games 100`: parsed all 100 games and found no illegal moves or PGN errors. Match log shows 28 threefold draws and 72 checkmates; no time forfeits.
- `THIRD_PARTY_NOTICES.md` lists the current Rust runtime dependencies and the source snapshot includes the identified MIT/Apache license texts. Valox's own source is licensed under MIT OR Apache-2.0; the external NNUE model's redistribution rights remain unresolved and the model is excluded from source archives.
- `build/valox-source-20261119.zip` (64 files, 191,403 bytes, no `.nnue` files,
  SHA-256 `C9E57D4E004B6A488C06248B87DE8A9700FEB83396C089D14F661A6E87CE5D76`)
  is accompanied by `build/valox-source-20261119.zip.sha256`. A fresh extraction
  passes 31 tests with the classical evaluator and builds offline; with
  `VALOX_NNUE_PATH` pointing at the synthetic network outside the archive, it
  passes all 38 tests.
- Earlier 20-game match (2026-11-12) against Stockfish 19 configured with `UCI_Elo=2000`, `Hash=256`, at `10+0`: Valox 12 wins, 6 draws, 2 losses (75%). Cute Chess reported a relative difference of `+190.8 ±151.4 Elo`; this is a small sample and not Valox's standalone rating. Valox scored 8/10 as White and 4/10 as Black. PGN contains 20 games and parses with zero errors; all terminations were mate or draw, with no time forfeits or illegal moves in the match log.
- Match record from 2026-11-12: Valox scored 29 wins, 11 draws, 0 losses in 40 games against Stockfish 19 configured with `UCI_LimitStrength=true` and `UCI_Elo=2000`, at `15+0`. This is a short, limited-strength local result, not an official Elo estimate. `15+0` is 15 seconds initial clock per side with no increment for the entire game.
- Current network weights are Reckless v60, not a Valox-trained network. The experimental Bullet checkpoints in `tools/training` did not outperform the current evaluator on their small correlated holdout and are not integrated.
- Current embedded candidate with runtime `EvalFile` support:
  `build\valox-runtime-evalfile-embedded-20261119.exe` (63,804,928 bytes), SHA-256
  `383400F286345509F613438B7DD47BC6BB9D48ED059E60147A577665BD69E774`. The
  previous pinned throughput artifact remains
  `build\valox-threat-ray-20261119.exe`, SHA-256
  `D1DEBC3333B42E481E94B0A4A2169CFF632F9078FA6596DB8CDAB09A38D2E958`. These are
  local candidate artifacts only; no public release has been prepared.
- The distributable weight-free release is
  `build\release\valox-0.5.0-windows-x86_64.zip` (218,735 bytes), SHA-256
  `74767B75D09B22256B479A9E80C51CCB2F19C0D606076970D8B27DD8FC2F3907`; its
  `valox.exe` has SHA-256
  `B66CDA444AA21721D4A3E8F9DA3582B6D4CD9ABB9827DBD9368F0CCBACC80DD5`. With
  `--evalfile networks\v60-7f587dfb.nnue` it reports the same `--bench 10`
  nodes and best moves as the embedded build. Eight ABBA `--bench 12` rounds
  measured a median -0.18% time (5/8 faster), which is noise; no performance
  gain is claimed.
- The exact build and network fingerprints are recorded in [`RELEASE_CANDIDATE.md`](RELEASE_CANDIDATE.md); the paired match files are in `tools\engine-match\results`.

## Reproduction commands

From the repository root in PowerShell:

```powershell
cargo test
cargo build --release
pwsh -File tools\engine-match\run-match.ps1 -Rounds 50 -TimeControl '5+0' -Concurrency 2
```

The UCI executable is `target\release\valox.exe`; local match scripts use `build\valox.exe`.
