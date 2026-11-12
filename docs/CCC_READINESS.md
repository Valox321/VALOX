# Valox CCC preparation

Status last reviewed: 2026-11-12

This file tracks technical preparation. It does not claim an invitation, an official rating, or CCC approval.

## CCC entry path

Chess.com's CCC overview says the championship runs ongoing engine events, includes qualifying events for lesser-known engines, and plays both standard chess and Chess960: <https://www.chess.com/terms/computer-chess-championship>. The latest published event plan found, CCC25 (Nov. 2025), used an Entry League followed by qualifiers: <https://www.chess.com/blog/AndrewGrantEthereal/ccc25-blitz>.

The latest direct public contact guidance found is on the CCC24 Blitz page, updated Jan. 28, 2025. It says selection is subjective and at the tournament director's discretion, and invites engine authors to contact the director through Chess.com or the OpenBench Discord: <https://www.chess.com/blog/AndrewGrantEthereal/ccc24-blitz>. The CCC25 event plan does not provide a current application form. This is not a promise of consideration. Do not send a submission until Valox's model rights and release are cleared.

## Readiness checklist

- [x] Native Windows release build completes.
- [x] UCI handshake and `isready` work in a local protocol smoke check.
- [x] UCI options include `Hash`, `Threads`, `SyzygyPath`, `Clear Hash`, and `UCI_Chess960`.
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
- [ ] Decide Valox's own source license.
- [ ] Resolve permission/license terms for `networks/v60-7f587dfb.nnue` before distributing a binary that embeds it. The RecklessNetworks repository contains model files but currently shows no model-specific license in its README or repository file list: <https://github.com/codedeliveryservice/RecklessNetworks>.
- [ ] After rights and release artifacts are ready, ask the CCC tournament director whether Valox can enter a qualifying event.

## Current evidence

- `cargo test`: 13 NNUE-enabled unit tests pass locally (2026-11-12); the source-only fallback build passes 12 tests. Both configurations also pass formatting, Clippy, and release build.
- GitHub Actions run [#1](https://github.com/Valox321/VALOX/actions/runs/43240093039) passed on commit `e540d9b`: Windows native build, UCI handshake/stress checks, Linux native build/UCI stress, and WebAssembly compile all succeeded. The public source build uses the documented classical fallback because the unlicensed model weights are excluded.
- `cargo fmt --all -- --check`, Windows `cargo clippy --all-targets -- -D warnings`, Linux-target `cargo check --all-targets` and `cargo clippy --all-targets -- -D warnings`, native release build, and `cargo check --lib --target wasm32-unknown-unknown` pass locally.
- Fresh extraction of `build\valox-source-20261112.zip`: formatting, 13 tests, Clippy with warnings denied, release build, UCI standard/Chess960/stop smoke check, and 10 UCI stress searches all passed. Extracted Windows build has the same size as the pinned executable but differs in 24 PE metadata bytes due to a different build path; source build works, but the executable is not bit-for-bit reproducible across paths.
- `cargo build --release`: passes (2026-11-12).
- `tools\engine-match\verify-uci.ps1`: passed with eight threads; checks UCI identification/options, readiness, standard search, Chess960 Shredder-FEN and rook-square castling input, `stop`, and `quit`.
- `python tools\engine-match\stress-uci.py build\valox.exe --games 100`: passed 100 searches with eight threads, alternating start position/opening moves and mixing depth-limited, clock-managed, and explicitly interrupted searches; repeated `ucinewgame`, `position`, `isready`, and clean exit all passed.
- `python tools\engine-match\differential-perft.py build\valox.exe --positions 100 --chess960 40 --depth 3 --seed 20261112` and a second run of 20 standard plus 10 Chess960 positions at depth 4: every leaf count matched python-chess.
- `python tools\tactical-suite.py --depth 12`: Valox 5/5; Stockfish reference 5/5.
- `build\valox.exe --bench 10`: 352,653 nodes in 9,237 ms (38,178 nodes/s) on this Windows host; this is a local performance baseline, not a strength estimate.
- `python tools\engine-match\bench-threads.py build\valox.exe --depth 10`: 1/2/4/8 threads measured 49,022/86,961/162,047/225,781 nps on the fixed start-position search. This is one noisy host sample at one depth; it records throughput, not playing strength.
- Latest 100-game paired match (2026-11-12) against Stockfish 19 with `UCI_LimitStrength=true`, `UCI_Elo=2000`, `Hash=256`, Syzygy WDL, `5+0`, one thread per engine, two concurrent games, and the bundled sequential opening set: Valox 68 wins, 28 draws, 4 losses (82/100 points, 82%). Cute Chess reports `+263.4 ±65.9` relative Elo and LOS 100%. Valox scored 43/50 as White and 39/50 as Black. Treat this as a local result for this configuration, not an official or opponent-independent Valox rating. The pinned Valox executable SHA-256 was `87EF90A88B2A629A357DE7ABB460216E5733B7130E7AC7BCE96F1C94E03951BF`.
- `python tools\engine-match\audit-pgn.py tools\engine-match\results\valox-stockfish-2000-20261112-145753.pgn --expected-games 100`: parsed all 100 games and found no illegal moves or PGN errors. Match log shows 28 threefold draws and 72 checkmates; no time forfeits.
- `THIRD_PARTY_NOTICES.md` lists the current Rust runtime dependencies and the source snapshot includes the identified MIT/Apache license texts. The external NNUE model's redistribution rights and Valox's own source license are still unresolved.
- Earlier 20-game match (2026-11-12) against Stockfish 19 configured with `UCI_Elo=2000`, `Hash=256`, at `10+0`: Valox 12 wins, 6 draws, 2 losses (75%). Cute Chess reported a relative difference of `+190.8 ±151.4 Elo`; this is a small sample and not Valox's standalone rating. Valox scored 8/10 as White and 4/10 as Black. PGN contains 20 games and parses with zero errors; all terminations were mate or draw, with no time forfeits or illegal moves in the match log.
- Match record from 2026-11-12: Valox scored 29 wins, 11 draws, 0 losses in 40 games against Stockfish 19 configured with `UCI_LimitStrength=true` and `UCI_Elo=2000`, at `15+0`. This is a short, limited-strength local result, not an official Elo estimate. `15+0` is 15 seconds initial clock per side with no increment for the entire game.
- Current network weights are Reckless v60, not a Valox-trained network. The experimental Bullet checkpoints in `tools/training` did not outperform the current evaluator on their small correlated holdout and are not integrated.
- Current Windows release executable SHA-256: `87EF90A88B2A629A357DE7ABB460216E5733B7130E7AC7BCE96F1C94E03951BF` (`build\valox.exe`, built 2026-11-12). This is a local candidate artifact only; no public release has been prepared.
- The exact build and network fingerprints are recorded in [`RELEASE_CANDIDATE.md`](RELEASE_CANDIDATE.md); the paired match files are in `tools\engine-match\results`.

## Reproduction commands

From the repository root in PowerShell:

```powershell
cargo test
cargo build --release
pwsh -File tools\engine-match\run-match.ps1 -Rounds 50 -TimeControl '5+0' -Concurrency 2
```

The UCI executable is `target\release\valox.exe`; local match scripts use `build\valox.exe`.
