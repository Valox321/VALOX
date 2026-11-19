# VALOX Strength Roadmap

Last reviewed: 2026-11-19

This is a measurement-led engineering plan. It does not claim a particular
playing strength or promise a fixed percentage gain. Keep a change only when it
preserves correctness and demonstrates a repeatable improvement against the
previous build.

## Current technical baseline

- Rust UCI engine with legal move generation, make/unmake, alpha-beta/PVS,
  iterative deepening, aspiration windows, transposition table, null-move and
  futility pruning, LMR, SEE ordering, history heuristics, Syzygy WDL, and an
  embedded Reckless v60 NNUE when the local weights are available.
- The NNUE is third-party trained data. VALOX has no independent trained NNUE
  yet. VALOX source is dual-licensed under MIT OR Apache-2.0 since 2026-11-18;
  the weights are not covered by that license and their redistribution rights
  remain unresolved (`docs/NETWORK-PERMISSION-DRAFT.md`).
- 2026-11-18/19 throughput: accepted changes 16-23 cut `--bench 12` wall time
  from about 4.4-4.9 s to about 3.0-3.1 s at identical nodes. The final ABBA
  comparison against the i16-accumulator build measured a median **-28.76%**
  time (about **1.40x NPS**). Change 24 adds capture-only quiescence generation
  at identical nodes and moves; its ABBA medians were -1.56%, -1.69%, -0.72%
  (bench 12) and -1.97% (bench 13), 15 of 21 rounds faster. Change 25 computes
  the TT key and the repetition key from one piece-mask pass; six ABBA bench-12
  rounds gave a median -1.45% (6/6 faster) and five bench-13 rounds -0.83%.
  Throughput only; no Elo claim.
- `cargo test`: 38 tests passed on 2026-11-19.
- `cargo clippy --all-targets -- -D warnings`: passed on 2026-11-19.
- 2026-11-17 strength work: AVX2 NNUE feature updates (+17.5% NPS at identical
  nodes), a rewritten root-split SMP search (3.7x fewer nodes at depth 10 with
  four threads; depth 13 instead of 10 at 3 s/move), stop checks every 512
  nodes, a less conservative sudden-death budget, and Chess960/Syzygy
  correctness fixes. A countermove plus "improving" heuristic batch was
  rejected after 30 paired games (baseline 7-2-21, +58.5 Elo, LOS 95.2%).
  Later changes in the same campaign: incremental NNUE feature generation,
  a shared lock-free 16-byte-slot transposition table (9-3-18, +70.4 Elo,
  LOS 95.8%), SEE reuse in quiescence (bit-identical search, -3.8% time), and
  dynamic root scheduling with all requested threads (90 games, 57.2%,
  +50.5 Elo, LOS ~97.6%; 24% faster to depth 13 with four threads), and
  soft/hard time deadlines that finish a running iteration (60 games, 64.2%,
  ~+101 Elo, no time forfeits).
  SEE pruning of losing captures and an improving heuristic were tested and
  rejected (combined 13-16-31 and 8-10-12 respectively); a shared root
  best-score threshold was rejected after losing 5-19-6 (26.7%), while capture
  history and internal iterative reduction were neutral (50.0% each) and
  reverted. History-based LMR was also neutral (50.0%) and larger hard-deadline
  overhead was not significant (51.7%). Singular extensions scored 55.0% over
  60 games on the six-opening set (+34.9 Elo, LOS ~78%), but only 48.75% over
  80 games on the expanded twenty-opening set (baseline +8.7 +/- 55.6), so the
  apparent gain did not generalize and they were rejected. Correction history
  (pawn-structure-keyed static-eval correction) was also rejected: after
  damping it to a 6.6% node increase it scored 43.1% over 80 games on the
  expanded set (baseline +48.1 +/- 52.0, LOS 96.5%). ProbCut with a depth-5
  verification reduced nodes by 5.9% but was neutral over 80 games (+4.3
  +/- 50.6, LOS ~57%). Broad delta pruning in quiescence cut nodes by 44% but
  lost 28.5-51.5 (35.6%, baseline +102.8 +/- 56.5) and was reverted. A
  countermove table was neutral in isolation over 160 expanded-opening games
  (50.6%, +4.3 Elo, LOS 56.3%) and was also reverted. Extending reverse
  futility to depth 6 raised bench nodes and scored 45.6% (-30.5 Elo) over 80
  games, so the deeper pruning was also reverted. A more aggressive
  sudden-death time divisor (30 instead of 40) was exactly neutral over 80
  games (50.0%) with no time forfeits and was reverted too. A repetition-scan
  cleanup keeps the search bit-identical and adds one test.
  Details and measurements: `docs/STRENGTH-CAMPAIGN-2026-11.md`.
- The 2026-11-12 full-strength Stockfish browser experiment ended 0-2, both by
  mate. VALOX had one thread and at most five seconds per move; Stockfish had
  four threads and the same per-move cap. This two-game sample is diagnostic
  only and cannot estimate VALOX Elo.
- A later 2-game browser match ended VALOX 0-1-1: one stalemate draw as White
  and one checkmate loss as Black. Both games used the same 8-ply opening with
  colors reversed. The complete PGN is legal and preserved with browser state
  and full position analysis under `tools/engine-match/results/`.
- Stockfish 19 re-analyzed all 353 positions at 1 million nodes per position
  with 4 threads and 2 GB hash. No VALOX move in the drawn game lost >=0.50
  pawns in this pass. In the loss, several fixed-node candidate mistakes
  appeared, but spot checks changed some rankings; none of the largest late
  swings can be called the loss cause because Stockfish already had a forced
  win. The detailed move list, method, caveats, and experiment data are in
  `tools/engine-match/results/MATCH-POSTMORTEM-20261112.md`.
- The browser server did not preserve per-move UCI `info` lines. Its known
  configuration was VALOX 1 thread / Stockfish 4 threads, 64 MB hash each,
  depth cap 60 and 5 seconds per move. Five-second same-position spot checks
  found VALOX at depth 10-15 with about 0.21-0.57M nodes, versus Stockfish at
  depth 18-76 with about 18.6-50.5M nodes; these are post-match diagnostics,
  not the missing match logs and not a playing-strength comparison.
- The match does not prove a move-generation, NNUE, move-ordering, or time
  management defect. Every move was legal, and there was no timeout. Do not
  change the model or pruning based on this two-game result alone.
- Absolute anchor, 2026-11-17: the accepted build scored 80.0% (19-10-1)
  against Stockfish 19 limited to `UCI_Elo=2800` and 53.3% (6-4-20,
  +23.2 +/- 72.6) against `UCI_Elo=3000`, at `tc=10+0.1`, one thread and
  256 MB hash for both engines, with the fixed 8-ply opening set. This places
  VALOX at roughly the 3000 level under these local conditions. It is not an
  official or rating-list Elo; Stockfish's limited-strength scale is not
  linear, and the campaign's normal four-thread configuration is stronger.
  Full table and caveats: `docs/STRENGTH-CAMPAIGN-2026-11.md`.
- A bitboard-based position-hash candidate passed correctness checks and
  measured +5.1% median throughput on the fixed four-position depth-10 bench.
  In a 20-game paired 5+0 A/B match it scored 9.5/20 against the previous
  build (baseline 10.5/20); the estimated 17 Elo baseline edge had a wide
  +/-105 Elo interval and is inconclusive. The candidate is retained as a
  performance change, not as a proven playing-strength gain.
- ETW profiling on the Ryzen 7 5700X exposed repeated NNUE threat geometry as
  the largest measured local hot spot (`square_attacks`: about 52% of VALOX's
  sampled CPU weight). A per-attacker attack-map reuse optimization passed
  feature-index, accumulator, UCI, and 140-position differential-perft checks.
  Four interleaved depth-10 benchmark runs measured +12.6% median NPS with
  identical best moves and node counts. A paired 20-game 5+0 self-play scored
  12/20 for the candidate; the estimate was +70 +/-124 Elo, 87.6% LOS, and is
  not statistically significant. Keep it as a speed optimization only. Full
  trace, raw runs, PGN, and Cute Chess log are documented in
  `tools/engine-match/results/NNUE-THREAT-CACHE-20261112.md`.
- A follow-up profile showed `square_attacks` still consumed 46.75% of
  VALOX's sampled CPU weight. Replacing its 64-square scan with direct leaper
  destinations and slider rays matched the reference for all 768
  piece/square/side combinations. It passed tests, Clippy, UCI, and 140-position
  differential perft. Four interleaved fixed-depth runs measured a further
  +51.7% NPS with identical nodes and best moves. In paired 20-game self-play,
  it scored 12.5/20; the +88.7 +/-89.2 Elo estimate (97.1% LOS) is encouraging
  but still too small to establish a playing-strength gain. Data and artifacts
  are in `tools/engine-match/results/NNUE-THREAT-CACHE-20261112.md`.
- The post-rays profile shifted the top hot spot to NNUE `apply_delta` (30.14%
  sampled process CPU), then NNUE output evaluation (25.77%). Splitting the
  16-bit piece and 8-bit threat accumulation loops passed all 18 tests and
  Clippy. Four interleaved depth-10 runs measured +1.9% NPS with identical
  nodes and moves. In 20 paired games it scored 13.5/20; +127 +/-134 Elo and
  97.4% LOS are promising but remain statistically inconclusive. Its rationale
  and artifacts are recorded in the NNUE experiment report.
- 2026-11-18: a fresh ETW profile put `Position::is_attacked` at 17.5% of VALOX
  samples, the largest cost outside NNUE updates. Replacing its mailbox scan
  with const leaper masks and directional ray/nearest-blocker scans passed a
  new parity test across about 6,600 positions (37 tests total), Clippy with
  `-D warnings`, the wasm check, and the tactical suite at depths 12 and 16.
  Bench depths 8-12 are identical in nodes and best moves; six interleaved
  `--bench 12` pairs measured 5,913.5 ms versus 5,088.5 ms (-13.95% time,
  about +16.2% NPS) at 1,216,373 nodes. The repeat profile reduced
  `is_attacked` to 5.6% of samples. Pinned as
  `build/valox-fast-attacks-20261118.exe`, SHA-256
  `E59A94C78B5F9E4978D2825710D1EE3E862B9343D955C42468784062C6E1AA19`.
  This is a throughput change only; there is no Elo claim because the
  fixed-depth search is bit-identical. Full section: `docs/STRENGTH-CAMPAIGN-2026-11.md`.
- 2026-11-18: the accumulator width was halved to `i16`, matching the Reckless
  reference for the same network. `nnue-bench` checksums (`45968` evaluation,
  `8871920` update) are unchanged for both AVX2 and forced scalar updates, as
  are bench nodes and best moves at depths 8-12. Six interleaved `--bench 12`
  pairs measured 4,914.5 ms versus 4,402 ms (-10.43% time, about +11.65% NPS)
  at 1,216,373 nodes. The repeat profile reduced `apply_feature_avx2` from
  30.8% to 23.1% of samples and its absolute sample count by 36.6%. Pinned as
  `build/valox-i16-acc-20261118.exe`, SHA-256
  `1530B5A4BA4E15C670C347EE92B53FD6174C6FC0B674457FE540A5DCAD89D929`.
  Throughput only; no Elo claim. A follow-up that applied four features per
  accumulator pass halved the update kernel's samples in the profile but was
  not reproducibly faster end to end (ABBA runs gave +1.7% and -1.1% time), so
  it was reverted and the single-feature updater kept. Compile-time `const`
  threat tables were also neutral (ABBA median +0.96% time) and were reverted.
- 2026-11-18: NNUE king lookup uses the existing king bitboard instead of a
  64-square mailbox scan in `reset` and `push`; parent king squares come from
  the move origin. Checksums (`45968`, `8871920`), nodes and moves are
  unchanged. Six ABBA `--bench 12` rounds gave a median -5.22% time (all
  negative); five `--bench 13` rounds gave -3.05%. VALOX profile samples fell
  9.8% at identical nodes, with `NnueEvaluator::push` samples down 19.6%.
  Pinned as `build/valox-kingsq-20261118.exe`, SHA-256
  `E45D367E2F993AFB0E7CC75F350F54CC4411C553474609BC47A5711B0CAF03FF`.
  Throughput only; no Elo claim.
- 2026-11-18: `is_attacked` now tests the nearest ray blocker directly against
  the side's rook/queen and bishop/queen bitboards instead of looking the
  square up in the mailbox. The attack parity test and all 37 tests pass,
  checksums/nodes/moves are unchanged. Six ABBA `--bench 12` rounds gave a
  median -3.12% time; eight `--bench 11` rounds -1.41%; ten `--bench 13`
  rounds were neutral within noise. Profile samples for `is_attacked` fell
  15.1%. Pinned as `build/valox-attackmasks-20261118.exe`, SHA-256
  `EE99AF3406AB82C996802C6DD580A924D272BF251BF6B65A7BEBD4FB866EA8D6`.
  Throughput only; no Elo claim.
- 2026-11-18: legal move generation now runs the make/unmake legality verifier
  only for checks, king moves/castling, en passant and absolutely pinned
  pieces; unpinned non-king moves are copied directly. Full perft tests, 140
  differential-perft cases at depth 3 and 60 at depth 4 (including Chess960),
  Clippy, wasm and the tactical suite pass with identical nodes/moves and NNUE
  checksums. Six ABBA `--bench 12` rounds gave a median -9.62% time; four
  `--bench 13` rounds -10.47%. `make_move` profile samples fell 66.7%. Pinned
  as `build/valox-legalfast-20261118.exe`, SHA-256
  `36592131D2711D0E7800F2C0322B42AED58C43C62E4A890803B28182DFEEE524`.
  Throughput only; no Elo claim.
- 2026-11-18: NNUE dirty-threat detection uses reverse attack masks per
  changed square instead of scanning 64 squares and their attack maps.
  Checksums and bench nodes/moves at depths 8-13 are unchanged; 37 tests,
  Clippy, wasm and the tactical suite pass. Six ABBA `--bench 12` rounds gave
  a median -7.36% time; `NnueEvaluator::push` profile samples fell 70.6%.
  Pinned as `build/valox-dirtymask-20261118.exe`, SHA-256
  `C5BEC4B322B2E608DC4B7AE6320A8BE8D4E4B5A8480983F67C1A640A8F6F37DE`.
  Throughput only; no Elo claim.
- 2026-11-18: move-ordering scores use a reusable 256-entry buffer in
  `Searcher` instead of a 4 KB stack array initialized per node. Nodes, moves
  and NNUE checksums are unchanged; Clippy, wasm, the tactical suite and all
  37 tests pass. Six ABBA `--bench 12` rounds gave a median -1.68% time; four
  `--bench 13` rounds -2.05%; `order_moves` profile samples fell 44.6%.
  Pinned as `build/valox-orderscores-20261118.exe`, SHA-256
  `A4F8D21BDFD55369365100744AD6EE01B6D9075DB80636A9D41A8CC6D5EB030D`.
  Throughput only; no Elo claim.
- 2026-11-18: release builds use `codegen-units = 1` so LLVM can inline hot
  helpers across modules; fat LTO was tested and was consistently slower. Nodes,
  moves and NNUE checksums unchanged; tests, Clippy, wasm and the tactical
  suite pass. Median -3.48% time over five ABBA bench-12 rounds, -2.26% over
  four bench-13 rounds. Pinned as `build/valox-cgu1-20261118.exe`, SHA-256
  `C64429012896A34D4A6A729F5044FDC2214DBAE325262BC0712094612D45D03E`.
  Throughput only; no Elo claim.
- 2026-11-18: quiescence generates only captures and promotions when not in
  check, in the same relative order as the full generator; in-check nodes and
  zero-capture semantics are unchanged. Bench nodes/moves at depths 8-13 and
  NNUE checksums are identical; 38 tests, Clippy, wasm, Linux-target checks,
  differential perft and the tactical suite pass. Three ABBA `--bench 12`
  series gave medians of -1.56%, -1.69% and -0.72%; a bench-13 series gave
  -1.97% (15 of 21 rounds faster, conservative reading about 1%). Pinned as
  `build/valox-capture-gen-20261118.exe`, SHA-256
  `21053119F9FCA0309262DB6F5B7C0AA6CB656F65C4AF4C6E354CAA022045557E`.
  Throughput only; no Elo claim.
- 2026-11-19: `Position::search_hashes` computes the full transposition key and
  the clock-independent repetition key from a single `hash_without_clock` pass;
  `negamax` previously ran that pass twice per node. Values are identical by
  construction. Bench nodes/moves at depths 8-13 and NNUE checksums are
  unchanged; 38 tests, fmt, Clippy, wasm, Linux-target checks, differential
  perft and the tactical suite pass. Six ABBA `--bench 12` rounds gave a median
  -1.45% time (6/6 faster, mean -1.92%); five bench-13 rounds gave -0.83%.
  Pinned as `build/valox-hashpair-20261119.exe`, SHA-256
  `362AF47FFD692F142A2E723D9B81C25149CB2B68DA611876A5F74A7CD1D0C782`.
  Throughput only; no Elo claim.
- 2026-11-19: the NNUE incremental update no longer rebuilds every outgoing
  threat edge of every geometric attacker. Changed attacker squares get a full
  rebuild; unchanged leapers get a single edge to the changed target, and
  unchanged sliders regenerate only the affected ray's first-blocker edge.
  Accumulators are bit-identical. Bench depths 8-13 and NNUE checksums
  (`45968`/`8871920`) are unchanged; 38 tests, fmt, Clippy, wasm, Linux-target
  checks, differential perft and the tactical suite pass. Eight ABBA
  `--bench 12` rounds: median -7.58% time (8/8 faster); six bench-13 rounds:
  -7.24% (6/6 faster). Pinned as `build/valox-threat-ray-20261119.exe`,
  SHA-256
  `D1DEBC3333B42E481E94B0A4A2169CFF632F9078FA6596DB8CDAB09A38D2E958`.
  Throughput only; no Elo claim.

## Prioritized work

### 1. Establish a clean search and strength baseline

- Pin the release binary, source revision, network hash, CPU, OS, engine options,
  and opening set for every run.
- Measure fixed-node search on a set of tactical, quiet, and endgame positions
  at 1/2/4/8 threads, repeating samples after warm-up.
- Run paired-color A/B matches for any search change with identical openings,
  time/node limits, hash, and thread count. Preserve PGN and engine logs.
- Use SPRT or confidence intervals for acceptance; do not treat a two-game
  browser match or NPS alone as strength evidence.
- Persist per-move UCI diagnostics from browser games (engine/version hash,
  depth, nodes, NPS, time, score, and PV) before using them for subsystem-level
  conclusions.
- Compare VALOX at 1 and 4 threads in paired-color matches against the same
  fixed opponent and openings. The browser match's 1-vs-4-thread allocation is
  an obvious confounder, but multi-thread search should only be preferred if
  controlled results support it.

### 2. Audit root parallel search

`src/engine.rs::search_with_threads` currently partitions root moves statically,
copies the transposition table into each worker each iteration, searches each
root move with a full window, and does not carry the prior iteration's best-move
ordering into the next root iteration. Measure the cost and tactical behavior
before changing this design. Compare against the single-thread search and retain
the simpler path if a candidate does not win a controlled match.

The hot-path position key now mixes the maintained piece bitboards and position
state instead of hashing all 64 mailbox squares. Keep this optimization only as
a measured NPS gain; the 20-game strength test did not show a statistically
reliable playing-strength change. Caching that key in `Position` and updating
it incrementally per make was tested in change 26 and rejected: it was 6-9%
slower on `--bench 12`, because maintenance runs per make rather than per
visited node while the remaining one-pass computation is only about 1.4% of
runtime.

### 3. Audit search correctness and time handling

- Differential-test search behavior for repetition, fifty-move draws, mate
  distance, interrupted searches, node limits, and TT bound reuse.
- Review pruning and reductions with tactical and quiet-position suites.
- Check that incomplete iterations never overwrite the last completed best move
  and that UCI `stop` always returns promptly with a legal move.

### 4. Evaluate NNUE and evaluation improvements

- Preserve the known-good embedded Reckless v60 model while licensing is unclear.
- Build a diverse, game-separated training/validation pipeline; compare WDL
  calibration on held-out games and then run paired engine matches.
- Reject networks that only improve training loss or correlated position scores.
- Do not publish model weights or a binary embedding them until rights are clear.
- Investigate endgame evaluation only after building a held-out endgame FEN/game
  suite. The 2026-11-12 match suggests poor defensive results in the pawn ending,
  but it does not establish that NNUE caused the loss.

### 5. Tournament and release readiness

- Keep UCI, standard chess, Chess960, Windows/Linux builds, Syzygy, and stress
  checks green.
- Valox source is dual-licensed under MIT OR Apache-2.0. The Reckless v60 model
  rights remain unresolved; do not distribute binaries, archives, or CI
  artifacts that contain the weights.
- Weight-free builds can load a compatible model at runtime through the UCI
  `EvalFile` option, `--evalfile`, or `VALOX_EVALFILE`. This path is
  performance-neutral and keeps the third-party weights out of release
  archives.
- Linux x86_64 musl cross-builds are available and produce a static ELF. The
  `linux-nnue` CI job tests the NNUE path with a synthetic same-layout network,
  compares a runtime-loaded build against the embedded build, and exercises the
  static binary without third-party weights.
- Use `tools/build-release.py` for release archives. It defaults to a
  weight-free binary and refuses to package an embedded network without
  `--allow-network-redistribution`.
- Update `docs/CCC_READINESS.md` with pinned artifacts and reproducible results.
- Do not contact or submit to CCC without the user's explicit authorization and
  a cleared release artifact.

## Acceptance gate for engine changes

1. Reproduce the existing baseline and record exact commands/configuration.
2. Make one isolated change.
3. Run formatting, tests, Clippy, release build, UCI/perft/tactical checks.
4. Run controlled paired matches against the prior VALOX build.
5. Keep a change only when the results support it and no correctness regression
   appears; otherwise revert it and record the result.
