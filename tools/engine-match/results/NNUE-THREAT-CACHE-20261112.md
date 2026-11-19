# VALOX NNUE threat extraction experiment — 2026-11-12

## Question

Can the NNUE threat-feature path avoid repeated geometric attack calculations
without changing any feature indices, and does that improve throughput or
playing results?

## Baseline and diagnosis

- CPU: AMD Ryzen 7 5700X (8 cores / 16 logical processors), Windows.
- Release benchmark: four fixed positions (start, open, tactics, endgame),
  depth 10, NNUE enabled, one engine process.
- ETW CPU sampling captured `valox.exe --bench 11`. Raw trace:
  `valox-cpu-profile-20261112.etl`; symbol summary:
  `valox-cpu-profile-symbols.txt`.
- In the profiled process, `square_attacks` accounted for about 52% of its
  sampled CPU weight (5,609,433 sample weight). `apply_delta` accounted for
  about 18% and NNUE `evaluate` about 15%. `legal_moves_into` was about 1.3%.
  This identified repeated NNUE threat geometry as the highest-impact local
  speed target; it did not identify a playing-strength defect.

## Isolated implementation

`src/nnue.rs` now computes the oriented geometric attack map once per attacking
piece and reuses it for all of that piece's occupied targets. The existing
feature ordering/rank and model layout are preserved. The original independent
feature-index path remains available to the tests as a reference.

## Correctness checks

- `cargo test`: 17 passed, 0 failed, including brute-force comparison of NNUE
  threat feature indices over legal positions and incremental accumulator
  consistency checks.
- `cargo clippy --all-targets -- -D warnings`: passed.
- Candidate UCI checks: standard search, Chess960 search, and `stop` all
  returned legal `bestmove` responses.
- Differential perft: 100 standard + 40 Chess960 randomized positions at
  depth 3, seed 20261112; all matched python-chess.
- All four depth-10 benchmark positions returned the same best move and node
  count as the previous binary.

## Throughput comparison

Four interleaved baseline/candidate repetitions, depth 10 on the same four
positions. Raw runs are preserved in `nnue-threat-cache-ab-20261112.json`.

| Build | Median time | Median NPS | Nodes searched |
|---|---:|---:|---:|
| Baseline | 6,155.5 ms | 63,041 | 388,049 |
| Cached threat map | 5,468 ms | 70,967 | 388,049 |

Measured throughput change: **+12.6% NPS**. This is a performance result, not
an Elo estimate.

## Paired self-play A/B

- 20 games, 10 paired opening/color rounds, fixed opening suite, 5+0, Hash 256
  MB each, two concurrent games, seed 20261112.
- Baseline scored 4 wins, 8 draws, 8 losses (8/20 points); candidate scored
  8 wins, 8 draws, 4 losses (12/20 points).
- Cute Chess estimate: baseline vs candidate -70.4 +/- 124.2 Elo, LOS 12.4%
  for baseline (87.6% for the candidate). This sample is **not significant**
  and does not establish a strength gain. There was also one time loss, so the
  fast time control and parallel resource contention are caveats.
- Complete PGN: `nnue-threat-cache-ab-20261112-231916.pgn`.
- Complete Cute Chess log: `nnue-threat-cache-ab-20261112-231916.txt`.

## Decision and next steps

Keep this narrowly scoped optimization: it preserves all tested search outputs
at equal depth, passes correctness checks, and produces a repeatable local
throughput gain. Do not claim that it made VALOX stronger; the 20-game result is
too small and noisy. The next engineering bottleneck should be selected from a
fresh profile of a representative match workload. Search strength work still
needs a larger paired test, and multi-thread root search remains a separate
candidate that must be benchmarked independently.

## Follow-up: direct attack-map generation

The post-optimization ETW profile still showed `square_attacks` as 46.75% of
VALOX's sampled CPU weight (`valox-cpu-profile-post-cache-symbols-20261112.txt`).
Inspection showed that it scanned all 64 squares for every query. A second
isolated candidate now enumerates only legal knight/king/pawn destinations or
walks the relevant slider rays. The old 64-square implementation remains as a
test reference.

- New test compares all 6 piece kinds x 64 squares x 2 sides (768 maps);
  all masks match exactly.
- `cargo test`: 18 passed; Clippy with warnings denied passed.
- Differential perft: 100 standard + 40 Chess960 random positions at depth 3;
  UCI standard/Chess960/stop checks passed.
- Four interleaved depth-10 runs, same 388,049 nodes and same four best moves:
  prior build median 71,182 NPS; direct-rays build median 108,016 NPS, **+51.7%**.
  Raw data: `nnue-square-rays-ab-20261112.json`.
- 20-game paired 5+0 self-play: prior build scored 1 win, 13 draws, 6 losses
  (7.5/20); direct-rays build scored 6 wins, 13 draws, 1 loss (12.5/20).
  Cute Chess estimated +88.7 +/-89.2 Elo for direct rays, LOS 97.1%. This is
  promising but remains a small sample with only seven decisive games, so it is
  not a confirmed strength gain.
- PGN: `nnue-square-rays-ab-20261112-232755.pgn`; full log:
  `nnue-square-rays-ab-20261112-232755.txt`.

Retain direct ray enumeration as a measured performance candidate with exact
attack-map equivalence. Do not call its match result a proven Elo improvement.

## Follow-up: specialize NNUE delta application

The profile after direct ray generation showed `apply_delta` at 30.14% of the
sampled process CPU weight, followed by NNUE `evaluate` (25.77%),
`square_attacks` (17.95%), `is_attacked` (8.70%), and legal move generation
(2.52%). The delta updater was branching on the feature type inside each of
the 768 accumulator-element iterations. An isolated candidate moves that
choice outside the inner loop and processes the 16-bit piece rows and 8-bit
threat rows with separate loops.

- 18 Rust tests and Clippy passed; incremental accumulator consistency tests
  passed.
- Four interleaved fixed-position depth-10 runs searched the same 388,049
  nodes and chose the same moves. Median throughput rose from 107,821 to
  109,867 NPS (**+1.9%**).
- A paired 20-game 5+0 match scored 13.5/20 for the delta candidate (10 wins,
  7 draws, 3 losses) versus 6.5/20 for the previous build. Estimate: +127
  +/-134 Elo, LOS 97.4%. This is promising but not statistically decisive;
  retain it as a measured performance candidate, not a proven strength gain.
- Raw benchmark data: `nnue-delta-slices-ab-20261112.json`.
- PGN: `nnue-delta-slices-ab-20261112-233305.pgn`; log:
  `nnue-delta-slices-ab-20261112-233305.txt`.
