# VALOX vs Stockfish postmortem — 2026-11-12

## Result and artifacts

- Browser match score: VALOX 0 wins, 1 draw, 1 loss (0.5/2).
- Game 1: draw by stalemate, 214 plies; VALOX was White.
- Game 2: Stockfish win by checkmate, 137 plies; VALOX was Black.
- Both games began with the same 8-ply opening, with colors reversed.
- The exported PGN passes `audit-pgn.py`: 2 games, all moves legal, no parse
  errors. PGN SHA-256: `438FEE090ABB2061C836AFAD27BB6C64C906EF12A0F6DF1B6423C58D44EAD6BF`.

Preserved files:

- `valox-stockfish-fullstrength-browser-20261112.pgn` — both games.
- `browser-match-20261112-state.json` — exact browser API state and move lists.
- `browser-match-20261112-analysis-1m-clean.json` — position-by-position
  Stockfish analysis, one million nodes per position.
- `browser-match-20261112-analysis-3m.json` and earlier analysis JSON files —
  retained raw experiments, including the run with unstable scores described
  below.

## Analysis method and limits

All 353 positions (including both initial positions) were checked with bundled
Stockfish 19 at full skill, 4 threads, 2 GB hash, one million nodes per position,
clearing the hash before each position. The 3–5 piece tablebase path was not
used in the final pass: the relevant endgame positions had more than five
pieces, so those tables could not adjudicate them. The complete final pass took
98 seconds.

An earlier pass with the tablebase path enabled produced implausibly large,
inconsistent scores in positions with seven pieces. Those raw files remain
preserved, but were excluded from conclusions. The clean no-tablebase pass and
spot checks at five seconds are the evidence used here. Fixed-node evaluations
are still estimates; moves near a threshold need deeper root analysis before
being called blunders.

The browser server retained final moves and results, not per-move UCI `info`
lines. Therefore the actual match's per-move VALOX depth, nodes, NPS, and elapsed
search time cannot be recovered. The configuration is known: VALOX 1 thread,
Stockfish 4 threads, 64 MB hash each, depth cap 60 and a 5-second cap per move,
without a match clock.

For a limited same-position comparison, six positions were re-searched for five
seconds with the current local binaries. VALOX reached depth 10–15, about
0.21–0.57 million nodes, and about 65k–168k NPS on one thread. Stockfish reached
depth 18–76, about 18.6–50.5 million nodes, and about 3.7–10.1 million NPS on
four threads. These are diagnostic spot checks after the match, not its actual
logs and not a strength estimate; thread count and engine implementation differ.

## Move-by-move evaluation changes

The clean one-million-node pass found no VALOX move in game 1 that lost at least
0.50 pawns between the pre-move and post-move evaluations. The largest estimated
changes were 10.Nd2 (+0.34), 59.fxe4 (+0.25), and 67.Qa2+ (+0.22). The game was
ultimately drawn by stalemate, so this game contains no significant VALOX error
under that threshold.

In game 2, the largest estimated VALOX-side losses were:

| Ply | VALOX move | Estimated loss | Assessment |
| ---: | --- | ---: | --- |
| 124 | 62...Kg8 | 3.14 pawns | High-suspicion defensive miss in the fixed-node pass, but five-second Stockfish analysis still selected 62...Kg8 among its top lines and showed mate in 13 for White. It did not change the game result; do not call it the cause of the loss. |
| 120 | 60...Kg8 | 1.95 pawns | Search-sensitive: the five-second root search selected the played move. Not established as a blunder. |
| 50 | 25...Be6 | 1.40 pawns | Search-sensitive: five-second single-PV search selected the played move, while MultiPV/forced-root searches disagreed. Not established as a blunder. |
| 116 | 58...Kg6 | 1.15 pawns | Possible endgame-defense inaccuracy; already substantially worse for Black before this move. Requires more targeted analysis. |
| 70 | 35...b5 | 1.13 pawns | Five-second Stockfish preferred 35...Qg6; forced-root comparison narrowed the estimated difference to roughly 0.34 pawns. Likely a modest inaccuracy, not a tactical blunder. |
| 88 | 44...Qh5 | 0.85 pawns | Five-second Stockfish preferred 44...Kg6; forced-root result suggests a modest loss, but the game was already worse for Black. |
| 48 | 24...Qxb2 | 0.79 pawns | Fixed-node candidate only; needs its own deeper root comparison. |
| 82 | 41...Rc2 | 0.61 pawns | Five-second Stockfish preferred 41...Rc4; forced-root difference was about 0.40 pawns. Modest inaccuracy at most on present evidence. |

The later mating sequence was not the origin of the loss. At the position before
62..., Stockfish's five-second search already found a forced mate in 13 for
White. At the position before 63...Kh8, it preferred 63...Kh7, but still found a
forced mate (in 13 versus in 8 after the played move). This was a worsening of
the mate distance in an already lost position, not the move that first lost the
game.

The evaluation drifted from about +0.3 pawns at the start to roughly +1.2 after
20 plies, +3.3 after 50 plies, and around +5–6 by the queen trade/endgame
transition. The exact score varies by search limit, but the broad trend is
stable: Stockfish built a durable advantage over many moves rather than winning
through one early tactical shot. This two-game sample cannot identify a single
root-cause move or estimate Elo.

## Subsystem diagnosis

- **Move generation:** no evidence of a fault. Every recorded move is legal and
  both games ended through valid chess rules (stalemate and checkmate).
- **Search/time:** the clearest measured limitation is the search budget gap.
  In the match VALOX had one thread against Stockfish's four. Five-second spot
  checks also show far fewer nodes and shallower searches for VALOX, though they
  do not reconstruct the match's own search statistics. A search weakness is
  plausible; the two games do not isolate root search, move ordering, pruning,
  or time allocation.
- **NNUE/evaluation:** not proven as the direct cause. A third-party Reckless v60
  network is embedded, but the game provides too little controlled evidence to
  blame the network. Divergent scores at shallow/deep searches are not enough to
  establish a model defect. Build held-out endgame positions before changing
  network scaling or weights.
- **Endgame play:** likely an area to investigate. The sustained advantage grew
  through the queen trade and pawn ending, and VALOX failed to hold the position.
  Current tablebases cover at most five pieces and therefore could not rescue
  the key seven-piece positions.
- **Time management:** no timeout or illegal response occurred. The server's
  final state does not include clock/search traces, so allocation behavior
  cannot be diagnosed from this match.
- **Move ordering:** the logs do not expose root ordering, and the match does not
  isolate it. No attribution is justified.

## Three highest-priority weaknesses to test

1. **Unequal search resources and low effective search volume.** VALOX had one
   thread while Stockfish had four. First compare VALOX at 1 vs 4 threads on
   paired openings; keep the parallel mode only if it gains playing strength,
   not merely NPS.
2. **Weak defensive accuracy in sustained inferior positions.** Review queen
   trades and pawn endings with a tactical/endgame suite. Measure blunder rate
   and score loss by position class before touching the NNUE.
3. **Search/evaluation instability in close root choices.** Some candidate move
   rankings changed between fixed-node, single-PV, and MultiPV analyses. Audit
   aspiration re-search, root move ordering, interrupted iterations, and
   evaluator consistency; add fixed-position regression cases for identified
   misses.

## Measurable next experiments

1. Save UCI `info` records for every engine move in future browser matches,
   including depth, nodes, NPS, elapsed time, best move, and engine binary hash.
2. Run a paired-color 1-thread vs 4-thread VALOX match on identical openings,
   same hash and fixed time; target at least 200 games or use SPRT. Report score
   and confidence interval. Do not infer a gain from NPS.
3. Create a small, versioned tactical/endgame suite from the flagged FENs and
   public test positions. Record best-move agreement and centipawn loss at fixed
   nodes; add only stable tactical misses as regression tests.
4. For the first code candidate, change one search component only, run all
   correctness tests, then paired-color A/B against the exact prior binary.
   Require no legality/perft regressions and statistically positive match
   evidence before keeping it.

No engine-source change was made in this postmortem: the completed match and
follow-up analysis do not yet isolate an individual algorithmic defect strongly
enough to justify a speculative change. The next implementation should follow
the instrumented, controlled experiments above.
