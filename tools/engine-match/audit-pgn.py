"""Validate a Cute Chess PGN and report result/color/termination counts."""

from __future__ import annotations

import argparse
from collections import Counter
from pathlib import Path

import chess.pgn


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("pgn", type=Path)
    parser.add_argument("--player", default="Valox")
    parser.add_argument("--expected-games", type=int)
    args = parser.parse_args()

    games = 0
    score = 0.0
    colors: dict[str, Counter[str]] = {"White": Counter(), "Black": Counter()}
    results: Counter[str] = Counter()
    terminations: Counter[str] = Counter()
    parse_errors: list[tuple[int, str]] = []

    with args.pgn.open(encoding="utf-8") as source:
        while game := chess.pgn.read_game(source):
            games += 1
            if game.errors:
                parse_errors.extend((games, str(error)) for error in game.errors)
            board = game.board()
            illegal = None
            for move in game.mainline_moves():
                if move not in board.legal_moves:
                    illegal = f"illegal {board.uci(move)} after {board.fen()}"
                    break
                board.push(move)
            if illegal:
                parse_errors.append((games, illegal))

            white, black = game.headers.get("White"), game.headers.get("Black")
            color = "White" if white == args.player else "Black" if black == args.player else None
            if not color:
                parse_errors.append((games, f"player {args.player!r} not found in headers"))
                continue
            result = game.headers.get("Result", "*")
            results[result] += 1
            terminations[game.headers.get("Termination", "unknown")] += 1
            colors[color][result] += 1
            score += 1.0 if result == ("1-0" if color == "White" else "0-1") else 0.5 if result == "1/2-1/2" else 0.0

    print(f"games={games} score={score:.1f}/{games} ({score / games * 100:.1f}%)")
    print(f"results={dict(results)}")
    print(f"white={dict(colors['White'])}")
    print(f"black={dict(colors['Black'])}")
    print(f"terminations={dict(terminations)}")
    print(f"parse_or_legality_errors={len(parse_errors)}")
    for number, error in parse_errors:
        print(f"game {number}: {error}")

    if args.expected_games is not None and games != args.expected_games:
        raise SystemExit(f"expected {args.expected_games} games, got {games}")
    if parse_errors:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
