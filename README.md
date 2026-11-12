# Valox

Valox is an independent UCI chess engine written in Rust. Its legal move generation, position handling, search, and evaluation are implemented in this project.

Valox's source license has not been selected yet. The public repository does not currently grant permission to reuse or redistribute the source code.

## Play against Valox in your browser

1. In PowerShell, move into this folder and run `.\run-browser.ps1`.
2. The script builds the WebAssembly engine, starts a loopback-only local web server, and opens `http://127.0.0.1:8765` in your browser.
3. Click **Neue Partie**, choose a side and search depth, then click a piece and its destination square.
4. Run `.\stop-browser.ps1` when you are done.

The board and engine run locally on your computer. No chess service or account is needed.

## Build the UCI engine

```powershell
cargo build --release
```

The desktop engine is `target\release\valox.exe`. For the browser build, install the Rust target once with `rustup target add wasm32-unknown-unknown`; the launch script rebuilds the `.wasm` file when you start the local site.

## Engine options and diagnostics

Valox supports the UCI `Hash`, `Threads`, and `UCI_Chess960` options. Chess960 positions can be loaded from Shredder-FEN with rook-file castling rights (`A`–`H` / `a`–`h`); with `UCI_Chess960` enabled, castling moves use king-to-rook UCI notation. Set `Threads` to use parallel root search. Desktop builds support Syzygy WDL tablebases. The 3–5 piece WDL files are installed in `syzygy\3-4-5-wdl`; the match launcher passes this folder to both Valox and Stockfish. To configure a UCI GUI manually, send `setoption name SyzygyPath value <folder>` and then `isready`. On Windows, use a path on the current drive because the underlying Syzygy path format treats colons as separators. The installed set contains WDL files only, not DTZ files.

Run a repeatable search benchmark with `build\valox.exe --bench 10`. The final line reports total nodes, elapsed time and nodes per second; compare runs on the same computer and depth.

The engine uses a mailbox board with maintained bitboards, reversible make/unmake search, transposition and move-history tables, SEE move ordering, and parallel root search. If `networks\v60-7f587dfb.nnue` is present at build time, native and WebAssembly builds embed it and use NNUE; if it is absent, source-only builds use the classical evaluator. The NNUE inference code is part of Valox; the trained weights are the third-party Reckless v60 model and are not a Valox-trained network. The weights are excluded from this public source repository because their redistribution terms are unclear. The local browser build includes the model only when you have separately obtained and placed that file in `networks`. The reported local matches used a build with those weights; they do not describe the no-weights fallback build. Benchmarks measure speed and search behavior, not Elo. In the latest local 100-game paired match on 2026-11-12 against Stockfish 19 limited to UCI_Elo 2000 at 5+0, Valox scored 68 wins, 28 draws, and 4 losses. CuteChess reported +263.4 ±65.9 relative Elo; this single-configuration result is not an official or independent Valox rating. Valox scored 43/50 with White and 39/50 with Black. Full results and caveats are in [`docs/CCC_READINESS.md`](docs/CCC_READINESS.md).

## Experimental Bullet training

An isolated Bullet-based NNUE trainer is in [`tools/training`](tools/training/README.md). It produces an experimental `Chess768` network and does not replace the current Reckless v60 evaluator. Training requires the CUDA Toolkit and labeled chess data. A small 1,500-game dataset exists in the local workspace as an experiment, but it is not included in the engine source snapshot or sufficient for a competitive network. A new model needs matching Valox inference code and engine-match validation before it can become the default evaluator.

## Quality checks and tournament preparation

Run `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `cargo build --release` before a release. `tools\engine-match\verify-uci.ps1` checks UCI handshake, standard and Chess960 searches, eight-thread configuration, and stopping a search. For a broader move-generation cross-check, install `python-chess` and run `python tools\engine-match\differential-perft.py build\valox.exe --positions 100 --chess960 40 --depth 3`. The current CCC preparation checklist and remaining gates are in [`docs/CCC_READINESS.md`](docs/CCC_READINESS.md).
