"""Compare a fixed-depth start-position search at several UCI thread counts."""

from __future__ import annotations

import argparse
import re
import subprocess
import time
from pathlib import Path


def run(engine: Path, threads: int, depth: int) -> tuple[int, int, str]:
    process = subprocess.Popen(
        [str(engine.resolve())], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT, text=True, bufsize=1,
    )
    assert process.stdin is not None and process.stdout is not None

    def send(command: str) -> None:
        process.stdin.write(command + "\n")
        process.stdin.flush()

    send("uci")
    for line in process.stdout:
        if line.startswith("uciok"):
            break
    send(f"setoption name Threads value {threads}")
    send("setoption name Hash value 256")
    send("isready")
    for line in process.stdout:
        if line.startswith("readyok"):
            break
    send("position startpos")
    send(f"go depth {depth}")
    deadline = time.monotonic() + 90
    final_info = ""
    bestmove = ""
    while time.monotonic() < deadline:
        line = process.stdout.readline()
        if not line:
            break
        line = line.strip()
        if line.startswith(f"info depth {depth} "):
            final_info = line
        elif line.startswith("bestmove "):
            bestmove = line
            break
    send("quit")
    process.wait(timeout=10)
    match = re.search(r"\bnodes (\d+)\b.*\btime (\d+)\b", final_info)
    if not bestmove or not match:
        raise RuntimeError(f"incomplete search at {threads} threads: {final_info!r} {bestmove!r}")
    return int(match.group(1)), int(match.group(2)), bestmove


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("engine", type=Path, nargs="?", default=Path("build/valox.exe"))
    parser.add_argument("--depth", type=int, default=10)
    args = parser.parse_args()
    for threads in (1, 2, 4, 8):
        nodes, elapsed, move = run(args.engine, threads, args.depth)
        print(f"threads={threads} depth={args.depth} nodes={nodes} time_ms={elapsed} nps={nodes * 1000 // max(elapsed, 1)} {move}")


if __name__ == "__main__":
    main()
