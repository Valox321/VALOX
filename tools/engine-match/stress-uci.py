"""Repeated UCI lifecycle smoke test for a Valox executable."""

from __future__ import annotations

import argparse
import queue
import subprocess
import threading
import time
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("engine", type=Path, nargs="?", default=Path("build/valox.exe"))
    parser.add_argument("--games", type=int, default=50)
    args = parser.parse_args()

    lines: queue.Queue[str | None] = queue.Queue()
    process = subprocess.Popen(
        [str(args.engine.resolve())],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1,
    )

    def read_output() -> None:
        assert process.stdout is not None
        for line in process.stdout:
            lines.put(line.rstrip())
        lines.put(None)

    threading.Thread(target=read_output, daemon=True).start()

    def send(command: str) -> None:
        assert process.stdin is not None
        process.stdin.write(command + "\n")
        process.stdin.flush()

    def expect(prefix: str, timeout: float = 20.0) -> str:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                line = lines.get(timeout=max(0.01, deadline - time.monotonic()))
            except queue.Empty as exc:
                raise RuntimeError(f"timeout waiting for {prefix}") from exc
            if line is None:
                raise RuntimeError(f"engine exited waiting for {prefix}")
            if line.startswith(prefix):
                return line
        raise RuntimeError(f"timeout waiting for {prefix}")

    try:
        send("uci")
        expect("uciok")
        send("setoption name Threads value 8")
        send("setoption name Hash value 128")
        send("isready")
        expect("readyok")
        for index in range(args.games):
            send("ucinewgame")
            if index % 2:
                send("position startpos moves e2e4 e7e5 g1f3 b8c6")
            else:
                send("position startpos")
            if index % 5 == 3:
                send("go wtime 5000 btime 5000 winc 100 binc 100")
                move = expect("bestmove", timeout=15.0)
            elif index % 5 == 4:
                send("go infinite")
                expect("info")
                send("stop")
                move = expect("bestmove", timeout=15.0)
            elif index % 6 == 5:
                started = time.monotonic()
                send("go nodes 10000")
                move = expect("bestmove", timeout=15.0)
                if time.monotonic() - started > 5.0:
                    raise RuntimeError("go nodes did not stop promptly")
            else:
                send("go depth 7")
                move = expect("bestmove", timeout=30.0)
            if len(move.split()) < 2 or move.split()[1] in {"0000", "(none)"}:
                raise RuntimeError(f"invalid bestmove in iteration {index + 1}: {move}")
            if index % 10 == 9:
                send("isready")
                expect("readyok")
        send("quit")
        process.wait(timeout=10)
        if process.returncode != 0:
            raise RuntimeError(f"engine exited with status {process.returncode}")
        print(f"UCI stress passed: {args.games} searches, 8 threads, node-limited, depth, clock-managed and interrupted searches, repeated ucinewgame/position/isready/quit")
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()


if __name__ == "__main__":
    main()
