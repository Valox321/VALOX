"""Compare VALOX and Stockfish on a fixed-node sample of training positions."""

from __future__ import annotations

import argparse
import random
from pathlib import Path

import chess
import chess.engine


ROOT = Path(__file__).resolve().parents[2]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--positions",
        type=Path,
        default=ROOT / "tools/training/data-10000-n3000/validation.data",
    )
    parser.add_argument("--valox", type=Path, default=ROOT / "build/valox.exe")
    parser.add_argument(
        "--stockfish",
        type=Path,
        default=ROOT
        / "tools/engine-match/stockfish/stockfish/stockfish-windows-x86-64-universal.exe",
    )
    parser.add_argument("--samples", type=int, default=40)
    parser.add_argument("--nodes", type=int, default=20_000)
    parser.add_argument("--seed", type=int, default=7749)
    parser.add_argument("--show", type=int, default=12)
    args = parser.parse_args()

    rows = args.positions.read_text(encoding="ascii").splitlines()
    rng = random.Random(args.seed)
    sample = rng.sample(rows, min(args.samples, len(rows)))
    engines = [
        chess.engine.SimpleEngine.popen_uci(str(path.resolve()))
        for path in (args.valox, args.stockfish)
    ]
    results: list[tuple[int, str, str, str, int, int]] = []
    try:
        for engine in engines:
            options = {}
            if "Threads" in engine.options:
                options["Threads"] = 1
            if "Hash" in engine.options:
                options["Hash"] = 128
            if "UCI_LimitStrength" in engine.options:
                options["UCI_LimitStrength"] = False
            if options:
                engine.configure(options)

        for row in sample:
            fen, teacher, _result = (part.strip() for part in row.split("|"))
            board = chess.Board(fen)
            evaluations = []
            moves = []
            for engine in engines:
                info = engine.analyse(board, chess.engine.Limit(nodes=args.nodes))
                evaluations.append(info["score"].white().score(mate_score=100_000) or 0)
                moves.append(info["pv"][0].uci())
            results.append(
                (
                    abs(evaluations[0] - evaluations[1]),
                    fen,
                    moves[0],
                    moves[1],
                    evaluations[0],
                    int(teacher),
                )
            )
    finally:
        for engine in engines:
            engine.quit()

    print(
        f"Fixed-node disagreement: {len(results)} positions, {args.nodes} nodes per engine, "
        f"seed {args.seed}"
    )
    for diff, fen, valox_move, stockfish_move, valox_score, teacher in sorted(
        results, reverse=True
    )[: args.show]:
        print(
            f"diff={diff:6} VALOX={valox_score:7} SF-best={stockfish_move:5} "
            f"VALOX-best={valox_move:5} teacher={teacher:6} FEN={fen}"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
