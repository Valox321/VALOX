"""Small deterministic tactical smoke test for Valox and a Stockfish reference.

Requires python-chess: py -m pip install python-chess
Run from the repository root: py tools/tactical-suite.py --depth 10
"""

import argparse
from pathlib import Path

import chess
import chess.engine


ROOT = Path(__file__).resolve().parents[1]
POSITIONS = [
    (
        "mate in one: queen checkmate",
        "7k/8/6K1/6Q1/8/8/8/8 w - - 0 1",
        {"g5d8"},
    ),
    (
        "mate in one: protected queen mate",
        "k7/8/1QK5/8/8/8/8/8 w - - 0 1",
        {"b6b7"},
    ),
    (
        "mate in one: king-box mate",
        "k7/8/2K5/1Q6/8/8/8/8 w - - 0 1",
        {"b5b7"},
    ),
    (
        "take an undefended queen",
        "q3k3/8/8/8/8/8/8/R3K3 w - - 0 1",
        {"a1a8"},
    ),
    (
        "capture a hanging queen",
        "4k3/8/8/4q3/8/8/4R3/4K3 w - - 0 1",
        {"e2e5"},
    ),
    (
        "find a forced mate while defending",
        "8/5R2/3p3k/8/P4R1p/8/6KP/8 b - - 1 45",
        {"mate"},
    ),
]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--depth", type=int, default=12)
    parser.add_argument("--valox", type=Path, default=ROOT / "target/release/valox.exe")
    parser.add_argument(
        "--stockfish",
        type=Path,
        default=ROOT
        / "tools/engine-match/stockfish/stockfish/stockfish-windows-x86-64-universal.exe",
    )
    args = parser.parse_args()

    results = {"Valox": [], "Stockfish": []}
    for name, path in (("Valox", args.valox), ("Stockfish", args.stockfish)):
        engine = chess.engine.SimpleEngine.popen_uci(str(path))
        try:
            for label, fen, expected in POSITIONS:
                board = chess.Board(fen)
                info = engine.analyse(board, chess.engine.Limit(depth=args.depth))
                move = info["pv"][0]
                uci = move.uci()
                score = info["score"].white()
                found_expected_mate = "mate" in expected and score.is_mate()
                passed = move in board.legal_moves and (uci in expected or found_expected_mate)
                results[name].append(passed)
                print(
                    f"{name:9} {'PASS' if passed else 'MISS':4} "
                    f"{label:38} move={uci:5} score={score} depth={info.get('depth')}"
                )
        finally:
            engine.quit()

    valox_passes = sum(results["Valox"])
    reference_passes = sum(results["Stockfish"])
    print(
        f"\nSummary: Valox {valox_passes}/{len(POSITIONS)}; "
        f"Stockfish reference {reference_passes}/{len(POSITIONS)}"
    )
    return 0 if all(results["Valox"]) else 1


if __name__ == "__main__":
    raise SystemExit(main())
