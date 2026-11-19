# VALOX strength campaign — 17 November 2026

This note tracks concrete, measurement-led work toward a stronger VALOX. It is
not an Elo claim and not a promise that VALOX will become the world's strongest
engine. Full-strength Stockfish is still far above VALOX; the local anchor in
this document places VALOX near 3000 under one specific limited-opponent
configuration, which is not a rating-list comparison. Closing the remaining gap
needs large-scale NNUE training, a production-grade SMP search, and long
controlled tuning. This document records the first steps of that effort.

## Baseline

- Source: commit `9e16ae0` plus the local uncommitted work present on
  2026-11-16 (bitboard hash, direct NNUE attack maps, delta-loop
  specialization, AVX2 L2, `nnue-scalar`, `nnue-bench`, packaging changes).
- Pinned binary: `build/valox-strength-baseline-20261116.exe`,
  SHA-256 `8F0752AF3C6C7DA9A1E84981458966223A67C9C74C51A9CA73C699DFE5FFE4BE`.
- `cargo build --release` succeeded; `cargo test --release` passed 21 tests;
  Clippy with warnings denied passed.
- `valox.exe --bench 10`: 388,049 nodes, 113,763 NPS on the first isolated run
  (139,257 NPS in the later clean interleaved run; the difference is host
  load/thermal variance, not an engine change).
- CPU: AMD Ryzen 7 5700X (8 cores / 16 logical processors), Windows.

## Accepted change 1: AVX2 NNUE feature updates

`apply_feature` now has an AVX2 path that widens eight 16-bit piece weights or
eight 8-bit threat weights at a time with `_mm256_cvtepi16_epi32` /
`_mm256_cvtepi8_epi32`, then adds or subtracts them from the i32 accumulator.
The scalar path remains as the reference and fallback. AVX2 is detected at
runtime; `VALOX_NNUE_SCALAR_UPDATES=1` forces the scalar path for A/B tests.

Validation:

- New test `avx2_feature_update_matches_scalar` compares both paths for
  features 0, 1, 17, 767 x piece/threat x add/subtract. It passes.
- `cargo test --release`: **24 tests passed**.
- `nnue-bench` on 300 positions, identical checksum `8871920` for both paths:
  - scalar: 4.10 us per push/pop pair
  - AVX2: 2.53-2.85 us per push/pop pair
- `valox.exe --bench 10`, alternating runs, identical 388,049 nodes and best
  moves:

| Mode | Time | NPS |
|---|---:|---:|
| Scalar | 3,347 ms | 115,939 |
| AVX2 | 2,898 ms | 133,902 |
| Scalar | 3,428 ms | 113,199 |
| AVX2 | 2,868 ms | 135,302 |

Median throughput gain: **+17.5%** with no change in searched nodes or chosen
moves. This is a performance result, not an Elo estimate.

## Accepted change 2: root-split SMP rewrite

The previous `search_with_threads` copied up to `tt_limit / workers` entries
into every worker on every iteration, searched every root move with a full
window, and merged unbounded worker tables back into the parent. At depth 10
with 4 threads it searched 852,893 nodes and took 2.29 s, versus 128,674 nodes
and 0.92 s for one thread.

The new root split:

1. searches the previous iteration's best move first at full depth with a
   small aspiration window;
2. uses that exact score as a threshold;
3. partitions all remaining root moves across workers;
4. tests each remaining move with a null window and re-searches only on a
   fail-high;
5. uses one empty, bounded worker table per worker instead of copying the
   parent table;
6. merges worker entries only while the parent table is below its limit;
7. keeps the previous completed best move if any worker is interrupted;
8. orders the non-best root moves by their previous exact scores.

Clean fixed-depth benchmark, `tools/engine-match/bench-threads.py`, depth 10:

| Threads | Old nodes | Old time | Old NPS | New nodes | New time | New NPS |
|---:|---:|---:|---:|---:|---:|---:|
| 1 | 128,674 | 924 ms | 139,257 | 128,674 | 804 ms | 160,042 |
| 2 | 749,093 | 2,978 ms | 251,542 | 179,782 | 856 ms | 210,025 |
| 4 | 852,893 | 2,288 ms | 372,767 | 232,179 | 897 ms | 258,839 |
| 8 | 737,058 | 1,351 ms | 545,564 | 235,677 | 729 ms | 323,288 |

At 4 threads the new search needs **3.7x fewer nodes** and **2.6x less wall
time** to reach the same depth. The NPS columns are lower because the new
search does not waste nodes; time-to-depth is the meaningful number.

Fixed-time check on the start position, 3 s per move:

| Threads | Old depth | New depth |
|---:|---:|---:|
| 1 | 11 | 12 |
| 2 | 10 | 12 |
| 4 | 10 | 13 |
| 8 | 11 | 11 |

The 8-thread result is not better than 4 threads on this host; more workers
increase memory/contention and the sequential best-move search limits scaling.
Two to four threads are the useful range for now.

### Corrected thread-count semantics (v2)

The table above was measured before the worker-count fix: `Threads=N` spawned
`N` workers in addition to the main search thread, i.e. `N + 1` actual threads.
The UCI option now means **total search threads**: `N = 1` main thread,
`N > 1` main thread plus `N - 1` root-split workers.

Corrected build `build/valox-smp-avx2-v2-20261117.exe`,
SHA-256 `90E6F07B843B1541947463FE79EAE6806B813E20AE93607FD18025026F7BC8EF`.
Start position:

| Threads | d10 nodes | d10 time | 1 s depth | 3 s depth |
|---:|---:|---:|---:|---:|
| 1 | 128,674 | 834 ms | 10 | 12 |
| 2 | 244,623 | 1,683 ms | 9 (incomplete 10) | 10 |
| 4 | 236,769 | 1,001 ms | 10 | 12 |
| 8 | 231,495 | 775 ms | 10 | 13 |

Two threads is currently *worse* than one on this host. The root split
searches the best move sequentially and leaves every other root move to the
single worker, so there is almost no parallelism; the null-window re-searches
add overhead. The design only pays off from about four threads upward, and
the sequential best-move search remains the scaling bottleneck. This is the
main reason the next SMP step should be a shared transposition table with
lazy-SMP style parallel search rather than more partitioning.

Note: the baseline binary used in matches was built before this semantic fix,
so `Threads=4` gives it five actual threads versus four for v2. The match
result therefore understates v2's equal-thread advantage slightly; the
measurements above are the cleaner efficiency evidence.

## Accepted change 3: time handling and stop latency

- The time check now runs every 512 nodes instead of every 2,048, reducing the
  worst-case overshoot at fast controls.
- Sudden-death budget changed from `time / 60 + 0.75 * increment` to
  `time / 40 + 0.8 * increment`, with a 30 ms communication/scheduling margin
  (15 ms for `movetime`) capped at one fifth of very small budgets.
- `movestogo` is still honored when supplied.

This is a practical strength change at fast controls, not a measured Elo result
yet.

## Correctness fixes

- `src/main.rs` now calls `Searcher::set_uci_chess960(chess960)`, so UCI
  `info ... pv` uses Chess960 castling notation, not only `bestmove`.
- `src/syzygy.rs` refuses to probe positions that still have castling rights.
  Syzygy tables assume no castling, so a WDL result from such a position can be
  wrong.
- Syzygy now passes the en-passant square only when a legal en-passant capture
  exists, as the tablebase format expects.
- Two new tests cover the threaded search: a mate-in-one must be found with 4
  threads, and the start position must return a legal move with 4 threads.

## Rejected change: countermove + improving heuristic batch

A batch adding a countermove heuristic and an "improving" flag for
razoring/reverse-futility/LMP margins was tested at fixed depth 9, 30 games,
paired colors, same openings:

- Baseline: 7 wins, 2 losses, 21 draws (17.5/30, 58.3%)
- Candidate: 2 wins, 7 losses, 21 draws
- Cute Chess: **+58.5 Elo for the baseline**, LOS 95.2% for the baseline

The candidate also searched 15-20% more nodes at depth 10. It was reverted.
This is a useful negative result: the standard-looking heuristics did not help
this search at this time control.

## Accepted match: combined SMP + AVX2 build vs baseline

Candidate `build/valox-smp-avx2-v2-20261117.exe`,
SHA-256 `90E6F07B843B1541947463FE79EAE6806B813E20AE93607FD18025026F7BC8EF`.
30 paired games, `tc=10+0.1`, Hash 64, Threads 4, concurrency 2, fixed 8-ply
openings, seed 20261117:

- v2 scored **24 wins, 5 draws, 1 loss** (26.5/30, 88.3%).
- Cute Chess estimated **-351.7 +/- 190.1 Elo for the baseline**, LOS 100% for
  v2.
- Every decisive game ended by mate; there were no time forfeits.
- Average game length 96 plies (minimum 37, maximum 245).

Caveats: the baseline predates the thread-count fix, so `Threads=4` gave it
five actual threads versus four for v2. The result is therefore conservative
about v2's equal-thread advantage. Thirty games also leave a wide confidence
interval; this is a strong regression check, not a rating claim.

Three rejected match setups, recorded so they are not repeated:

1. `-each tc=inf st=1` makes cutechess send `go infinite`; the engine then
   relies on a late `stop`. Use `-each st=1`, which sends `go movetime 1000`.
2. `-each st=1` is not a valid comparison with the old baseline: it uses the
   full 1000 ms with no safety margin and lost every game on time after the
   8-ply book. That match ended 30-0 by time forfeit and is not a strength
   result. The v2 margin is a real robustness improvement.
3. Fixed-depth matches at `Threads=4` are impractical against the old
   baseline: its root split searches full-window subtrees with copied worker
   tables and can spend minutes on one move. Bounded time controls such as
   `tc=10+0.1` are required for A/B matches with it.

## Accepted change 4: cached NNUE attack maps

Threat-feature generation used to compute each attacker's geometric attack map
from scratch, even though the map depends only on piece kind, oriented square
and side. `attack_map` now builds the 12 x 64 maps once (about 49 KB) and
returns a reference; the previous direct-ray implementation remains as the
builder, and the 64-square reference version remains as a test oracle.

Validation and measurement:

- `cargo test --release`: 24 tests passed, including the 768-map comparison
  against the reference scanner and all incremental-accumulator tests.
- Clippy with warnings denied: passed.
- Six interleaved `--bench 10` pairs against v2, alternating order:

| Build | Median time | Median NPS |
|---|---:|---:|
| v2 (uncached) | 2,997 ms | 129,695 |
| Cached attack maps | 2,879 ms | 135,233 |

The cached build was faster in all six pairs, **+4.1% time and +4.3% NPS**,
with identical 388,049 nodes and identical best moves in every run. Binary:
`build/valox-attack-table-20261117.exe`. Keep as a measured performance
change; no strength claim beyond the throughput gain.

## Accepted change 5: skip static evaluation when in check

`negamax` used to compute the NNUE static evaluation before checking the
in-check flag, although razoring, reverse futility, null-move and LMP all
require a quiet position. The evaluation is now skipped when in check. The
same pass also reuses the already-computed `quiet` flag for the cutoff
history update instead of asking `is_capture` a second time.

Validation: 24 tests and Clippy passed. Six interleaved `--bench 10` pairs
against `build/valox-attack-table-20261117.exe` produced identical 388,049
nodes and identical best moves in every run, with median times of 2.747 s and
2.759 s (**within noise**). Kept as a behavior-preserving cleanup; binary
`build/valox-lazy-eval-20261117.exe`.

## Accepted change 6: cached static evaluation in transposition entries

Shallow TT hits already carry the position key but used to force a fresh NNUE
forward pass for the static evaluation. `Entry` now stores the static eval as
an `i16` with an `i16::MIN` sentinel for in-check nodes. To keep the entry at
16 bytes, the bound moved into two flag bits and the replacement age into the
remaining six bits (`flags = bound | (age & 0x3f) << 2`); a test asserts
`size_of::<Entry>() == 16`. Cached evals are only read when the node is not in
check, exactly where the stored value was produced, so search behavior is
unchanged.

Validation and measurement:

- `cargo test --release`: 25 tests passed; Clippy with warnings denied passed.
- Six interleaved `--bench 10` pairs against the lazy-eval build, alternating
  order:

| Build | Median time | Median NPS |
|---|---:|---:|
| Lazy eval | 2,748 ms | 141,213 |
| TT eval cache | 2,732 ms | 142,066 |

All 12 runs had the identical signature `g1f3 / c1g5 / e3c5 / c4c5` and
388,049 nodes. The **+0.6% time** gain is inside benchmark noise, so this is
kept as a behavior-preserving change that avoids redundant NNUE passes in
real searches, not as a strength claim. Binary:
`build/valox-tt-eval-20261117.exe`.

## Accepted change 7: AVX2 dot product for the NNUE L1 layer

The 768-to-16 first layer was a scalar i32 loop over 192 groups of four
features. It is now an AVX2 int8 dot product: the four transformed inputs are
broadcast with `_mm256_set1_epi32`, multiplied against 32 weight bytes with
`_mm256_maddubs_epi16`, and accumulated as i32 with `_mm256_madd_epi16`. The
transformed values are at most `(255 * 255) >> 9 = 127`, so a pair of products
cannot saturate the signed 16-bit intermediate; the vector result is
bit-identical to the scalar loop. A scalar fallback remains, and a test
compares both paths for every output bucket and 32 randomized samples.

Measurement:

- `nnue-bench` on 300 positions: evaluation time **2.753 -> 0.851 us**
  (-69%), identical checksum 183872.
- Six interleaved `--bench 10` pairs against the TT-eval build:

| Build | Median time | Median NPS |
|---|---:|---:|
| TT eval cache | 2,713 ms | 143,034 |
| AVX2 L1 | 2,171 ms | 178,742 |

That is **-20.0% time and +25.0% NPS** with identical 388,049 nodes and
identical best moves in all 12 runs (single signature `g1f3 / c1g5 / e3c5 /
c4c5`). 26 tests and Clippy passed. Binary:
`build/valox-avx2-l1-20261117.exe`.

## Accepted change 8: cached attack ranks for threat features

Building one threat feature computed its rank by scanning the whole 64-square
attack map up to the target, for every candidate pair. The rank is fixed per
(piece kind, square, side, target), so a 12 x 64 x 64 `u8` table now stores it
(255 means "not attacked"). The per-attacker `attack_map` lookup and the
redundant target-map check were removed as well.

Validation and measurement:

- 27 tests passed, including a new exhaustive test that compares every cached
  rank with the prefix scan. Clippy passed.
- Six interleaved `--bench 10` pairs against the AVX2-L1 build:

| Build | Median time | Median NPS |
|---|---:|---:|
| AVX2 L1 | 2,196 ms | 176,713 |
| Cached ranks | 2,054 ms | 188,889 |

**-6.5% time and +6.9% NPS**, identical 388,049 nodes and a single best-move
signature in all 12 runs. `nnue-bench` push/pop dropped from 2.504 to
2.260 us per pair with identical checksums. Binary:
`build/valox-rank-cache-20261117.exe`.

## Accepted change 9: AVX2 accumulator transform

The 768-value transform (clamp both accumulator halves to 0..255, multiply,
shift right by nine) ran as scalar code. It now processes eight lanes at a time
with AVX2 min/max, `mullo`, `srli` and lossless packs; the scalar version
remains as fallback and as a test oracle. A randomized test compares both
paths for 64 accumulator samples and both sides to move.

- 28 tests and Clippy passed.
- `nnue-bench` evaluation: 0.858 -> 0.805 us (-6.2%), identical checksum.
- Six interleaved `--bench 10` pairs against the rank-cache build:
  2,054 -> 2,015 ms and 188,972 -> 192,602 NPS (**-1.9% time, +1.9% NPS**),
  identical nodes and best moves. The build won five of six pairs.

Binary: `build/valox-avx2-transform-20261117.exe`.

## Accepted match: NNUE SIMD build vs TtEval control

30 paired games, `tc=10+0.1`, Hash 64, Threads 4, concurrency 2, fixed 8-ply
openings, seed 20261117. Control `build/valox-tt-eval-20261117.exe`; candidate
`build/valox-avx2-transform-20261117.exe`, which adds the AVX2 L1 dot product,
cached attack ranks and the AVX2 accumulator transform.

- NnueSimd scored **15 wins, 7 losses, 8 draws** (19/30, 63.3%).
- Cute Chess reported **-94.9 +/- 113.5 Elo for TtEval**, LOS 95.6% for
  NnueSimd.
- Every decisive game ended by mate; there were no time forfeits.
- Average game length 145.2 plies (minimum 63, maximum 268).

The two engines have identical search behavior, so this match isolates the
value of the NNUE speedup at a fast time control. Thirty games still leave a
wide interval; the result is evidence that the throughput gain converts into
strength, not a precise rating. Match log:
`tools/engine-match/results/nnue-simd-tc10-20261117.txt`.

## Accepted change 10: bitset feature diff

`push` regenerated both feature lists, sorted them and merged them to find
additions and removals. The generators now leave the lists unsorted, and
`diff_feature_bits` marks old features in a bitset, clears matched bits while
scanning the new list, and emits the still-set bits as removals. The piece
bitset covers all ten input buckets (7,680 bits); the threat bitset covers
66,864 bits. Only touched words are revisited, so the operation is O(features)
without sorting. Feature indices are unique per position, which makes one bit
per index sufficient. The existing incremental-vs-full-refresh and
optimized-vs-brute-force tests passed unchanged; a first version with a
too-small piece bitset was caught by those tests and fixed.

Validation and measurement:

- 28 tests and Clippy passed.
- `nnue-bench` push/pop: 2.186 -> 1.999 us per pair (-8.6%), identical
  checksums.
- Six interleaved `--bench 10` pairs against the AVX2-transform build:

| Build | Median time | Median NPS |
|---|---:|---:|
| AVX2 transform | 2,027 ms | 191,484 |
| Bitset diff | 1,908 ms | 203,440 |

**-5.9% time and +6.2% NPS**, identical 388,049 nodes and best moves in all 12
runs. Binary: `build/valox-bitset-diff-20261117.exe`.

## Accepted change 11: truly incremental feature generation

The full generators were split into per-square helpers (`piece_feature_index`,
`threat_features_for_square`), then `push` stopped regenerating both complete
feature lists. It now reconstructs the parent position from the undo record,
derives the changed squares (from, to, capture square, castling rook squares)
and only emits the dirty feature sets:

- Piece features: only squares whose contents changed. A king move changes the
  input bucket and orientation of every feature of its side, so that
  perspective falls back to a full rebuild.
- Threat features: attackers cannot change their feature set unless a changed
  square lies on their geometric attack map or they stand on a changed square,
  so only those squares are regenerated for both parent and child.

Each dirty old/new pair goes through the existing bitset diff, and only the
resulting delta is stored per frame; `pop` applies the reverse delta. No active
feature lists are kept: an earlier version maintained them via linear search,
which made the engine 12% slower despite a faster accumulator update. Removing
that bookkeeping was the decisive fix. `Undo` is now doc-hidden `pub` with
crate-visible fields so the NNUE module can read it; `play_tracked` exists for
tests and benchmarks.

Validation and measurement:

- 28 tests and Clippy passed; the incremental-vs-full-refresh tests now run
  through the new path and still match exactly, including promotions,
  en passant, castling and long random lines.
- `nnue-bench` push/pop: 1.999 -> 1.547 us per pair (-22.6%), identical
  checksums.
- Six interleaved `--bench 10` pairs against the helper-refactor build:

| Build | Median time | Median NPS |
|---|---:|---:|
| Feature helpers (full rebuild + diff) | 1,826 ms | 212,463 |
| Incremental features | 1,688 ms | 229,955 |

**-7.6% time and +8.2% NPS**, identical 388,049 nodes and the same four best
moves in all 12 runs. Binary: `build/valox-incremental-features-20261117.exe`.

## Accepted change 12: shared lock-free transposition table

Each worker used to own a private `HashMap<u64, Entry>`, and after every
completed depth the main thread merged worker entries one by one. A 64 MB
`Hash` setting therefore held only about 1.4 million entries, and workers
could not learn from each other during the search.

The table is now one flat array of 16-byte slots shared by all threads:

- Each slot is two atomics: a verification word (`key ^ data`) and the packed
  entry. Probes are wait-free; a write that races with a probe can only
  produce a mismatched verification word, so torn entries are rejected.
- The entry fits in one 64-bit word: best move (16 bits), depth (8), score
  (16), cached static eval (16), replacement age (6), bound (2). The move
  encoding stores the rook origin for castling and derives the king target
  from the rook's side, which is exactly how castling rights are defined.
  This keeps every field and reproduces the original `Move` bit-for-bit.
- Replacement keeps deeper results and ages out entries from older searches.
  The generation is shared by the root searcher and its workers.
- `Hash` now maps directly to entries: 64 MB is 4 million slots instead of
  roughly 1.4 million HashMap entries. `Searcher::tt` is an `Arc`, so the
  table persists across searches, is cleared by `ucinewgame`/`Clear Hash` and
  is reallocated when `Hash` changes.

Validation and measurement:

- 35 tests, Clippy with `-D warnings`, and the wasm32 library check passed.
  New tests cover entry round-trips (including promotions and Chess960
  castling), slot verification, and the Hash-to-entry mapping.
- Single-threaded `--bench 10` at an equal 65,536 slots is 4.8-5.5% slower
  (median 2,330 vs 2,224 ms; 389,894 vs 388,049 nodes). The compact flat table
  is not faster per lookup in that tiny configuration; its advantage is
  memory density, which only pays off at realistic Hash sizes and with
  sharing.
- 30 paired games, `tc=10+0.1`, Hash 64, Threads 4, fixed 8-ply openings:

| Result | Score |
|---|---:|
| SharedTT vs Incremental | 9 wins, 3 losses, 18 draws (60.0%) |
| Elo | **+70.4 +/- 79.1 for SharedTT**, LOS 95.8% |

Every decisive game ended by mate; there were no time forfeits. Match log:
`tools/engine-match/results/shared-tt-tc10-20261117.txt`. Binary:
`build/valox-shared-tt-20261117.exe`.

## Rejected change: SEE pruning of losing captures

Move ordering already computes the static exchange evaluation (SEE) of every
capture. The candidate reused that value to skip captures with
`SEE < -85 * depth` at depths up to 5, except in check, when giving check, or
for the first ordered move. On the fixed-depth bench this cut nodes by 8%
(389,894 to 358,574) and time by 8-10%.

Two independent 30-game batches at `tc=10+0.1`, Hash 64, Threads 4, fixed
8-ply openings, decided otherwise:

| Batch | Result for SEE pruning | Elo |
|---|---:|---:|
| 1 | 8 wins, 4 losses, 18 draws (56.7%) | +46.6 +/- 79.6, LOS 87.6% |
| 2 | 5 wins, 12 losses, 13 draws (38.3%) | -82.6 +/- 96.8, LOS 4.5% |
| Combined | 13 wins, 16 losses, 31 draws (47.5%) | no gain |

The batches disagree in sign, and the combined score is below 50%, so the
pruning was rejected and removed. The node reduction did not translate into
game strength: SEE is a static approximation, and pruning those subtrees
apparently costs tactical accuracy in a way the bench signature does not show.
Match logs: `tools/engine-match/results/see-prune-tc10-20261117.txt` and
`...-batch2.txt`.

## Accepted change 13: reuse SEE from move ordering in quiescence

Quiescence used to recompute `static_exchange_eval` for each capture even
though `order_moves` had just computed the identical value for the same
position and move. `order_moves` now optionally writes the SEE values into an
array, and quiescence reads them by move index instead of evaluating again.
The main search passes `None`, so its hot path is unchanged.

This is intended to be behavior-preserving, and the measurement confirms it:

- `--bench 10` produces exactly the same 389,894 nodes and the same four best
  moves (`g1f3`, `c1g5`, `e3c5`, `c4c5`) as the shared-TT build.
- 12 alternating runs: median 2,279 to 2,192 ms (-3.8%), NPS 171,108 to
  177,831 (+3.9%).
- 35 tests, Clippy with `-D warnings`, and the wasm32 library check pass.

Because nodes and best moves are bit-identical, a confirmation match is not
required for correctness; the win is purely the removed duplicate SEE work.
Binary: `build/valox-see-reuse-20261117.exe`. This is the new benchmark
control.

## Rejected change: improving heuristic in RFP and LMR

The candidate kept a per-ply stack of static evaluations and defined a node as
"improving" when its static evaluation was higher than the value two plies
earlier. It then doubled the reverse-futility margin when not improving and
added one extra late-move reduction when not improving. The intent was
standard: trust a high static score less when the position has been getting
worse.

- `--bench 10`: 333,092 nodes instead of 389,894 (-14.6%). The start-position
  best move changed from `g1f3` to `c2c4`, so the pruning visibly changed
  search decisions rather than only saving work.
- Tactical suite at depth 12: 6/6 passed.
- 30 paired games, `tc=10+0.1`, Hash 64, Threads 4, seed 20261121:

| Result | Score |
|---|---:|
| Improving vs SeeReuse | 10 wins, 8 losses, 12 draws (53.3%) |
| Elo | +23.2 for Improving, LOS 68.0% |

The sign was positive but far from significant, and an earlier campaign batch
that combined countermove with an improving flag for razoring/reverse futility
had already lost 2-7-21 to the baseline. The node reduction therefore did not
translate into a repeatable strength gain, and the change was reverted. After
the revert the bench is again exactly 389,894 nodes with the same four best
moves. Match log: `tools/engine-match/results/improving-tc10-20261117.txt`.

## Accepted change 14: dynamic root scheduling with all requested threads

At each depth the main thread searched the previous best move with a full
window, then three workers split the remaining root moves into static
round-robin partitions. The partition containing the hardest move determined
the end of the depth, and the main thread sat idle after its single PV search.

Now the remaining root moves live in one queue behind an `AtomicUsize`. Every
worker pulls the next move, tests it against the PV threshold with a null
window, and re-searches only on a fail-high. `Threads=4` therefore starts four
candidate workers while the main thread waits for them; the main thread still
performs the serial PV search that establishes the threshold. At most
`Threads` searches run at once, so the CPU is not oversubscribed.

Measurement at fixed depth 13, Threads 4, Hash 256, eight alternating runs:

| Build | Median time | Median nodes |
|---|---:|---:|
| SeeReuse control | 2,714 ms | 771,444 |
| Dynamic root | **2,058 ms** | **669,200** |

That is **-24% time to complete the same depth** with fewer nodes. Three
independent 30-game batches at `tc=10+0.1`, Hash 64, Threads 4, fixed 8-ply
openings, seeds 20261122-20261124:

| Batch | Dynamic root score | Elo |
|---|---:|---:|
| 1 | 10 wins, 5 losses, 15 draws (58.3%) | +58.5, LOS 90.2% |
| 2 | 8 wins, 10 losses, 12 draws (46.7%) | -23.2, LOS 31.9% |
| 3 | 11 wins, 1 loss, 18 draws (66.7%) | +120.4, LOS 99.8% |
| **Combined** | **29 wins, 16 losses, 45 draws (57.2%)** | **+50.5, LOS ≈97.6%** |

The batches show how wide the 30-game error bars are, but the pooled result
and the fixed-depth speedup point in the same direction. Every decisive game
ended by mate; there were no time forfeits. Logs:
`tools/engine-match/results/dynamic-root-tc10-20261117*.txt`. Binary:
`build/valox-dynamic-root2-20261117.exe`.

## Rejected change: shared best-score threshold for root workers

The candidate published the best score found so far through an `AtomicI32`
(`fetch_max`) while the root workers were running. Every worker then tested
its next move against that raised value, so it could skip full re-searches for
moves that had become irrelevant.

The speed measurement looked promising at long depth but neutral at short
depth, and the game result was decisively negative:

- Fixed depth 15, Threads 4, five alternating runs: median 5,095 to 4,723 ms
  (-7.3%), median nodes 1,910,700 to 1,840,864. At depth 13 the medians were
  1,556 vs 1,563 ms, i.e. no difference.
- 30 paired games, `tc=10+0.1`, Hash 64, Threads 4, seed 20261125:

| Result | Score |
|---|---:|
| DynRoot vs SharedRoot | 19 wins, 5 losses, 6 draws (73.3%) |
| Elo | -175.7 for SharedRoot, LOS 0.2% |

Changing the comparison threshold mid-depth makes the reported root scores
inconsistent: a move judged against a stale threshold can be re-searched (or
not) differently depending on timing, so move selection becomes effectively
non-deterministic and the higher threshold suppressed moves that the full
window would have preferred. The change was rejected and removed; the workers
again compare against the fixed PV score `baseline_score`. Match log:
`tools/engine-match/results/shared-root-threshold-tc10-20261117.txt`. Binary
of the rejected candidate: `build/valox-shared-root-threshold-20261117.exe`.

## Behavior-preserving cleanup: faster repetition scan

`repeated_position` scanned every ancestor on the current path. The repetition
hash includes the side to move and every move (including a null move) flips the
side, so only every second ancestor can match. The new helper
`path_has_repetition` starts at `ply - 2` and steps back by two, with a unit
test for the parity and the ply-clamping edge cases.

The bench confirms that this changes no decision: `--bench 10` still produces
exactly 389,894 nodes and the four best moves `g1f3`, `c1g5`, `e3c5`, `c4c5`.
Ten alternating runs against the dynamic-root build measured a median of
1,756 ms and 222,036 NPS for the control vs 1,756 ms and 222,035 NPS for the
new build - no measurable speedup, because the scan is already tiny next to
NNUE evaluation. The change is kept only as a behavior-preserving cleanup with
one extra test (36 total), not as a strength change. Binary:
`build/valox-repetition-scan-20261117.exe` (identical search behavior to
`build/valox-dynamic-root2-20261117.exe`).

## Rejected change: capture history

Losing captures are ordered by static exchange evaluation alone, so the
candidate added a second history table indexed by from/to square. A capture
that caused a beta cutoff received the same gravity bonus as quiet history, and
its value was added to the ordering score of losing captures (`see + bonus/4`).

- `--bench 10`: 389,810 nodes instead of 389,894 (-0.02%), same four best
  moves. The tiny node delta already showed that losing-capture ordering is
  rarely the bottleneck.
- Tactical suite at depth 12: 6/6.
- 30 paired games, `tc=10+0.1`, Hash 64, Threads 4, seed 20261126: exactly
  8 wins, 8 losses, 14 draws (50.0%), Elo difference 0.0 +/- 92.8.

The change is neutral, so it was reverted. Match log:
`tools/engine-match/results/capture-history-tc10-20261117.txt`.

## Rejected change: internal iterative reduction

The candidate reduced depth by one ply at non-PV nodes with no TT move and at
least depth 4, so the node is searched more cheaply and deposits a best move in
the shared TT for the next visit. This is a standard ordering aid and it saved
a lot of nodes:

- `--bench 10`: 325,575 nodes instead of 389,894 (-16.5%). The start, tactics
  and endgame best moves stayed `g1f3`, `e3c5`, `c4c5`, but the open-position
  move changed from `c1g5` to `c3a4`.
- Tactical suite at depth 12: 6/6.
- 30 paired games, `tc=10+0.1`, Hash 64, Threads 4, seed 20261127: 6 wins,
  6 losses, 18 draws (50.0%), Elo difference 0.0 +/- 79.9.

As with SEE pruning and capture history, the node reduction did not translate
into game strength, so the change was reverted. Match log:
`tools/engine-match/results/iir-tc10-20261117.txt`.

## Accepted change 15: soft and hard time deadlines

The engine used one deadline. Whenever it expired, the search aborted the
current iteration and discarded it, even if the iteration was nearly complete.
The clock budget itself was unchanged, but all work spent in that partial
iteration was lost.

Clock searches now have two deadlines:

- The **soft** deadline is the existing time budget. No new iteration starts
  after it.
- The **hard** deadline is `soft * 4/3`, additionally capped by one eighth of
  the remaining clock plus the increment. An iteration that is already running
  may finish until the hard deadline, then the search stops.

`movetime`, `depth`, `nodes` and `infinite` keep soft equal to hard, so
`movetime` still returns after the requested time. The UCI node and depth
semantics are unchanged.

Correctness checks: 36 tests, Clippy with `-D warnings`, and the wasm32 library
check pass. `--bench 10` is unchanged at exactly 389,894 nodes, because bench
searches use a huge deadline. A `movetime=1500` run respects the limit and
reaches depth 12 instead of 11 in the sampled start position.

Two independent 30-game batches, `tc=10+0.1`, Hash 64, Threads 4, fixed 8-ply
openings, seeds 20261128 and 20261129:

| Batch | DepthFinish score | Elo |
|---|---:|---:|
| 1 | 11 wins, 1 loss, 18 draws (66.7%) | +120.4 |
| 2 | 10 wins, 3 losses, 17 draws (61.7%) | +82.6, LOS 97.4% |
| **Combined** | **21 wins, 4 losses, 35 draws (64.2%)** | **≈+101, LOS >99.9%** |

There were no time forfeits in the 60 games. The gain comes from converting
already-spent time into completed iterations rather than discarding them; the
hard cap keeps the engine from overdrawing the clock. Binaries:
`build/valox-depth-finish-20261117.exe` (accepted) and
`build/valox-dynamic-root2-20261117.exe` (previous control). PGNs:
`tools/engine-match/results/depth-finish-tc10-20261117*.pgn`.

## Rejected change: extra time on best-move changes

The follow-up candidate added a soft-time extension whenever a completed
iteration changed the best move, capped by the hard deadline. The idea was to
spend the extra time exactly when the move choice was unstable.

One 30-game batch at `tc=10+0.1`, Hash 64, Threads 4, seed 20261130 scored
8 wins, 7 losses, 15 draws for the candidate (51.7%, about +11.6 Elo, LOS
around 60%). That is not a demonstrated gain, and the accepted hard deadline
already covers the useful case of finishing an iteration. The change was
reverted; `--bench 10` is again exactly 389,894 nodes. PGN:
`tools/engine-match/results/change-extension-tc10-20261117.pgn`.

## Rejected change: larger hard-deadline overhead

The accepted deadline reserves 33% of the soft budget for the iteration that
is already running. The candidate raised this to 50%, giving a near-finished
iteration more room at the risk of drawing more clock.

One 30-game batch at `tc=10+0.1`, Hash 64, Threads 4, seed 20261131 scored
15.5-14.5 for the candidate (51.7%, about +11.6 Elo, not significant), with
no time forfeits. Because the effect is not clearly positive and the larger
overhead spends more clock, the candidate was reverted to the validated
`soft + soft/3` cap. `--bench 10` is back at exactly 389,894 nodes. PGN:
`tools/engine-match/results/hard-half-tc10-20261117.pgn`.

## Rejected change: history-based LMR

The existing quiet and continuation history was only used for move ordering.
The candidate also fed it into late-move reductions: subtract
`history / 8192` from the base reduction, so a move with good history is
reduced less and bad history is reduced one ply more, clamped to a one-ply
child search.

- `--bench 10`: 402,899 nodes instead of 389,894 (+3.3%).
- Tactical suite at depth 12: 6/6.
- 30 paired games, `tc=10+0.1`, Hash 64, Threads 4, seed 20261143: 7 wins,
  7 losses, 16 draws (50.0%), Elo difference 0.0.

The extra nodes and shallower reductions did not translate into game
strength, so the change and its small refactor were reverted. `--bench 10` is
again exactly 389,894 nodes. PGN:
`tools/engine-match/results/history-lmr-tc10-20261117.pgn`.

## Rejected change: singular extensions

The candidate extended the TT move by one ply when a reduced-depth exclusion
search showed that no other move came close to the TT score. Conditions were
non-PV nodes, no check, depth at least 8, a TT entry no more than three plies
older than the node, and a non-upper bound; the exclusion search skipped TT
cutoffs and did not store its result.

- `--bench 10`: 454,543 nodes instead of 389,894 (+16.6%).
- Tactical suite at depth 12: 6/6.
- Two 30-game batches on the six-opening set, `tc=10+0.1`, Hash 64,
  Threads 4, seeds 20261152 and 20261153, scored exactly 16.5-13.5 for the
  candidate each time. Pooled over 60 games: 33 wins, 27 losses (55.0%),
  about +34.9 Elo, LOS ~78%.
- Expanded retest on `openings-expanded.pgn` (20 openings, seed 20261154,
  80 games): baseline 22 wins, 20 losses, 38 draws; the candidate scored
  39/80 (48.75%), i.e. the baseline was +8.7 +/- 55.6 Elo ahead, LOS 62.1%.

The apparent gain on the six-opening set did not generalize to the broader
opening sample, so the change was rejected and reverted. `--bench 10` is again
exactly 389,894 nodes. The binary `build/valox-singular-20261117.exe` and the
PGNs (`tools/engine-match/results/singular-tc10-20261117*.pgn`,
`singular-expanded-tc10-20261117.pgn`) are kept as evidence that six-opening
results alone are not sufficient for acceptance.

## Rejected change: correction history

The candidate kept a per-search table of static-evaluation corrections keyeded
by a pawn-structure hash. The corrected evaluation was used for pruning and
quiescence stand-pat, while the transposition table continued to store the raw
NNUE value so the correction could not be applied twice. Updates were derived
from the difference between the searched score and the raw static evaluation,
with bounded steps and decay.

- A first version was far too aggressive: `--bench 10` rose to 593,615 nodes
  (+52%), because large corrections disabled pruning. The table was damped to
  a quarter-strength application with smaller updates, giving 415,700 nodes
  (+6.6%).
- Correctness checks stayed green: 36 tests, Clippy `-D warnings`, wasm check,
  tactical suite 6/6.
- 80-game A/B on `openings-expanded.pgn` (seed 20261155, `tc=10+0.1`, Hash 64,
  Threads 4): baseline 24 wins, 13 losses, 43 draws. The candidate scored
  34.5/80 (43.1%), so the baseline was +48.1 +/- 52.0 Elo ahead with LOS 96.5%
  in its favour.
- Per opening, the candidate lost ground in the Nimzo-Indian (0.5/4), Italian,
  Scotch, King's Gambit, and Benoni (1/4 each); only the Najdorf was positive
  (3/4). The correction did not generalise.

The change was rejected and fully reverted; `--bench 10` is again exactly
389,894 nodes. The binary `build/valox-correction-20261117.exe` and the PGN
`tools/engine-match/results/correction-expanded-tc10-20261117.pgn` are kept as
evidence.

## Neutral change (reverted): ProbCut

The candidate tried captures with SEE >= 0 in a reduced verification search
before the main move loop, at non-PV nodes, depth at least 5, with a beta
margin of 180 and checking moves skipped.

- A `depth-4` verification cost too much: `--bench 10` rose to 428,002 nodes
  (+9.8%). Using `depth-5` turned it into a saving: 366,770 nodes (-5.9%).
- 36 tests, Clippy `-D warnings`, wasm check, and the tactical suite (6/6)
  stayed green.
- 80-game A/B on `openings-expanded.pgn` (seed 20261156, `tc=10+0.1`, Hash 64,
  Threads 4): baseline 17 wins, 18 losses, 45 draws. The candidate scored
  40.5/80 (50.6%), i.e. +4.3 +/- 50.6 Elo, LOS ~57%. This is neutral, not an
  improvement.
- Per opening the result was mixed: the candidate lost the Grünfeld, Najdorf,
  Italian, Nimzo-Indian, and Slav starts at 0.5-1/4, but won the French,
  Dragon, and Vienna starts at 3/4. No consistent pattern remained.

The change was reverted; `--bench 10` is again exactly 389,894 nodes. The
binary `build/valox-probcut-20261117.exe` and the PGN
`tools/engine-match/results/probcut-expanded-tc10-20261117.pgn` are kept as
evidence.

## Rejected change: broad delta pruning in quiescence

The candidate removed the `see < -75` gate from quiescence pruning, so every
capture whose static-exchange value plus stand-pat could not reach alpha was
skipped, not only clearly losing captures.

- `--bench 10` fell to 218,233 nodes (-44.0%), i.e. the search reached
  noticeably deeper at the same time budget.
- 36 tests, Clippy, wasm check, and the tactical suite passed at depth 12 and
  16 (6/6 both times).
- 80-game A/B on `openings-expanded.pgn` (seed 20261157, `tc=10+0.1`, Hash 64,
  Threads 4): baseline 33 wins, 10 losses, 37 draws. The candidate scored only
  28.5/80 (35.6%), so the baseline was +102.8 +/- 56.5 Elo ahead with LOS 100%.
- The candidate lost nearly every opening, including 0/4 in the Queen's
  Indian and 0.5/4 in the Najdorf, Dragon, Grünfeld, Catalan, King's Gambit,
  and Benoni. The node saving came from skipping capture lines that the
  engine actually needs.

The change was reverted; `--bench 10` is again exactly 389,894 nodes. The
binary `build/valox-delta-20261118.exe` and the PGN
`tools/engine-match/results/delta-expanded-tc10-20261118.pgn` are kept as
evidence that a large node reduction alone is not evidence of strength.

## Rejected change: countermove heuristic

The candidate added a countermove table indexed by the previous move's
from/to squares. A quiet move that caused a beta cutoff was stored as the
reply to the move that led to the node, and move ordering searched that reply
at 800,000, after the two killers but ahead of history and continuation
history.

- `--bench 10` rose to 430,617 nodes (+10.4%). The effect was path-dependent:
  depths 8 and 9 also used more nodes, but depth 11 fell from 688,016 to
  617,325 and depth 12 from 1,216,373 to 1,107,494.
- 36 tests, Clippy with `-D warnings`, the wasm check, and the tactical suite
  at depths 12 and 16 (6/6 both) passed.
- First 80-game A/B on `openings-expanded.pgn` (seed 20261158, `tc=10+0.1`,
  Hash 64, Threads 4): the candidate scored 42.5/80 (53.1%), about +22 Elo.
- Second 80-game A/B (seed 20261159): the candidate scored 38.5/80 (48.1%),
  -13.0 +/- 57.6 Elo, LOS 32.7%.
- Pooled 160 games: 81/160 (50.6%), +4.3 Elo, LOS 56.3%. With killers and
  continuation history already in place the extra table is neutral here.

The change was reverted; `--bench 10` is again exactly 389,894 nodes. The
binary `build/valox-countermove-20261118.exe` and the PGNs
`tools/engine-match/results/countermove-expanded-tc10-20261118.pgn` and
`...-batch2.pgn` are kept as evidence.

## Rejected change: reverse-futility pruning to depth 6

The candidate kept the existing linear margin `90 + 85*depth` but extended
reverse-futility pruning from depth 3 to depth 6 at non-PV nodes.

- `--bench 10` rose to 438,767 nodes (+12.5%). In the "open" benchmark
  position the depth-10 best move changed from `c1g5` to `a2a4`. Stockfish 19
  at depth 20 scored the two moves +0.04 and +0.08, so the new choice is not a
  blunder, but the node overhead was real.
- 36 tests, Clippy, the wasm check, and the tactical suite at depths 12 and 16
  (6/6 both) passed.
- 80-game A/B on `openings-expanded.pgn` (seed 20261160, `tc=10+0.1`, Hash 64,
  Threads 4): 36.5/80 (45.6%), -30.5 Elo, LOS 78.4% for a regression.
- The margins at depths 4-6 (430-600 raw units, about 120-165 centipawns) are
  too aggressive for a search without correction history.

The change was reverted; `--bench 10` is again exactly 389,894 nodes. The
binary `build/valox-rfp6-20261118.exe` and the PGN
`tools/engine-match/results/rfp6-expanded-tc10-20261118.pgn` are kept.

## Rejected change: time divisor 30

At sudden-death controls the engine divided the remaining clock by 40 before
adding 80 % of the increment. The candidate changed the divisor to 30, so at
`10+0.1` it used about 486 ms per move instead of 402 ms in a direct UCI
measurement.

- Search, tests, Clippy and the wasm check were unaffected; the benchmark is
  depth-limited and does not use the clock.
- 80-game A/B on `openings-expanded.pgn` (seed 20261161, `tc=10+0.1`, Hash 64,
  Threads 4): exactly 40.0/80 (50.0%). No time forfeits occurred. Games
  averaged 145 plies and the longest reached 343 plies.
- At this control the extra 20 % of thinking time did not convert into a
  measurable score gain, so the more aggressive budget was not kept.

The change was reverted; the binary `build/valox-time30-20261118.exe` and the
PGN `tools/engine-match/results/time30-expanded-tc10-20261118.pgn` are kept.

## Accepted change 16 (performance): bitboard attack detection

A fresh ETW CPU profile of the accepted build (`--bench 12` with symbols from a
debug-info build) put `Position::is_attacked` at 17.5% of VALOX's sampled CPU
weight (1,039,707 of 5,952,370 samples), the largest cost besides NNUE feature
updates. The function was reached from `in_check` at every node and from the
Chess960 castling-path legality check, and it iterated over every piece of the
attacking side in mailbox order.

The rewrite replaces that loop with compile-time tables and bit scans:

- `KNIGHT_MASKS`, `KING_MASKS` and `PAWN_ATTACKERS[side]` answer the leaper
  checks with one bitboard AND each.
- Eight directional `RAYS` (four orthogonal, four diagonal) are masked with the
  full occupancy. The nearest blocker is extracted with `trailing_zeros` or
  `leading_zeros`, and only that piece is tested for rook/queen respectively
  bishop/queen.
- Pawn attackers are looked up from the target square, so no per-piece mailbox
  scan remains.

Validation:

- New test `bitboard_attack_detection_matches_reference` compares the new
  implementation with the previous mailbox scan for all 64 squares and both
  sides across about 6,600 positions (start position, Kiwipete, endgame,
  promotion, pin, quiet middlegame and Chess960 parents plus their children).
  All comparisons match.
- `cargo test --release`: **37 tests passed**. Clippy with `-D warnings` and
  the wasm32 library check pass. The tactical suite scores 6/6 at depths 12
  and 16.
- Bench depths 8-12 produce identical node counts and best moves to the
  previous binary; `--bench 10` remains 389,894 nodes with best moves `g1f3`,
  `c1g5`, `e3c5`, `c4c5`.
- Six interleaved `--bench 12` runs per binary, alternating baseline and
  candidate: median 5,913.5 ms versus 5,088.5 ms, **-13.95% wall time** (about
  **+16.2% NPS**) at the identical 1,216,373 nodes.
- A repeat ETW profile dropped `is_attacked` to 5.6% of VALOX samples (300,043
  of 5,405,489) and moved NNUE `apply_feature_avx2` to the top at 30.8%,
  followed by `NnueEvaluator::push` (10.0%), `push_threat_feature` (8.2%) and
  `make_move` (7.0%).
- Fixed-time spot checks: from the start position at 1 s the new build reaches
  depth 11 instead of 10; at 3 s both reach depth 13. Kiwipete and a quiet
  middlegame keep identical depths and scores at 1 s and 3 s.

Because the fixed-depth search is bit-identical, this carries no separate Elo
claim; it is kept as a measured throughput gain. Pinned binary:
`build/valox-fast-attacks-20261118.exe`, SHA-256
`E59A94C78B5F9E4978D2825710D1EE3E862B9343D955C42468784062C6E1AA19`.
The timing baseline is `build/valox-profile-base-20261118.exe`; the profiles
are `tools/engine-match/results/valox-profile-debug-20261118.txt` and
`tools/engine-match/results/valox-profile-fast-20261118.txt`.

## Accepted change 17 (performance): i16 NNUE accumulators

The post-attack profile still put `apply_feature_avx2` first at 30.8% of VALOX
samples (1,666,448 of 5,405,489), and `transform_accumulators_avx2` at 1.7%.
Both streamed 768 i32 values per perspective even though the Reckless reference
implementation of the same network stores its accumulators as `i16`. Halving
the accumulator width halves the memory traffic of every feature update and of
the output transform.

The candidate changes only the storage type and the update/transform kernel:

- Accumulators become `[[i16; FEATURES]; 2]`. The full refresh still sums in a
  temporary i32 buffer, asserts that every final value is representable, and
  narrows once; incremental updates use wrapping `i16` adds/subtracts, matching
  Reckless's reference behavior.
- The AVX2 feature update processes 16 lanes per iteration: piece weights are
  loaded directly with `_mm256_loadu_si256`, threat weights are widened with
  `_mm256_cvtepi8_epi16`, and updates use `_mm256_add_epi16` /
  `_mm256_sub_epi16`. The scalar fallback mirrors this with `wrapping_add`.
- The AVX2 transform clamps in 16-bit lanes, multiplies with
  `_mm256_mullo_epi16`, shifts right by 9, and packs with
  `_mm256_packus_epi16`; the product never exceeds 65025, so the logical shift
  is exact.

Validation:

- `nnue-bench` on the 300-position dataset: evaluation checksum `45968` and
  update checksum `8871920` are identical to the i32 build. The update
  checksum also matches the long-standing reference value recorded for the
  AVX2 feature-update change. Forcing the scalar update path with
  `VALOX_NNUE_SCALAR_UPDATES=1` gives the same two checksums.
- `cargo test --release`: **37 tests passed**. Clippy with `-D warnings`, the
  wasm32 library check and the tactical suite at depths 12 and 16 (6/6 each)
  pass.
- Bench depths 8-12 again produce identical node counts and best moves to the
  i32 build; `--bench 10` remains 389,894 nodes with moves `g1f3`, `c1g5`,
  `e3c5`, `c4c5`.
- Six interleaved `--bench 12` runs per binary, alternating i32 baseline and
  i16 candidate: median 4,914.5 ms versus 4,402 ms, **-10.43% wall time** (about
  **+11.65% NPS**) at the identical 1,216,373 nodes. The i16 minimum was
  4,201 ms against 4,825 ms for the baseline.
- A repeat ETW profile of the new binary reduced `apply_feature_avx2` to 23.1%
  of samples and its absolute sample count by 36.6% (1,055,955 versus
  1,666,448). `transform_accumulators_avx2` fell out of the top 15. Total VALOX
  samples fell from 5,405,489 to 4,568,326 (-15.5%) for the same work.
- Fixed-time spot checks: the quiet middlegame reaches depth 12 (+511) at 1 s
  instead of depth 11 (+502); the start position reaches depth 14 at 3 s
  instead of 13. Where both builds complete the same depth, scores and best
  moves match.

The fixed-depth search remains bit-identical, so this is a throughput gain
without a separate Elo claim. Pinned binary:
`build/valox-i16-acc-20261118.exe`, SHA-256
`1530B5A4BA4E15C670C347EE92B53FD6174C6FC0B674457FE540A5DCAD89D929`.
The comparison baseline is `build/valox-fast-attacks-20261118.exe`; the new
profile is `tools/engine-match/results/valox-profile-i16-20261118.txt`.

## Neutral change (reverted): batched NNUE feature updates

With i16 accumulators in place, `apply_feature_avx2` was still the largest
single sample consumer. A follow-up candidate applied four features per
accumulator pass: the four weight rows were summed in AVX2 registers and the
accumulator cache line was loaded and stored once instead of four times.
Because 16-bit addition is modulo 2^16, this is bit-identical to adding the
features one at a time, including intermediate overflow.

Validation passed:

- `nnue-bench` checksums remained `45968` (evaluation) and `8871920` (update).
- 38 tests passed, including a new test comparing batched lists of length
  0-7 against repeated single-feature updates for piece and threat features.
  Clippy with `-D warnings`, the wasm check, and the tactical suite at depths
  12 and 16 passed.
- Bench depths 8-12 had identical nodes and best moves; `--bench 10` stayed at
  389,894 nodes.
- The ETW profile reduced the combined update-kernel weight from 23.1% to
  19.4% of samples (absolute samples 1,055,955 to 905,796, about -16% after
  normalizing for the shorter run). `apply_feature_avx2` and the batch helper
  split the remaining cost.

End-to-end timing did not reproduce that kernel gain:

- The first 16 alternating `--bench 12` pairs gave a paired median of -5.0%
  time, but a later 8-round ABBA `--bench 12` measurement gave +1.7%, and a
  5-round ABBA `--bench 13` measurement gave -1.1%.
- The effect is below the host's run-to-run resolution and does not justify a
  second update kernel plus a new test, so the change was reverted to the
  single-feature updater.

The accepted state remains the i16 build
`build/valox-i16-acc-20261118.exe`; `--bench 10` is 389,894 nodes. The batch
profile is kept as `tools/engine-match/results/valox-profile-batch4-20261118.txt`.

## Neutral change (reverted): compile-time threat tables

The threat-feature path still consulted three `OnceLock` tables per candidate
(`attack_map`, `attack_rank`, `threat_layout`). A candidate replaced all three
with `const fn` builders evaluated during compilation, so the hot path read
plain statics instead of performing atomic load/check sequences.

Correctness was unchanged: `nnue-bench` checksums stayed at `45968` and
`8871920`, bench depths 8-12 had identical nodes and best moves, all 37 tests
passed, and Clippy with `-D warnings`, the wasm check and the tactical suite at
depths 12 and 16 passed.

End-to-end timing did not improve. Six ABBA `--bench 12` rounds against the
pinned i16 build gave a median of **+0.96% time** (mean +0.84%; five of six
rounds slower). The atomic loads are not measurable against the surrounding
feature extraction, and the larger static tables may cost cache locality, so
the change was reverted. The accepted build remains
`build/valox-i16-acc-20261118.exe`.

## Accepted change 18 (performance): bitboard king lookup in NNUE updates

`NnueEvaluator::reset` and `NnueEvaluator::push` located each king by
scanning all 64 squares and comparing mailbox entries. `push` runs at every
node, so this cost was paid 64 times per perspective: for the black king the
linear scan touched almost the whole board before finding it. `Position`
already maintains a king bitboard, so the lookup is a single
`trailing_zeros()`.

The candidate makes `Position::king_square` crate-visible and uses it in
`reset` and both perspective loops of `push`. For a king move the parent king
square is known exactly from the move origin (`undo.mv.from`), so the two
parent scans in the feature-rebuild paths are also removed. No evaluation,
feature or search logic changes.

Validation:

- `nnue-bench` checksums stay at `45968` (evaluation) and `8871920` (update)
  across 300 positions and 33,116 push/pop pairs.
- `cargo test --release`: 37 tests passed. Clippy with `-D warnings`, the wasm
  check and the tactical suite at depths 12 and 16 (6/6 each) pass.
- Bench depths 8-12 have identical node counts and best moves; `--bench 10`
  remains 389,894 nodes with `g1f3`, `c1g5`, `e3c5`, `c4c5`.
- Six ABBA `--bench 12` rounds against the pinned i16 build all favored the
  candidate: median **-5.22% wall time** (mean -4.89%). Five ABBA `--bench 13`
  rounds were also all negative: median **-3.05%** (mean -3.42%) at the
  identical 1,957,764 nodes.
- The repeat ETW profile has 4,120,415 VALOX samples versus 4,568,326 for the
  same work (-9.8%). `NnueEvaluator::push` fell from 495,962 to 398,966
  absolute samples (-19.6%) and `threat_features_for_square` from 275,971 to
  212,944 (-22.8%).

The fixed-depth search remains bit-identical, so this is a throughput gain
without a separate Elo claim. Pinned binary:
`build/valox-kingsq-20261118.exe`, SHA-256
`E45D367E2F993AFB0E7CC75F350F54CC4411C553474609BC47A5711B0CAF03FF`.
The comparison baseline was `build/valox-i16-acc-20261118.exe`; the profile is
`tools/engine-match/results/valox-profile-kingsq-20261118.txt`.

## Accepted change 19 (performance): nearest-attacker bit test in `is_attacked`

`Position::is_attacked` found the nearest blocker on each ray and then looked
that square up in the mailbox to ask whether it held an enemy rook, bishop or
queen. Both pieces of information are already in the piece bitboards: the
nearest blocker is the lowest (or highest) set bit of the blockers mask, and
the attacker test is a single `&` against the side's rook/queen or bishop/queen
mask. The candidate replaces the mailbox lookup, `Option` handling and piece
match with that bit test.

Validation:

- The `bitboard_attack_detection_matches_reference` parity test (~6,600
  positions) and all 37 tests pass. Clippy with `-D warnings`, the wasm check
  and the tactical suite at depths 12 and 16 pass.
- `nnue-bench` checksums are unchanged (`45968`, `8871920`); bench depths 8-12
  have identical nodes and best moves, `--bench 10` stays at 389,894 nodes.
- Six ABBA `--bench 12` rounds were all negative: median **-3.12% time**
  (mean -3.46%). Eight ABBA `--bench 11` rounds gave median **-1.41%**
  (mean -1.16%, six of eight negative). Ten ABBA `--bench 13` rounds across two
  runs were neutral: medians +0.20% and +0.25%, so the gain fades at longer
  runs on this noisy host.
- The ETW profile reduced `Position::is_attacked` from 273,030 to 231,884
  absolute samples (-15.1%; share 6.6% to 5.3%) for the same 1,216,373-node
  search.

Pinned binary: `build/valox-attackmasks-20261118.exe`, SHA-256
`EE99AF3406AB82C996802C6DD580A924D272BF251BF6B65A7BEBD4FB866EA8D6`.
Comparison baseline was `build/valox-kingsq-20261118.exe`; the profile is
`tools/engine-match/results/valox-profile-attackmasks-20261118.txt`.

## Accepted change 20 (performance): legality test only for critical moves

`legal_moves_into` generated pseudo-legal moves and then called
`make_move`/`in_check`/`unmake_move` for every one of them. That is correct but
does a full board update, castling-rights bookkeeping and clock update just to
learn whether a quiet knight move leaves the king in check.

The candidate keeps the same make/unmake verifier but only runs it when the
move can actually change king safety:

- when the side to move is in check (every move must be an evasion),
- for king moves (including castling),
- for en passant captures (removing two pawns from one rank can expose a rank
  check),
- for pieces that are absolutely pinned to their king.

Everything else is copied straight to the legal list. Pin detection is one
8-ray scan per position: the nearest blocker on a king ray must be an own
piece and the next blocker an enemy rook/queen (orthogonal) or bishop/queen
(diagonal). Unpinned non-king moves cannot create a new attacker, so skipping
make/unmake for them cannot change the legal set.

Validation:

- `cargo test --release`: 37 tests passed, including the full perft suite
  (start position depth 4 = 197,281; Kiwipete depth 4 = 4,085,603) and the
  Chess960 tests.
- Differential perft against python-chess: 100 standard + 40 Chess960
  positions at depth 3 (seed 20261112) and 40 standard + 20 Chess960 positions
  at depth 4 (seed 777) all matched.
- Clippy with `-D warnings`, the wasm check and the tactical suite at depths
  12 and 16 (6/6 each) pass.
- Bench depths 8-12 have identical nodes and best moves; `nnue-bench`
  checksums stay `45968`/`8871920`; `--bench 10` remains 389,894 nodes.
- Six ABBA `--bench 12` rounds were all clearly negative: median **-9.62%
  wall time** (mean -9.59%). Four ABBA `--bench 13` rounds confirmed it:
  median **-10.47%** (mean -10.52%) at 1,957,764 nodes.
- The repeat ETW profile reduced `Position::make_move` from 390,591 to 129,979
  absolute samples (-66.7%) and dropped `unmake_move` out of the top 15; total
  VALOX samples fell from 4,356,372 to 3,745,654 for the same search.

Pinned binary: `build/valox-legalfast-20261118.exe`, SHA-256
`36592131D2711D0E7800F2C0322B42AED58C43C62E4A890803B28182DFEEE524`.
Comparison baseline was `build/valox-attackmasks-20261118.exe`; the profile is
`tools/engine-match/results/valox-profile-legalfast-20261118.txt`.

## Accepted change 21 (performance): reverse attack masks for dirty NNUE threats

After the legality shortcut, the profile showed `NnueEvaluator::push` at
370,807 absolute samples. Most of the remaining cost was the search for
attackers whose threat features must be regenerated: it scanned all 64
squares, looked at each piece, fetched its empty-board attack map and compared
it against every changed square. That is up to 64 x 5 attack-map lookups per
perspective and node.

The candidate builds one conservative attacker mask per changed square in
`Position::attackers_to_mask`:

- knight and king masks for the target square against both sides' piece
  bitboards,
- side-specific pawn-attacker masks,
- the four orthogonal rays for rooks/queens and the four diagonal rays for
  bishops/queens.

The union of these masks and the changed squares themselves is exactly the set
the old loop marked dirty: sliders use empty-board rays on both sides because
the dirty test is intentionally geometric. `push` then walks only the set bits
of that mask, so the 64-square scan and the per-piece map lookups disappear.
Feature lists and the feature diff are unchanged.

Validation:

- `nnue-bench` checksums remain `45968` (evaluation) and `8871920` (update)
  over 300 positions and 33,116 push/pop pairs; the update benchmark dropped
  from about 90 ms to 83 ms.
- `cargo test --release`: 37 tests passed. Clippy with `-D warnings`, the wasm
  check and the tactical suite at depths 12 and 16 (6/6 each) pass.
- Bench depths 8-13 produce identical node counts and best moves; `--bench 10`
  remains 389,894 nodes.
- Six ABBA `--bench 12` rounds were all strongly negative: median **-7.36%
  wall time** (mean -7.80%).
- The repeat ETW profile reduced `NnueEvaluator::push` from 370,807 to 108,984
  absolute samples (-70.6%); total VALOX samples fell from 3,745,654 to
  3,431,964 for the same 1,216,373-node search.

Pinned binary: `build/valox-dirtymask-20261118.exe`, SHA-256
`C5BEC4B322B2E608DC4B7AE6320A8BE8D4E4B5A8480983F67C1A640A8F6F37DE`.
Comparison baseline was `build/valox-legalfast-20261118.exe`; the profile is
`tools/engine-match/results/valox-profile-dirtymask-20261118.txt`.

## Accepted change 22 (performance): reusable move-ordering score buffer

`order_moves` declared `[(i32, Move, i32); 256]` as a local variable on every
call. That initializes 4 KB of stack for each node, although only the first
`moves.len()` entries are ever written, sorted or read.

The candidate moves the array into `Searcher` as a reusable `order_scores`
field. Each call overwrites the used prefix and sorts only that slice; the
recursive search begins after `order_moves` has already copied the ordering
back into the caller's move buffer, so a single buffer per searcher is safe.
`order_moves` now takes `&mut self` for that reason.

Validation:

- `cargo test --release`: 37 tests passed; Clippy with `-D warnings`, the wasm
  check and the tactical suite at depths 12 and 16 (6/6 each) pass.
- Bench depths 8-13 have identical node counts and best moves; `nnue-bench`
  checksums stay `45968`/`8871920`; `--bench 10` remains 389,894 nodes.
- Six ABBA `--bench 12` rounds were all negative: median **-1.68% wall time**
  (mean -1.81%). Four ABBA `--bench 13` rounds also all negative: median
  **-2.05%** (mean -2.33%).
- The repeat ETW profile reduced `Searcher::order_moves` from 156,982 to 86,999
  absolute samples (-44.6%); total VALOX samples fell from 3,431,964 to
  3,137,538 for the same search.

Pinned binary: `build/valox-orderscores-20261118.exe`, SHA-256
`A4F8D21BDFD55369365100744AD6EE01B6D9075DB80636A9D41A8CC6D5EB030D`.
Comparison baseline was `build/valox-dirtymask-20261118.exe`; the profile is
`tools/engine-match/results/valox-profile-orderscores-20261118.txt`.

## Accepted change 23 (build): single codegen unit for release builds

`Cargo.toml` had no `[profile.release]` section, so Rust used its default
`codegen-units = 16`. The engine is dominated by a few hot functions
(`apply_feature_avx2`, `push_threat_feature`, `legal_moves_into`,
`order_moves`) that call small helpers in other modules. With 16 codegen
units those helpers frequently stay behind a real call boundary.

Setting `codegen-units = 1` lets LLVM inline across the whole crate. The
source itself is unchanged; nodes, best moves and NNUE checksums are
identical.

Validation:

- `cargo test --release`: 37 tests passed; Clippy with `-D warnings`, the wasm
  check and the tactical suite at depths 12 and 16 (6/6 each) pass.
- Bench depths 8-13 have identical node counts and best moves; `--bench 10`
  remains 389,894 nodes; `nnue-bench` checksums stay `45968`/`8871920`.
- Five ABBA `--bench 12` rounds were all negative: median **-3.48% wall time**
  (mean -4.04%). Four ABBA `--bench 13` rounds confirmed: median **-2.26%**
  (mean -2.76%).
- For comparison, `lto = "fat"` on top of one codegen unit was consistently
  **slower** (+3.37% median over six bench-12 rounds) and was not kept.

Pinned binary: `build/valox-cgu1-20261118.exe`, SHA-256
`C64429012896A34D4A6A729F5044FDC2214DBAE325262BC0712094612D45D03E`.
Comparison baseline was `build/valox-orderscores-20261118.exe`.

After `cargo fmt` and adding the MIT/Apache-2.0 license files, the same source
semantics build to `build/valox-licensed-20261118.exe`, SHA-256
`1B2301C685A82FADE713926240C7CA19AB2D381DF001EE2A541FACD26B00BD20`. Four
ABBA `--bench 12` rounds against the `cgu1` artifact gave a median **-2.60%**
(mean -3.53%) at identical nodes; this is attributed to code layout, not to a
source optimisation, and is not claimed as a separate gain.

## Accepted change 24 (performance): capture-only quiescence move generation

Quiescence generated *all* legal moves and then discarded every non-capture
except promotions. The candidate adds `Position::legal_captures_into`, backed by
a `pseudo_captures_into` that emits exactly the same capture/promotion
subsequence in the same order as the full generator. Pawn pushes are only
emitted when they promote, rays stop at the first occupied square, and castling
is skipped. The existing legality filter was factored into
`filter_legal_into` and is shared by both entry points.

Quiescence uses the capture generator when not in check; in-check evasions,
the fifty-move and the deep-qsearch paths still use the full generator. A
zero-capture node must still distinguish stalemate from a quiet-only position:
the old code returned 0 for stalemate but `alpha` (including stand pat) when
quiet moves existed. The candidate reproduces that with a single full
generation only in the zero-capture case, so search semantics are unchanged.

Correctness:

- A new unit test compares the ordered capture list against the filtered full
  list for nine FENs (including en passant, promotions, pins, Chess960-style
  castling rights) to depth 2 and finds no difference.
- Bench depths 8-13 have identical node counts and best moves; `--bench 10`
  remains 389,894 nodes with `g1f3`, `c1g5`, `e3c5`, `c4c5`.
- `nnue-bench` checksums stay `45968`/`8871920`; `cargo test --release` passes
  38 tests.
- Clippy with `-D warnings`, the wasm check, Linux-target `cargo check` and
  Clippy, differential perft (100+40 positions at depth 3; 40+20 at depth 4)
  and the tactical suite at depths 12 and 16 (6/6 each) pass.

Speed, ABBA `--bench 12` against `build/valox-licensed-20261118.exe`:

| Series | Median | Mean | Rounds faster |
|---|---:|---:|---:|
| 1 (6 rounds) | -1.56% | -2.08% | 5/6 |
| 2 (6 rounds) | -1.69% | -2.25% | 4/6 |
| 3 (4 rounds, pinned artifact) | -0.72% | -0.48% | 3/4 |

A five-round ABBA `--bench 13` series confirmed the direction: median
**-1.97%** (mean -0.71%, 3/5 faster). Across all 21 rounds 15 were faster. The
effect is small but consistently negative; a conservative reading is roughly
**1% less wall time** at identical nodes and moves.

Pinned binary: `build/valox-capture-gen-20261118.exe`, SHA-256
`21053119F9FCA0309262DB6F5B7C0AA6CB656F65C4AF4C6E354CAA022045557E`.

## Accepted change 25 (performance): one hash pass per node

`negamax` needed two keys at the same node: `position.hash()` for the
transposition table and `position.repetition_hash()` for the repetition scan.
Both called `hash_without_clock`, which mixes twelve piece-mask contributions
plus side, castling rights, rook origins and en passant. The two calls therefore
recomputed the identical twelve-piece-mask pass twice.

`Position::search_hashes` now computes the base once and returns both the full
key (base XOR the halfmove component) and the clock-independent repetition key.
`negamax` uses that pair. The values are identical by construction, so this is a
pure throughput change.

Validation:

- Bench depths 8-13 have identical node counts and best moves; `--bench 10`
  remains 389,894 nodes with `g1f3`, `c1g5`, `e3c5`, `c4c5`.
- `nnue-bench` checksums stay `45968`/`8871920`; `cargo test --release` passes
  38 tests; fmt, Clippy with `-D warnings`, the wasm check, Linux-target
  `cargo check` and Clippy, differential perft and the tactical suite at depths
  12 and 16 (6/6 each) all pass.

Speed, ABBA `--bench 12` against `build/valox-capture-gen-20261118.exe`:

| Series | Median | Mean | Rounds faster |
|---|---:|---:|---:|
| 6 rounds, bench 12 | **-1.45%** | -1.92% | 6/6 |
| 5 rounds, bench 13 | **-0.83%** | +0.47% | 3/5 |

The bench-13 series is noisy, but the change removes work unconditionally and
the bench-12 series was uniform. A conservative reading is about 1% less wall
time at identical nodes and moves.

Pinned binary: `build/valox-hashpair-20261119.exe`, SHA-256
`362AF47FFD692F142A2E723D9B81C25149CB2B68DA611876A5F74A7CD1D0C782`.

## Rejected change 26 (performance): incremental cached position hash

Change 25 left one full hash pass per visited node: twelve piece-mask mixes plus
side, castling rights, rook origins and en passant. The next candidate cached
that clock-independent key in `Position` and maintained it in
`set_square`/`make_move`, so `search_hashes` would only read a field. The hash
values were identical by construction and all bench nodes and moves stayed
identical.

Two implementations were measured against `build/valox-hashpair-20261119.exe`:

- Direct updates XORed each old/new mask contribution into `Position::hash` as
  the squares changed. Eight ABBA `--bench 12` rounds: median **+6.04%** time
  (mean +8.95%), 0/8 faster.
- A second version accumulated all contributions of one move in a local XOR
  delta and applied it once, so the independent mixes could overlap. Eight
  ABBA `--bench 12` rounds: median **+9.12%** time (mean +9.07%), 0/8 faster.

Bench depths 8-13 had identical nodes and best moves, `--bench 10` stayed at
389,894 nodes, `nnue-bench` checksums stayed `45968`/`8871920`, and all 38 tests
passed. The regression is therefore not a behavior change but the cost model:
hash maintenance runs for every make - including pruning probes, quiescence
edges and legality probes that are immediately undone - while the baseline
computes one pass per visited node. Change 25 already removed half of that
pass, so the remaining headroom is only about 1.4% of runtime and cannot pay
for per-make stores and dependency chains.

The source was reverted to the Change 25 state. Candidate evidence is kept as
`build/valox-inc-hash-20261119.exe`, SHA-256
`32CBF85A822882E54B1399A518160D4618552CE0AB2BB63B35E627E3064FEA26`.

## Accepted change 27 (performance): targeted threat-feature deltas

The NNUE update is the largest remaining cost block. In the incremental push,
`threat_features_for_square` regenerated **every** outgoing threat edge of every
piece that geometrically attacked a changed square. That includes all targets of
the attacker, although a threat feature only depends on the attacker's square
and type, the target's square and type, and - for sliders - the first occupied
square on the ray. Most of that work was therefore redundant.

The new update rebuilds the full outgoing edge set only for pieces standing on a
changed square. For an unchanged **leaper** (pawn, knight, king) it emits just
the single edge to the changed target. For an unchanged **slider** it records
the ray through the changed target and regenerates only that ray's first-blocker
edge for the parent and the child. This also handles discovered attacks: when a
blocker leaves a ray, the next occupied square becomes the new threat target
(the failing test case was `b1c3` unblocking `a1` towards `c1`).

The feature sets are subsets of the previous, already duplicate-free sets, and
i16 accumulator addition is commutative, so accumulators are bit-identical by
construction.

Validation:

- Bench depths 8-13 produce identical output apart from time/NPS; `--bench 10`
  remains 389,894 nodes and `--bench 13` 1,957,764 nodes.
- `nnue-bench` checksums are unchanged: `45968` (evaluation) and `8871920`
  (update, 4 update repeats), as is the doubled `17743840` aggregate at 8
  repeats. All 38 tests pass, including the full-refresh comparison tests, the
  special-move tests and the 128-ply unwind test.
- Eight ABBA `--bench 12` rounds against
  `build/valox-hashpair-20261119.exe`: median **-7.58%** time (mean -9.38%),
  **8/8 faster**. Six ABBA `--bench 13` rounds: median **-7.24%** (mean -8.33%),
  **6/6 faster**.

This is throughput only; the search visits the same nodes with the same
evaluations, so no Elo claim is made.

Pinned binary: `build/valox-threat-ray-20261119.exe`, SHA-256
`D1DEBC3333B42E481E94B0A4A2169CFF632F9078FA6596DB8CDAB09A38D2E958`.

## Rejected experiments (18 November, after change 23)

Three follow-up candidates were measured under the `codegen-units = 1` build and
reverted because none of them produced a repeatable speedup:

- **Bitboard nearest-blocker inside `threat_features_for_square`.** Instead of
  walking a slider ray square by square, the candidate added a
  `Position::ray_blocker` helper using the occupancy bitboard and
  `isolate_lowest_one`/`leading_zeros`. Checksums and nodes were unchanged, but
  five ABBA `--bench 12` rounds gave a median **+3.13% time** (mean +4.00%),
  five `--bench 13` rounds **+3.98%** (mean +3.57%). Marking the helper
  `#[inline]` still left it at **+2.20%** (mean +2.34%), and retesting on top of
  `codegen-units = 1` gave **+3.86%** (mean +4.09%). Short rays are cheaper to
  scan than to mask.
- **Nearest-occupied dirty mask for NNUE threat updates.** `attackers_to_mask`
  was tightened from "all sliders whose geometric ray touches the changed
  square" to "the first occupied square in each direction if it is a slider of
  the matching type". Checksums (`45968`/`8871920`), bench nodes and moves 8-13
  stayed identical, which is good evidence the narrower set is exact. Six ABBA
  `--bench 12` rounds were nevertheless neutral: median **+0.69% time** (mean
  +0.63%). The extra bitboard distance computation offset the smaller rebuild
  set, so the conservative mask was kept.
- **`-C target-cpu=native` release build.** Build with native AVX2/BMI2 allowed
  everywhere: checksums unchanged, but five ABBA `--bench 12` rounds gave a
  median of **-0.08% time** (mean +1.91%, high variance). The hot loops are
  already hand-vectorised and runtime-detected, so a less portable binary buys
  nothing and was not adopted.

- **Bitboard `gives_check` before make/unmake.** The candidate computed check
  delivery from the post-move occupancy and piece masks so that LMP/futility
  could discard a move without make/unmake. A parity test compared the helper
  against make + `in_check` for every legal move to depth 2 over ten FENs,
  including en passant, promotions, pins and several Chess960 castles, and
  found no difference. Bench depths 8-13 also stayed identical. Nevertheless
  six ABBA `--bench 12` rounds were uniformly slower: median **+4.43% time**
  (mean +4.62%, 6/6 slower). The mask/occupancy reconstruction costs more than
  the make/unmake it saves, so the helper and its test were removed.

The pinned build remains `build/valox-capture-gen-20261118.exe`.

## Rejected change: wider aspiration windows

Both search paths start with a narrow aspiration window at depth 4 and double
it after a fail (single-thread `40` raw units, threaded `50`; the raw NNUE
scale is roughly 3.6x centipawns). The candidate widened both to `120`, about
28 centipawns, to cut re-searches.

At fixed depth this reduced work substantially: `--bench 10` fell from 389,894
to 365,853 nodes (-6.2%) and `--bench 12` from 1,216,373 to 993,094 (-18.4%,
about -17% wall time). The depth-10 best moves changed (`start`: `g1f3` to
`e2e4`, `open`: `c1g5` to `c3a4`). Stockfish 19 at depth 20 rated `e2e4` +28
against `g1f3` +21, and `c3a4` +13 against `c1g5` +11, so neither change was a
blunder.

Strength did not follow the node reduction. An 80-game A/B on
`openings-expanded.pgn` (seed 20261180, `tc=10+0.1`, Hash 64, Threads 4,
concurrency 2) finished 20-18-42 for the baseline: **41/80 for the baseline
(51.2%)**, Cute Chess +8.7 +/- 52.8 Elo for the baseline, LOS 62.7%. With the
interval spanning zero this is neutral-to-slightly-negative, not an
improvement, so both windows were restored to 40/50.

`--bench 10` is again exactly 389,894 nodes with the original best moves, and
38 tests pass. Candidate evidence is kept as
`build/valox-asp120-20261118.exe`, SHA-256
`62E5274E9E5A2BC14B71998FF5663400B0B066EF94C52ECB89CD73B58EAABC28`, with PGN
and log in `tools/engine-match/results/asp120-expanded-tc10-20261118.*`.

## Rejected change: tightened late-move pruning

The candidate halved the late-move-pruning start index from `4 + depth^2` to
`(8 + depth^2) / 2`. That leaves depth 1 unchanged (4), lowers depth 2 from 8
to 6, and depth 3 from 13 to 8. It was expected to help because the current
margins look loose at depth 3.

Fixed-depth work fell sharply: `--bench 10` went from 389,894 to 315,144 nodes
(-19.2%) and `--bench 12` from 1,216,373 to 933,390 (-23.3%, about -21% wall
time). The depth-10 `open` best move changed from `c1g5` to `c3a4`; Stockfish
19 at depth 20 rated `c3a4` +13 against `c1g5` +11, so the move itself was not
a blunder.

Strength went the other way. The 80-game A/B on `openings-expanded.pgn` (seed
20261181, `tc=10+0.1`, Hash 64, Threads 4, concurrency 2) ended
30-13-37 for the baseline: **48.5/80 for the baseline (60.6%)**, +75.0 Elo for
the baseline. Pruning more late quiets at depths 2-3 loses real tactics that
the deeper re-search would have found.

The source was restored to `4 + depth^2`. Candidate evidence is kept as
`build/valox-lmp-20261118.exe`, SHA-256
`CD793C194C9679CBB3712051249456EC0A3068CCC3DF5445EC050D844DE0814A`, with PGN
and log under `tools/engine-match/results/lmp-expanded-tc10-20261118.*`.

## Rejected change: transposition-table probing in quiescence

Quiescence did not consult the transposition table at all. The candidate probed
entries stored by the main search for bound cutoffs and used the stored move for
ordering. It did not store qsearch results, because the replacement rule would
let an exact depth-0 entry overwrite a deeper entry for the same position.

`--bench 12` fell from 1,216,373 to 1,173,975 nodes (-3.5%, about -2% time), but
`--bench 10` rose from 389,894 to 428,201 (+9.8%) and the depth-10 `start` best
move changed from `g1f3` to `e2e4`. Tactics were 6/6 at depths 12 and 16,
differential perft passed, and all 38 tests passed.

The 80-game A/B on `openings-expanded.pgn` (seed 20261182, `tc=10+0.1`, Hash 64,
Threads 4, concurrency 2) finished 19-17-44 **in favour of the candidate:
41.0/80 (51.2%), +8.7 Elo** with an interval spanning zero. That is not
evidence of a gain under the campaign's acceptance rule, so the probe was
removed and the source is back at 389,894 nodes and identical bench-12 output.

Candidate evidence is kept as `build/valox-qsearchtt-20261118.exe`, SHA-256
`9D3FCB6C334DC6E9E14B5BE950357B5C5D9883B2CA263109DC92044F0D613FBE`, with PGN
and log under `tools/engine-match/results/qsearchtt-expanded-tc10-20261118.*`.

## Rejected change: more aggressive null-move reduction

The candidate raised the null-move reduction from `2 + depth/5` to
`3 + depth/6`, keeping the evaluation-based bonus. At depth 3 that searches the
null move to quiescence instead of depth 0.

Fixed-depth work was mixed: `--bench 10` rose from 389,894 to 411,923 nodes
(+5.6%) with different best moves (`c2c4` in `start`, `c3a4` in `open`), while
`--bench 12` fell from 1,216,373 to 1,011,395 (-16.9%, about -19% time).
Tactics were 6/6 at depths 12 and 16, all 38 tests passed, and a 100-search
8-thread UCI stress run passed.

The match had to be split because of harness server restarts: four chunks of
10 rounds x 2 games (20 each), seeds 20261190-20261193, same
`tc=10+0.1`/Hash 64/Threads 4 settings. The first chunk's PGN also contains 15
games from an earlier aborted attempt with the same seed. Across all chunks 94
games finished (one unfinished game excluded):

- Baseline: 20 wins, 21 losses, 53 draws = **49.5%**
- Candidate: 21 wins, 20 losses, 53 draws = **50.5%**
- Cute Chess-style Elo: **+3.7 for the candidate**, far inside the interval.

An earlier 55-game partial run of the same candidate had pointed the other way
(53.6% for the baseline), which shows the noise level of these matches. No gain
was demonstrated, so the reduction was restored to `2 + depth/5`. Candidate
evidence is kept as `build/valox-nmr-20261118.exe`, SHA-256
`CA7932DAC622D484CCE99AEE2E4BCC4D81B9592200619A3C17825ECF8AE6CD5B`, with the
PGNs under `tools/engine-match/results/nmr-expanded-tc10-20261118.pgn` and
`nmr2-chunk*-tc10-20261119.pgn`.

## Absolute strength anchor, 17 November 2026

The accepted build `build/valox-depth-finish-20261117.exe` was matched
against Stockfish 19 with `UCI_LimitStrength=true` at increasing `UCI_Elo`
values. Both engines used one thread and 256 MB hash, `tc=10+0.1`, the fixed
8-ply opening set, color-swapped games and 30 games per rung. This is a local
anchor, not an official rating.

| Stockfish `UCI_Elo` | Valox W-D-L | Score | Elo difference |
|---:|---:|---:|---:|
| 2200 | 28-2-0 | 96.7% | +585 (saturated) |
| 2400 | 29-1-0 | 98.3% | +708 (saturated) |
| 2600 | 25-3-2 | 88.3% | +351.7 +/- 241.6 |
| 2800 | 19-10-1 | 80.0% | about +241 |
| 3000 | 6-4-20 | 53.3% | +23.2 +/- 72.6 |

At 3000 the result is statistically compatible with parity, so the engine is
roughly at the 3000 level under these conditions. The 2800 rung is clearly
won. The implied values from the lower rungs (about 2950-3040) are not
consistent enough to quote a precise number: Stockfish's limited-strength
scale is not a linear rating scale, and fast time controls amplify the
difference. The campaign's normal four-thread configuration is stronger than
this one-thread anchor. No time forfeits occurred. PGNs:
`tools/engine-match/results/anchor-sf*tc10-20261117.pgn`.

## Expanded opening set

The early A/B matches used a six-opening PGN, so every 30-game batch sampled
only six distinct start positions and the same opening could dominate a result.
`tools/engine-match/openings-expanded.pgn` adds fourteen common openings
(Italian, Scotch, French, Dragon, Nimzo-Indian, Queen's Indian, Grünfeld, Slav,
London, Catalan, Vienna, King's Gambit, Pirc, Benoni) for twenty openings total.
All are eight plies and were validated with python-chess. The original
six-opening file is kept so earlier results stay comparable; new validation
batches use the expanded file where practical.

## Next steps, in priority order

1. **NNUE training at scale.** The existing 10,000-game Stockfish dataset
   produced networks weaker than Reckless v60. A stronger net needs more
   diverse data, better targets, and GPU training; validation must use paired
   engine matches, not only Brier score.
2. **Search feature ablation one at a time.** Capture history, history-based
   LMR, singular extensions, pawn-keyed correction history, ProbCut, broad
   quiescence delta pruning, SEE pruning of losing captures, and
   transposition-table probing in quiescence have been tested and rejected.
   Remaining low-risk ideas are targeted pruning-parameter tuning and SMP
   improvements, each tested in isolation with the expanded opening set. A
   tightened late-move-pruning margin was also rejected (baseline 60.6%).
3. **Time management tuning.** The current budget uses a fixed moves-to-go
   divisor and a small overhead margin. Measuring time losses and average
   remaining clock in real games can recover strength at blitz controls.
4. **Optional TT tuning.** Two-entry clusters and prefetching could recover
   the small single-thread deficit at tiny table sizes, but the shared table
   already wins at realistic Hash settings, so this is lower priority.
5. **Resolve licensing before any public release.** The Valox source license is
   unselected and the Reckless v60 network has no clear redistribution grant.
