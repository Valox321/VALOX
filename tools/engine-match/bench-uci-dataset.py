"""Interleaved fixed-node UCI benchmark over a tab-separated FEN dataset."""

from __future__ import annotations

import argparse
import ctypes
import json
import queue
import re
import subprocess
import threading
import time
from pathlib import Path
from ctypes import wintypes


class ProcessMemoryCounters(ctypes.Structure):
    _fields_ = [
        ("cb", wintypes.DWORD),
        ("PageFaultCount", wintypes.DWORD),
        ("PeakWorkingSetSize", ctypes.c_size_t),
        ("WorkingSetSize", ctypes.c_size_t),
        ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
        ("QuotaPagedPoolUsage", ctypes.c_size_t),
        ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
        ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
        ("PagefileUsage", ctypes.c_size_t),
        ("PeakPagefileUsage", ctypes.c_size_t),
        ("PrivateUsage", ctypes.c_size_t),
    ]


def process_memory(pid: int) -> dict[str, int] | None:
    if not hasattr(ctypes, "WinDLL"):
        return None
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    psapi = ctypes.WinDLL("psapi", use_last_error=True)
    kernel32.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    kernel32.OpenProcess.restype = wintypes.HANDLE
    kernel32.CloseHandle.argtypes = [wintypes.HANDLE]
    kernel32.CloseHandle.restype = wintypes.BOOL
    psapi.GetProcessMemoryInfo.argtypes = [
        wintypes.HANDLE,
        ctypes.POINTER(ProcessMemoryCounters),
        wintypes.DWORD,
    ]
    psapi.GetProcessMemoryInfo.restype = wintypes.BOOL
    handle = kernel32.OpenProcess(0x1000, False, pid)
    if not handle:
        return None
    counters = ProcessMemoryCounters()
    counters.cb = ctypes.sizeof(counters)
    try:
        if not psapi.GetProcessMemoryInfo(
            handle, ctypes.byref(counters), counters.cb
        ):
            return None
        return {
            "working_set_bytes": counters.WorkingSetSize,
            "peak_working_set_bytes": counters.PeakWorkingSetSize,
            "private_bytes": counters.PrivateUsage,
        }
    finally:
        kernel32.CloseHandle(handle)


class Engine:
    def __init__(self, name: str, executable: Path, hash_mb: int) -> None:
        self.name = name
        self.process = subprocess.Popen(
            [str(executable.resolve())],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            bufsize=1,
        )
        assert self.process.stdin and self.process.stdout
        self.output: queue.Queue[str] = queue.Queue()
        threading.Thread(target=self._read, daemon=True).start()
        self.send("uci")
        self.until("uciok")
        self.send(f"setoption name Hash value {hash_mb}")
        self.send("setoption name Threads value 1")
        self.send("isready")
        self.until("readyok")

    def _read(self) -> None:
        assert self.process.stdout
        for line in self.process.stdout:
            self.output.put(line.rstrip())

    def send(self, command: str) -> None:
        assert self.process.stdin
        self.process.stdin.write(command + "\n")
        self.process.stdin.flush()

    def until(self, prefix: str, timeout: float = 45) -> str:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                line = self.output.get(timeout=max(0.01, deadline - time.monotonic()))
            except queue.Empty:
                break
            if line.startswith(prefix):
                return line
        raise TimeoutError(f"{self.name}: timed out waiting for {prefix}")

    def search(self, fen: str, nodes: int) -> dict[str, object]:
        self.send("ucinewgame")
        self.send(f"position fen {fen}")
        started = time.perf_counter()
        self.send(f"go nodes {nodes}")
        lines: list[str] = []
        while True:
            line = self.until("info", timeout=120) if not lines else None
            if line is not None:
                lines.append(line)
            # After the first info, consume all remaining lines until bestmove.
            while True:
                try:
                    line = self.output.get(timeout=0.01)
                except queue.Empty:
                    break
                if line.startswith("bestmove "):
                    bestmove = line.split()[1]
                    elapsed = time.perf_counter() - started
                    info = next(
                        (item for item in reversed(lines) if " nodes " in item), ""
                    )
                    fields = dict(re.findall(r"\b(nodes|time|depth|nps) (\d+)", info))
                    return {
                        "bestmove": bestmove,
                        "nodes": int(fields.get("nodes", 0)),
                        "reported_time_ms": int(fields.get("time", 0)),
                        "reported_nps": int(fields.get("nps", 0)),
                        "depth": int(fields.get("depth", 0)),
                        "wall_ms": elapsed * 1000,
                        "memory": process_memory(self.process.pid),
                    }
                if line.startswith("info "):
                    lines.append(line)

    def close(self) -> None:
        if self.process.poll() is None:
            self.send("quit")
            self.process.wait(timeout=5)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("candidate", type=Path)
    parser.add_argument("dataset", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--nodes", type=int, default=8000)
    parser.add_argument("--hash", type=int, default=64)
    args = parser.parse_args()

    positions = []
    for line in args.dataset.read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("#"):
            continue
        category, fen = line.split("\t", 1)
        positions.append((category, fen))

    baseline = Engine("baseline", args.baseline, args.hash)
    candidate = Engine("candidate", args.candidate, args.hash)
    rows = []
    try:
        for index, (category, fen) in enumerate(positions):
            order = (baseline, candidate) if index % 2 == 0 else (candidate, baseline)
            results = {engine.name: engine.search(fen, args.nodes) for engine in order}
            rows.append(
                {
                    "index": index,
                    "category": category,
                    "fen": fen,
                    "same_bestmove": results["baseline"]["bestmove"]
                    == results["candidate"]["bestmove"],
                    "baseline": results["baseline"],
                    "candidate": results["candidate"],
                }
            )
            if (index + 1) % 16 == 0 or index + 1 == len(positions):
                print(f"{index + 1}/{len(positions)} positions", flush=True)
    finally:
        baseline.close()
        candidate.close()

    summaries = {}
    for name in ("baseline", "candidate"):
        samples = [row[name] for row in rows]
        categories = {}
        for category in sorted({row["category"] for row in rows}):
            subset = [row[name] for row in rows if row["category"] == category]
            categories[category] = {
                "mean_nps": sum(item["reported_nps"] for item in subset)
                / len(subset),
                "mean_depth": sum(item["depth"] for item in subset) / len(subset),
                "total_nodes": sum(item["nodes"] for item in subset),
                "total_wall_ms": sum(item["wall_ms"] for item in subset),
            }
        summaries[name] = {
            "mean_reported_nps": sum(item["reported_nps"] for item in samples)
            / len(samples),
            "mean_depth": sum(item["depth"] for item in samples) / len(samples),
            "total_nodes": sum(item["nodes"] for item in samples),
            "total_wall_ms": sum(item["wall_ms"] for item in samples),
            "categories": categories,
        }
    result = {
        "positions": len(rows),
        "nodes_per_position": args.nodes,
        "hash_mb": args.hash,
        "threads": 1,
        "dataset": str(args.dataset),
        "baseline_executable": str(args.baseline),
        "candidate_executable": str(args.candidate),
        "same_bestmove_count": sum(row["same_bestmove"] for row in rows),
        "summary": summaries,
        "results": rows,
    }
    args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(f"JSON: {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
