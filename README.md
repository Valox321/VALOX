# Valox

Valox is an independent UCI chess engine written in Rust. Its legal move generation, position handling, search, and evaluation are implemented in this project.

Valox's source license has not been selected yet. The public repository does not currently grant permission to reuse or redistribute the source code.

## Build

```powershell
cargo build --release
```

The UCI executable is `target\release\valox.exe`. Start it from a chess GUI that supports UCI, such as Cute Chess.

## Engine features

Valox supports the UCI `Hash`, `Threads`, `SyzygyPath`, `Clear Hash`, and `UCI_Chess960` options. Chess960 positions use Shredder-FEN rook-file castling rights and king-to-rook castling notation when Chess960 mode is enabled. Native builds can use Syzygy WDL tablebases.

The engine uses a mailbox board with maintained bitboards, reversible make/unmake search, transposition and move-history tables, SEE move ordering, and parallel root search. If `networks\v60-7f587dfb.nnue` is present at build time, the build embeds it and uses NNUE; if it is absent, the source-only build uses the classical evaluator. The NNUE inference code is part of Valox; the trained weights are the third-party Reckless v60 model, not a Valox-trained network. The weights are excluded because their redistribution terms are unclear. The 100-game local match results documented below were produced with a build that had these weights.

Run a repeatable search benchmark with:

```powershell
build\valox.exe --bench 10
```

This reports nodes and speed, not Elo.

## Development checks

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

The `tools\engine-match` directory contains UCI verification, stress, perft-differential, tactical, and match tools. Experimental Bullet NNUE training is documented in [`tools/training/README.md`](tools/training/README.md).

## Match evidence and tournament preparation

In a local 100-game color-paired match against Stockfish 19 limited to UCI_Elo 2000 at 5+0, Valox scored 68 wins, 28 draws, and 4 losses. Cute Chess reported +263.4 ±65.9 relative Elo. This single-configuration result is not an official or independent Valox rating. Full methods, caveats, current CI status, and remaining CCC steps are in [`docs/CCC_READINESS.md`](docs/CCC_READINESS.md).
