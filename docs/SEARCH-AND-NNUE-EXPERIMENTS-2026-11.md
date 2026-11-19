# VALOX search and NNUE experiments, November 2026

This note records small local experiments used to decide which changes belong
in the engine. Match results below are exploratory, not official ratings.

## 1. Search diagnostics and UCI node limits

At 5+0, VALOX's clock manager gives a typical move about 80 ms, so low reported
search depths in short games do not by themselves mean the search is stuck.
The UCI parser also previously ignored `go nodes N`, silently falling back to
three seconds. VALOX now applies a shared node budget across search threads and
stops at that budget. A 10,000-node unit test and an 8-thread UCI stress case
cover the command. Search information now reports forced mates using UCI's
`score mate N` format rather than presenting mate scores as centipawns.

The new `tools/engine-match/analyze-disagreement.py` samples the game-separated
validation data and compares VALOX and Stockfish at the same node limit. This
keeps low-depth disagreements repeatable and surfaces tactical positions for
follow-up analysis.

## 2. Move ordering experiment

An MVV-LVA tie-break was added after static exchange evaluation and compared
against the previous ordering in 50 color-swapped games at 5+0, four concurrent
games, and the same opening list. Results were 11 baseline wins, 12 candidate
wins, and 27 draws; each side had one time loss. This is not evidence of a
strength gain, so the experimental ordering was discarded.

## 3. Late-move reduction experiment

A more conservative reduction formula was compared with the previous search
in the same 50-game setup. The previous search scored 13 wins, 15 losses, and
22 draws. This small sample is inconclusive, so the experimental reduction
formula was discarded.

A separate limited check extension found a forced mate in a difficult defensive
position where the previous search stayed at a large centipawn score. In a
50-game color-swapped match, the previous build scored 14 wins, the candidate
13, and there were 23 draws; each side also had one time loss. The result is
inconclusive as a general strength change, but the extension passes the new
forced-mate tactical case and is retained as a targeted search improvement.

## 4. Paired match method

The A/B runs used both colors for each opening, 5+0, four concurrent games,
Hash 256 MB, and local 3-4-5 Syzygy tables. These settings make regressions
visible quickly but still allow occasional time losses; neither run is a formal
Elo test. A longer test at a safer time control is needed before accepting
small search changes.

## 5. NNUE training gate

The existing 10,000-game Stockfish dataset produced 787,278 training positions
and a game-separated 40,036-position holdout. Three Bullet candidates using
WDL mixes 0.50, 0.25, and 0.00 scored 0.0807, 0.0814, and 0.0861 WDL Brier on
the holdout; the embedded Reckless v60 evaluator scored 0.0470. None of those
candidates should replace the current network. A useful next training attempt
needs more diverse, higher-quality targets and validation on actual paired
engine games, not just lower training loss.

## Current decision

Keep the proven NNUE feature-generation speedup, the UCI node-limit and mate
score fixes, and the limited check extension. Keep the existing move ordering
and LMR formula until a larger, cleaner A/B match shows a repeatable gain.

## Reckless source review and history-aware LMR trial

Reckless' current search combines several quiet/noisy/pawn-context histories,
corrects raw NNUE evaluations with learned correction histories, and adjusts
late-move reductions using search context and move history. VALOX already has
basic quiet and continuation histories, SEE ordering, aspiration windows,
null-move pruning, and LMR, but does not use its history values to tune the LMR
reduction and has no evaluation-correction history. These are candidate ideas,
not code to transplant; Reckless' search depends on a larger set of supporting
data structures and tuned parameters.

Two history-based search trials were rejected. A small LMR adjustment batch
was dominated by time losses, so it was inconclusive. A later 20-game fixed
50,000-node paired match scored baseline 5 wins, candidate 1 win, and 14 draws
(60% for baseline); discard the candidate and retain the original LMR formula.
PGN: `tools/engine-match/results/history-lmr-50k-ab.pgn`.

An evaluation-correction history prototype was also tested in two 20-game
batches at 100,000 nodes per move with color-swapped openings. Across both
batches, baseline scored 8 wins, candidate 6 wins, and 26 draws (52.5% for
baseline). The result is small and inconclusive, with no reason to keep its
extra evaluation cost; the prototype is not in the current engine. PGNs:
`tools/engine-match/results/correction-history-100k-ab.pgn` and
`tools/engine-match/results/correction-history-100k-ab-batch2.pgn`.

## Four-way transposition-table trial

A fixed-size, four-way clustered table was compared against the stable
HashMap table over 20 games, paired by color, from the same opening list. Each
engine used a 50,000-node limit per move and Hash 16 MB. The stable build scored
9 wins, 9 draws, and 2 losses; the clustered-table candidate scored 2 wins,
9 draws, and 9 losses. A four-position depth-10 benchmark also measured about
61,000 nodes/s for the stable build and 58,600 nodes/s for the candidate.
Reject this implementation and retain the HashMap table. Match files:
`tools/engine-match/results/tt-cluster-50k-20261112-204513.pgn` and `.txt`.

## ProbCut trial

A reduced-depth tactical ProbCut pass was tested in a 20-game color-paired
50,000-node match. The stable build scored 7 wins, 7 draws, and 6 losses
(52.5%); ProbCut scored 6 wins, 7 draws, and 7 losses. The difference is too
small to establish a regression, but it shows no gain, so keep the simpler
stable search until a larger run supports the feature. The depth-10 benchmark
also had similar throughput while searching about 6.7% more nodes. PGN:
`tools/engine-match/results/probcut-50k-20261112-205443.pgn`.

## NNUE AVX2 feature-update trial

An AVX2 implementation of NNUE accumulator feature-row updates produced the
same depth-10 best moves and node count on the benchmark positions, while
raising measured throughput by about 13%. It was then compared against the
scalar implementation in 20 color-paired games at 10 seconds per move. Scalar
scored 7 wins, 8 draws, and 5 losses (55%); AVX2 scored 5 wins, 8 draws, and 7
losses. The sample is not enough to establish a strength difference, and the
candidate did not show a playing-strength gain. The AVX2 code was removed from
the default engine; keep scalar NNUE updates until a stronger variant wins a
repeatable match. PGN and log:
`tools/engine-match/results/nnue-avx2-10s-20261112-210159.pgn` and `.txt`.

## Timed root-search thread check after Stockfish match

In a five-second-per-move browser match, Valox's four-thread root-split search
lost both games to unrestricted Stockfish. A repeatable timed check on two
positions from those games compared one and four Valox threads. Before a
position already lost by forced mate, one thread reached depth 10 (277,010
nodes) and four threads reached depth 7 (1,014,241 nodes); before another
forced-mate position, the results were depth 3 versus depth 2. This shows the
current root-split search spends its short budget less efficiently at four
threads; it does not establish a match-strength gain. The next browser match
therefore uses one thread for Valox and four for full-strength Stockfish, with
the same five-second move cap and no game clock. Treat the result as a targeted
configuration experiment, not an engine-strength guarantee.

## Full-strength Stockfish browser match and bitboard hash optimization

The two-game browser experiment finished 0-2 for Valox, both by mate. It used
full-strength Stockfish with four threads, one Valox thread, no game clock,
depth capped at 60, and a five-second per-move cap. The legal PGN is
`tools/engine-match/results/valox-stockfish-fullstrength-browser-20261112.pgn`.
Two games are not an Elo estimate. Post-game Stockfish analysis showed both
final mating sequences began from already-lost positions, so the final moves
are not themselves the root cause of either loss.

The search calls `Position::hash()` at every node. The old implementation fed
the full 64-cell mailbox array to `DefaultHasher`; it now mixes the maintained
piece bitboards plus side, castling rook files/rights, en-passant square, and
halfmove clock. A 160-ply make/unmake/FEN round-trip test was added.

Validation on 2026-11-12:

- 17 Rust tests, Clippy with warnings denied, UCI verification, and 100 random
  standard plus 40 Chess960 differential perft positions at depth 3 passed.
- Three runs of `--bench 10` over the same four positions searched 388,049
  nodes each. Median time fell from 6,985 ms to 6,645 ms; median throughput
  rose from 55,554 to 58,397 nodes/s (+5.1%).
- A 20-game paired-color 5+0 Cute Chess match used identical openings, one
  thread and 64 MB Hash per build, local 3-4-5 Syzygy tables, two concurrent
  games, and 8 book plies. Baseline scored 5 wins, 11 draws, and 4 losses
  (10.5/20); candidate scored 4 wins, 11 draws, and 5 losses (9.5/20). Cute
  Chess estimated a 17.4 +/- 105.1 Elo baseline edge (LOS 63.1%); this is
  inconclusive and does not establish a strength regression or gain. All 20
  games parsed with legal moves. PGN/log:
  `tools/engine-match/results/valox-hash-ab-20261112-221253.pgn` and `.txt`.

Keep the hash optimization for its repeatable throughput improvement and
correctness coverage, while recording that playing-strength impact is unknown.
