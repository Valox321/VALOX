# Valox Bullet training experiment, 12 Nov 2026

## Setup

- Teacher: bundled Stockfish, 1,500 nodes per move.
- Data: 1,500 self-play games; 327 unfinished games were discarded.
- Bullet-validated records: 100,453 training positions and 5,598 positions held out by game.
- Training ran on the NVIDIA GeForce RTX 3060.
- The best saved checkpoint from each model was selected using the held-out positions.

## Results

| Evaluator | Decisive result-side accuracy | WDL Brier score |
|---|---:|---:|
| Current Valox Reckless v60 NNUE | 87.2% | 0.0821 |
| Bullet 768x256, WDL mix 0.25, superbatch 5 | 83.3% | 0.1006 |
| Bullet 768x1024, WDL mix 0.25, superbatch 5 | 83.5% | 0.1049 |
| Bullet four king buckets, 512 hidden, WDL mix 0.50, superbatch 10 | 80.7% | 0.1132 |
| Earlier Bullet 768x128, tested on this holdout | 82.9% | 0.1059 |

Lower Brier scores are better. These are evaluator comparisons on correlated self-play positions, not Elo estimates or engine-match results. None of the tested Bullet checkpoints improved on the current Valox evaluator, so none was connected to the default engine.

## 10,000-game data run

- Teacher: bundled Stockfish, 3,000 nodes per move; seed `20261113`; maximum 180 plies.
- Requested games: 10,000; finished: 8,895 (3,396 White wins, 2,078 draws, 3,421 Black wins); 1,105 were unfinished and omitted.
- Bullet validated 787,278 training positions and 40,036 validation positions, split by game. No invalid positions were found.
- Hardware: NVIDIA GeForce RTX 3060; four-bucket, horizontally mirrored Chess768 with 512 hidden units.
- The best saved checkpoints appeared at superbatch 5. On the held-out positions, WDL mixes 0.50, 0.25, and 0.00 produced candidate WDL Brier scores of 0.0807, 0.0814, and 0.0861 respectively. The current Reckless v60 evaluator scored 0.0470 and 93.8% decisive result-side accuracy; the candidates reached 88.0%, 89.1%, and 89.4% respectively.
- These Bullet candidates are weaker on this holdout and remain isolated. None is integrated into Valox. Holdout metrics are not Elo or playing-strength evidence.

## Reproduce

From `tools/training`:

```powershell
python .\generate_data.py --stockfish ..\engine-match\stockfish\stockfish\stockfish-windows-x86-64-universal.exe --games 1500 --nodes 1500 --output-dir .\data-1500
.\prepare_data.ps1 -BulletUtilsPath C:\Users\Valox\AppData\Local\Temp\valox-bullet-inspect\target\release\bullet-utils.exe -DataDirectory .\data-1500
.\train.ps1 -CheckpointFolder .\checkpoints-1500-wdl25 -WdlMix 0.25
cargo build --release --bin validate
.\target\release\validate.exe .\checkpoints-1500-wdl25\valox-768x256-wdl25-5\quantised.bin .\data-1500\validation.data
```

The trainer and validation tool are experimental. The next useful step for a stronger NNUE is substantially more diverse, higher-quality training data and a feature representation with king-relative context, followed by game-based strength testing.
