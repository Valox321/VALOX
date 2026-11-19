#!/usr/bin/env python3
"""Create a deterministic synthetic NNUE file with the Reckless v60 layout.

The real Reckless weights are not redistributable until their rights are
clarified.  CI and Linux smoke tests still need to exercise the NNUE code path,
so this script writes a file of exactly the same size and layout with small
deterministic values.  It is not a strength model; it only makes incremental
and full-refresh inference comparable on platforms where the real weights are
not available.
"""

from __future__ import annotations

import argparse
import random
import struct
import sys
from pathlib import Path

FEATURES = 768
THREAT_FEATURES = 66_864
INPUT_BUCKETS = 10
OUTPUT_BUCKETS = 8
L2 = 16
L3 = 32

THREAT_BYTES = THREAT_FEATURES * FEATURES
PIECE_BYTES = INPUT_BUCKETS * FEATURES * FEATURES * 2
BIAS_BYTES = FEATURES * 2
L1_BYTES = OUTPUT_BUCKETS * L2 * FEATURES
L1_BIAS_BYTES = OUTPUT_BUCKETS * L2 * 4
L2_WEIGHT_BYTES = OUTPUT_BUCKETS * L2 * L3 * 4
L2_BIAS_BYTES = OUTPUT_BUCKETS * L3 * 4
L3_WEIGHT_BYTES = OUTPUT_BUCKETS * L3 * 4
# The final layer has one bias per output bucket (8 x f32) followed by padding
# up to the model's 64-byte alignment.
L3_BIAS_BYTES = OUTPUT_BUCKETS * 4
FINAL_PADDING_BYTES = 64 - L3_BIAS_BYTES

MODEL_BYTES = (
    THREAT_BYTES
    + PIECE_BYTES
    + BIAS_BYTES
    + L1_BYTES
    + L1_BIAS_BYTES
    + L2_WEIGHT_BYTES
    + L2_BIAS_BYTES
    + L3_WEIGHT_BYTES
    + L3_BIAS_BYTES
    + FINAL_PADDING_BYTES
)

# Map every byte to a signed i8 in [-4, 3].  These weights are small enough
# that the i16 accumulator of a legal position cannot overflow.
I8_TABLE = bytes(124 + (value & 7) for value in range(256))


def i8_block(rng: random.Random, size: int) -> bytes:
    return rng.randbytes(size).translate(I8_TABLE)


def small_floats(count: int) -> bytes:
    return b"".join(
        struct.pack("<f", ((index * 37 % 101) - 50) / 5000.0) for index in range(count)
    )


def write_model(path: Path, seed: int) -> None:
    rng = random.Random(seed)
    with path.open("wb") as model:
        model.write(i8_block(rng, THREAT_BYTES))
        model.write(i8_block(rng, PIECE_BYTES))
        model.write(b"\0" * BIAS_BYTES)
        model.write(i8_block(rng, L1_BYTES))
        model.write(small_floats(OUTPUT_BUCKETS * L2))
        model.write(small_floats(OUTPUT_BUCKETS * L2 * L3))
        model.write(small_floats(OUTPUT_BUCKETS * L3))
        model.write(small_floats(OUTPUT_BUCKETS * L3))
        model.write(small_floats(OUTPUT_BUCKETS))
        model.write(b"\0" * FINAL_PADDING_BYTES)
    actual = path.stat().st_size
    if actual != MODEL_BYTES:
        raise SystemExit(f"wrote {actual} bytes, expected {MODEL_BYTES}")
    if path.stat().st_size == 0:
        raise SystemExit("synthetic model is empty")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="destination .nnue file")
    parser.add_argument("--seed", type=int, default=20261119)
    parser.add_argument(
        "--reference",
        type=Path,
        help="optional real model whose size must match",
    )
    args = parser.parse_args()

    if args.reference and args.reference.stat().st_size != MODEL_BYTES:
        raise SystemExit(
            f"reference size {args.reference.stat().st_size} does not match "
            f"expected {MODEL_BYTES}"
        )

    args.output.parent.mkdir(parents=True, exist_ok=True)
    write_model(args.output, args.seed)
    print(
        f"wrote synthetic NNUE {args.output} ({MODEL_BYTES} bytes, seed {args.seed})"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
