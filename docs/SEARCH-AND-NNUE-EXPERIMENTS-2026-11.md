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
