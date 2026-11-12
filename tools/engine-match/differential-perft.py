"""Compare Valox move generation against python-chess on randomized legal positions.

Requires python-chess (`python -m pip install python-chess`). Run from the project root:
`python tools/engine-match/differential-perft.py build/valox.exe --positions 100 --chess960 40`
"""

from __future__ import annotations

import argparse
import random
import subprocess
from pathlib import Path

import chess


def reference_perft(board: chess.Board, depth: int) -> int:
    if depth == 0:
        return 1
    nodes = 0
    for move in list(board.legal_moves):
        board.push(move)
        nodes += reference_perft(board, depth - 1)
        board.pop()
    return nodes


def random_position(rng: random.Random, chess960: bool) -> tuple[chess.Board, str]:
    if chess960:
        board = chess.Board.from_chess960_pos(rng.randrange(960))
        board.chess960 = True
    else:
        board = chess.Board()

    plies = rng.randrange(0, 101)
    for _ in range(plies):
        legal_moves = list(board.legal_moves)
        if not legal_moves:
            break
        board.push(rng.choice(legal_moves))

    fen = board.fen(shredder=chess960)
    return board, fen


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("engine", type=Path)
    parser.add_argument("--positions", type=int, default=100)
    parser.add_argument("--chess960", type=int, default=40)
    parser.add_argument("--depth", type=int, default=3)
    parser.add_argument("--seed", type=int, default=20261112)
    args = parser.parse_args()
    if args.positions < 0 or args.chess960 < 0 or args.depth < 1:
        parser.error("positions and chess960 must be nonnegative; depth must be positive")

    rng = random.Random(args.seed)
    cases = [(False, random_position(rng, False)) for _ in range(args.positions)]
    cases.extend((True, random_position(rng, True)) for _ in range(args.chess960))

    process = subprocess.Popen(
        [str(args.engine.resolve())],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1,
    )
    assert process.stdin is not None and process.stdout is not None

    try:
        process.stdin.write("uci\n")
        process.stdin.flush()
        for line in process.stdout:
            if line.startswith("uciok"):
                break
        else:
            raise RuntimeError("engine exited before uciok")

        for index, (is_chess960, (board, fen)) in enumerate(cases, start=1):
            expected = reference_perft(board, args.depth)
            process.stdin.write(f"position fen {fen}\nperft {args.depth}\n")
            process.stdin.flush()
            for line in process.stdout:
                if line.startswith("nodes "):
                    actual = int(line.split()[1])
                    break
            else:
                raise RuntimeError(f"engine exited during case {index}")

            if actual != expected:
                mode = "Chess960" if is_chess960 else "standard"
                print(
                    f"MISMATCH case={index} mode={mode} depth={args.depth} "
                    f"Valox={actual} python-chess={expected}\nFEN: {fen}"
                )
                return 1

        process.stdin.write("quit\n")
        process.stdin.flush()
        process.wait(timeout=10)
        if process.returncode != 0:
            raise RuntimeError(f"engine exited with status {process.returncode}")
        print(
            f"Differential perft passed: {args.positions} standard + {args.chess960} "
            f"Chess960 positions at depth {args.depth}; seed={args.seed}"
        )
        return 0
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()


if __name__ == "__main__":
    raise SystemExit(main())
