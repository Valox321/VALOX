"""Interleaved direct NNUE evaluation and incremental update benchmarks."""

from __future__ import annotations

import argparse
import json
import re
import statistics
import subprocess
from pathlib import Path


def run(executable: Path, dataset: Path, eval_repeats: int, update_repeats: int):
    completed = subprocess.run(
        [
            str(executable.resolve()),
            str(dataset.resolve()),
            str(eval_repeats),
            str(update_repeats),
        ],
        check=True,
        capture_output=True,
        text=True,
    )
    output = completed.stdout
    features = re.search(r"cpu_features avx2=(\w+) fma=(\w+)", output)
    evaluation = re.search(
        r"nnue_eval positions (\d+) evaluations (\d+) time_ms ([\d.]+) "
        r"evals_per_sec ([\d.]+) us_per_eval ([\d.]+) checksum (-?\d+)",
        output,
    )
    update = re.search(
        r"nnue_update positions (\d+) push_pop_pairs (\d+) paired_eval_count (\d+) "
        r"total_time_ms ([\d.]+) pairs_per_sec ([\d.]+) "
        r"estimated_push_pop_us ([\d.]+) checksum (-?\d+)",
        output,
    )
    if not evaluation or not update:
        raise RuntimeError(output)
    return {
        "cpu_features": {
            "avx2": features.group(1) == "true" if features else None,
            "fma": features.group(2) == "true" if features else None,
        },
        "eval": {
            "positions": int(evaluation.group(1)),
            "evaluations": int(evaluation.group(2)),
            "time_ms": float(evaluation.group(3)),
            "evaluations_per_second": float(evaluation.group(4)),
            "microseconds_per_eval": float(evaluation.group(5)),
            "checksum": int(evaluation.group(6)),
        },
        "update": {
            "positions": int(update.group(1)),
            "push_pop_pairs": int(update.group(2)),
            "paired_eval_count": int(update.group(3)),
            "total_time_ms": float(update.group(4)),
            "pairs_per_second": float(update.group(5)),
            "estimated_push_pop_microseconds": float(update.group(6)),
            "checksum": int(update.group(7)),
        },
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("scalar", type=Path)
    parser.add_argument("simd", type=Path)
    parser.add_argument("dataset", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--eval-repeats", type=int, default=1024)
    parser.add_argument("--update-repeats", type=int, default=16)
    parser.add_argument("--runs", type=int, default=5)
    args = parser.parse_args()

    results = {"scalar": [], "simd": []}
    executables = {"scalar": args.scalar, "simd": args.simd}
    for index in range(args.runs):
        order = ("scalar", "simd") if index % 2 == 0 else ("simd", "scalar")
        for name in order:
            row = run(
                executables[name],
                args.dataset,
                args.eval_repeats,
                args.update_repeats,
            )
            results[name].append(row)
            print(
                f"run {index + 1}/{args.runs} {name}: "
                f"{row['eval']['evaluations_per_second']:.0f} eval/s, "
                f"{row['update']['estimated_push_pop_microseconds']:.3f} us push/pop",
                flush=True,
            )

    summary = {}
    for name, rows in results.items():
        summary[name] = {
            "median_evaluations_per_second": statistics.median(
                row["eval"]["evaluations_per_second"] for row in rows
            ),
            "median_microseconds_per_eval": statistics.median(
                row["eval"]["microseconds_per_eval"] for row in rows
            ),
            "median_estimated_push_pop_microseconds": statistics.median(
                row["update"]["estimated_push_pop_microseconds"] for row in rows
            ),
        }
    data = {
        "dataset": str(args.dataset),
        "eval_repeats": args.eval_repeats,
        "update_repeats": args.update_repeats,
        "runs": results,
        "summary": summary,
    }
    data["simd_eval_speedup_pct"] = (
        summary["simd"]["median_evaluations_per_second"]
        / summary["scalar"]["median_evaluations_per_second"]
        - 1
    ) * 100
    args.output.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    print(f"JSON: {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
