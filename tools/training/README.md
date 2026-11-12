# Valox NNUE training (experimental)

This separate trainer uses [Bullet](https://github.com/jw1912/bullet) to train an experimental horizontally mirrored, four king-bucket `Chess768` network (`(3072 -> 512)x2 -> 1`) with shared factoriser weights. It does not change Valox's current Reckless v60 network.

## Requirements on this PC

- Rust (already installed)
- NVIDIA CUDA Toolkit 13.5 (installed on this PC)
- A labeled training dataset, generated with the script below

The current PowerShell environment did not have `CUDA_PATH` set or `nvcc` on `PATH`; the Toolkit is installed at `C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v13.5`. The launch commands below set the needed environment for that shell.

## Data

The included generator plays Stockfish against itself from the project's opening lines and records its centipawn evaluations and game results. It keeps one game out of twenty for validation and shuffles both output files. This creates a starter distillation dataset; its size and teacher search depth are too small to claim strong Elo.

```powershell
python .\generate_data.py `
  --stockfish ..\engine-match\stockfish\stockfish\stockfish-windows-x86-64-universal.exe `
  --games 1500 --nodes 1500 --output-dir .\data-1500
```

Each text `.data` line has:

```text
<FEN> | <white-relative centipawn score> | <white-relative result>
```

The result is `1.0` for a White win, `0.5` for a draw, and `0.0` for a Black win. Do not train on invented/random labels. For serious training, use a large, filtered binpack dataset and keep a separate validation set. Bullet recommends binpack-style training data; its text-backed format is suitable for smaller experiments.

The trainer's `DirectSequentialDataLoader` reads Bullet's compact binary `ChessBoard` format. Run `prepare_data.ps1 -DataDirectory .\data-1500` to use Bullet's own `bullet-utils` converter and shuffler on both text files.

## Train

From this directory, after generating data:

```powershell
$env:Path += ";$env:USERPROFILE\.cargo\bin"
$bulletUtils = Get-Command bullet-utils.exe -ErrorAction SilentlyContinue
if (-not $bulletUtils) {
  git clone --filter=blob:none --no-checkout https://github.com/jw1912/bullet .bullet-source
  git -C .bullet-source checkout --detach 5d70177680437b7c0ddf958bd4722d95afa78b0c
  cargo build --release --package bullet-utils --manifest-path .bullet-source\Cargo.toml
  Copy-Item .bullet-source\target\release\bullet-utils.exe .\bullet-utils.exe
}
```

Then convert and train:

```powershell
$bulletUtilsPath = (Get-Command bullet-utils.exe -ErrorAction SilentlyContinue).Source
if (-not $bulletUtilsPath) { $bulletUtilsPath = (Resolve-Path .\bullet-utils.exe).Path }
.\prepare_data.ps1 -BulletUtilsPath $bulletUtilsPath -DataDirectory .\data-1500
$cudaPathValox = "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v13.5"
$env:CUDA_PATH = $cudaPathValox
$env:Path = "$cudaPathValox\bin;$cudaPathValox\bin\x64;$env:Path"
cargo run --release --bin valox-trainer --features cuda -- .\data-1500\train.data .\data-1500\validation.data .\checkpoints-1500 0.5
cargo run --release --bin validate -- .\checkpoints-1500\valox-768x4hm-512-wdl50-10\quantised.bin .\data-1500\validation.data
```

The trainer uses the dataset size to set one pass through the training records per superbatch. The last argument controls how strongly final WDL results are mixed into training targets; try values from 0.0 to 1.0. This pinned Bullet revision does not evaluate its configured validation set, so the separate `validate` binary compares a candidate's result prediction and calibration with Stockfish holdout labels and Valox's current evaluator. In the current 1500-game experiment, the best candidate remains weaker on the holdout, so it is intentionally not connected to normal Valox builds. Keep checkpoints separate from `networks\v60-7f587dfb.nnue`.

## Current limitations

- This starter architecture is intentionally different from Reckless v60, including its threat features and buckets.
- The generator produces a small self-play dataset. Larger, deeper, diverse binpack data is needed for serious training.
- A brief run or lower training loss does not establish playing strength or Elo.
