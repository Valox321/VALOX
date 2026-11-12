#!/usr/bin/env python3
"""Create a small Stockfish self-play dataset for the experimental Bullet trainer."""

from __future__ import annotations

import argparse
import json
import random
import sys
import time
from pathlib import Path

import chess
import chess.engine
import chess.pgn


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_OPENINGS = ROOT / "tools" / "engine-match" / "openings.pgn"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stockfish", required=True, type=Path, help="Stockfish executable")
    parser.add_argument("--openings", type=Path, default=DEFAULT_OPENINGS)
    parser.add_argument("--output-dir", type=Path, default=Path(__file__).parent / "data")
    parser.add_argument("--games", type=int, default=1000)
    parser.add_argument("--nodes", type=int, default=1000, help="Stockfish nodes per move")
    parser.add_argument("--threads", type=int, default=1)
    parser.add_argument("--hash-mb", type=int, default=128)
    parser.add_argument("--random-opening-plies", type=int, default=6)
    parser.add_argument("--max-plies", type=int, default=160)
    parser.add_argument("--sample-every", type=int, default=1)
    parser.add_argument("--seed", type=int, default=20261112)
    return parser.parse_args()


def load_openings(path: Path) -> list[chess.Board]:
    openings: list[chess.Board] = []
    with path.open("r", encoding="utf-8-sig") as source:
        while game := chess.pgn.read_game(source):
            if game.errors:
                continue
            board = game.board()
            for move in game.mainline_moves():
                if move not in board.legal_moves:
                    break
                board.push(move)
            if board.legal_moves.count():
                openings.append(board)
    if not openings:
        raise ValueError(f"No usable opening lines found in {path}")
    return openings


def result_label(result: str) -> float | None:
    return {"1-0": 1.0, "1/2-1/2": 0.5, "0-1": 0.0}.get(result)


def write_data(path: Path, positions: dict[str, tuple[int, float]], rng: random.Random) -> None:
    rows = list(positions.items())
    rng.shuffle(rows)
    with path.open("w", encoding="ascii", newline="\n") as output:
        for fen, (score, result) in rows:
            output.write(f"{fen} | {score} | {result:.1f}\n")


def main() -> int:
    args = parse_args()
    if args.games < 20 or args.nodes < 1 or args.sample_every < 1:
        print("Use at least 20 games and positive node/sample settings.", file=sys.stderr)
        return 2
    if not args.stockfish.is_file():
        print(f"Stockfish executable not found: {args.stockfish}", file=sys.stderr)
        return 2
    if not args.openings.is_file():
        print(f"Opening PGN not found: {args.openings}", file=sys.stderr)
        return 2

    try:
        openings = load_openings(args.openings)
    except (OSError, ValueError) as error:
        print(error, file=sys.stderr)
        return 2

    rng = random.Random(args.seed)
    training: dict[str, tuple[int, float]] = {}
    validation: dict[str, tuple[int, float]] = {}
    outcomes = {"1-0": 0, "1/2-1/2": 0, "0-1": 0, "unfinished": 0}
    start_time = time.monotonic()

    try:
        with chess.engine.SimpleEngine.popen_uci(str(args.stockfish)) as engine:
            options = {}
            if "Threads" in engine.options:
                options["Threads"] = args.threads
            if "Hash" in engine.options:
                options["Hash"] = args.hash_mb
            if options:
                engine.configure(options)

            for game_index in range(args.games):
                board = rng.choice(openings).copy()
                random_plies = rng.randint(0, args.random_opening_plies)
                for _ in range(random_plies):
                    if board.is_game_over(claim_draw=True):
                        break
                    board.push(rng.choice(list(board.legal_moves)))

                game_positions: list[tuple[str, int]] = []
                remaining = max(0, args.max_plies - board.ply())
                for local_ply in range(remaining):
                    if board.is_game_over(claim_draw=True):
                        break

                    play = engine.play(
                        board,
                        chess.engine.Limit(nodes=args.nodes),
                        info=chess.engine.INFO_SCORE,
                    )
                    score = play.info.get("score")
                    if (
                        local_ply % args.sample_every == 0
                        and score is not None
                        and not board.is_check()
                    ):
                        centipawns = score.white().score(mate_score=10000)
                        if centipawns is not None and abs(centipawns) < 10000:
                            game_positions.append((board.fen(en_passant="legal"), centipawns))

                    if play.move is None:
                        break
                    board.push(play.move)

                outcome = board.result(claim_draw=True) if board.is_game_over(claim_draw=True) else "*"
                label = result_label(outcome)
                if label is None:
                    outcomes["unfinished"] += 1
                    continue

                outcomes[outcome] += 1
                destination = validation if game_index % 20 == 0 else training
                for fen, centipawns in game_positions:
                    destination.setdefault(fen, (centipawns, label))

                if (game_index + 1) % 100 == 0 or game_index + 1 == args.games:
                    elapsed = time.monotonic() - start_time
                    print(
                        f"games {game_index + 1}/{args.games}; "
                        f"train {len(training):,}; validation {len(validation):,}; "
                        f"{elapsed:.0f}s",
                        flush=True,
                    )
    except (OSError, chess.engine.EngineError, chess.engine.EngineTerminatedError) as error:
        print(f"Stockfish data generation failed: {error}", file=sys.stderr)
        return 1

    overlap = training.keys() & validation.keys()
    for fen in overlap:
        training.pop(fen, None)

    if not training or not validation:
        print(
            "Not enough completed games to produce non-empty training and validation sets. "
            "Increase --games or adjust --max-plies.",
            file=sys.stderr,
        )
        return 1

    args.output_dir.mkdir(parents=True, exist_ok=True)
    train_path = args.output_dir / "train.data"
    validation_path = args.output_dir / "validation.data"
    write_data(train_path, training, rng)
    write_data(validation_path, validation, rng)

    metadata = {
        "teacher": str(args.stockfish.resolve()),
        "games_requested": args.games,
        "nodes_per_move": args.nodes,
        "random_seed": args.seed,
        "training_positions": len(training),
        "validation_positions": len(validation),
        "outcomes": outcomes,
        "elapsed_seconds": round(time.monotonic() - start_time, 1),
    }
    (args.output_dir / "generation.json").write_text(
        json.dumps(metadata, indent=2) + "\n", encoding="utf-8"
    )
    print(f"Wrote {train_path} and {validation_path}")
    print(f"Finished-game results: {outcomes}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
