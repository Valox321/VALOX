# Valox

Valox is an independent UCI chess engine written in Rust. Its legal move generation, position handling, search, and evaluation are implemented in this project. Valox's own source is dual-licensed under **MIT OR Apache-2.0**; see `LICENSE-MIT` and `LICENSE-APACHE`.

## Build

```powershell
cargo build --release
```

The UCI executable is `target\release\valox.exe`. Start it from a chess GUI that supports UCI, such as Cute Chess.

The default build embeds `networks\v60-7f587dfb.nnue` when that local file exists. To inject a network from outside the repository, set `VALOX_NNUE_PATH`:

```powershell
$env:VALOX_NNUE_PATH = 'D:\models\candidate.nnue'
cargo build --release
```

Set `VALOX_NO_NNUE=1` to build without embedded weights. Such a binary starts with the classical evaluator and can load a compatible network at runtime.

### Evaluation file at runtime

A weight-free binary can use the Reckless v60 model without embedding it:

```powershell
target\release\valox.exe --evalfile networks\v60-7f587dfb.nnue --bench 10
```

In a UCI session, set `EvalFile`:

```text
setoption name EvalFile value C:\models\v60-7f587dfb.nnue
```

The `VALOX_EVALFILE` environment variable does the same for scripts. The file must be exactly 63,266,880 bytes (the v60 layout). The engine validates the size and reports `info string NNUE loaded ...` on success. A valid earlier model stays active if a later load fails.

The upstream v60 file is a GitHub release asset of the RecklessNetworks repository:

```text
https://github.com/codedeliveryservice/RecklessNetworks/releases/download/networks/v60-7f587dfb.nnue
SHA-256 7F587DFB1FE5D74D53909328AFA6FD51650C8C7F45907602DB7FBB1E52948C61
```

The weights remain third-party data; download and use them under the upstream terms. Valox release archives do not contain the file.

For a static Linux x86_64 binary and release archives, use the packaging tool:

```powershell
python tools\build-release.py --build-only --network networks\v60-7f587dfb.nnue
python tools\build-release.py --target linux-musl
```

The second command deliberately builds without NNUE weights and produces an archive that can load them with `EvalFile`. Packaging a binary that contains the Reckless v60 network requires `--allow-network-redistribution`, which must only be used after written permission. `tools\make-synthetic-net.py` creates a deterministic same-layout network for CI tests of the NNUE code path without using the third-party weights.

`tools\linux-qemu-smoke.py` boots a static build in a minimal Linux initramfs under QEMU TCG. It needs a Linux kernel image, a static BusyBox binary, and QEMU; the script's help lists the arguments.

## Engine features

Valox supports the UCI `Hash`, `Threads`, `SyzygyPath`, `EvalFile`, `Clear Hash`, and `UCI_Chess960` options. Chess960 positions use Shredder-FEN rook-file castling rights and king-to-rook castling notation when Chess960 mode is enabled. Native builds can use Syzygy WDL tablebases.

The engine uses a mailbox board with maintained bitboards, reversible make/unmake search, transposition and move-history tables, SEE move ordering, and parallel root search. If `networks\v60-7f587dfb.nnue` is present at build time, the build embeds it and uses NNUE. A weight-free build starts with the classical evaluator and switches to NNUE after `EvalFile` loads a compatible network. The NNUE inference code is part of Valox; the trained weights are the third-party Reckless v60 model, not a Valox-trained network. The weights are excluded because their redistribution terms are unclear. The 100-game local match results documented below were produced with a build that had these weights.

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

## License

Valox source is dual-licensed under **MIT OR Apache-2.0**. See `LICENSE-MIT` and `LICENSE-APACHE`.

The Reckless v60 network file (`networks/v60-7f587dfb.nnue`) is not covered by this license. It is excluded from public source archives and release packages, and must not be redistributed until its terms are clarified. Weight-free builds use the classical evaluator and can load an external file through `EvalFile`.
