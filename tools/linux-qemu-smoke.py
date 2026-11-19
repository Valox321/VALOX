#!/usr/bin/env python3
"""Boot a static Valox binary in a minimal Linux initramfs under QEMU TCG.

This is a runtime check, not a performance benchmark: QEMU instruction
emulation is much slower than native Linux. It needs a Linux kernel image, a
static BusyBox binary, and a QEMU system emulator. It never downloads or
redistributes the NNUE weights; pass a Valox binary that already contains them.

Example:
    python tools/linux-qemu-smoke.py --valox build/valox-linux-x86_64-musl \
        --kernel vmlinuz-virt --busybox busybox \
        --qemu C:/qemu/qemu-system-x86_64.exe --qemu-share C:/qemu/share \
        --bench 10 --expect-nodes 389894
"""

from __future__ import annotations

import argparse
import gzip
import subprocess
import sys
import tempfile
from pathlib import Path

DIRS = [
    (".", 0o040755, 2),
    ("bin", 0o040755, 2),
    ("dev", 0o040755, 2),
    ("proc", 0o040755, 2),
    ("sys", 0o040755, 2),
]
LINKS = ["sh", "mount", "poweroff", "uname"]


def cpio_record(name: str, data: bytes, mode: int, inode: int, nlink: int = 1) -> bytes:
    name_bytes = name.encode("utf-8") + b"\0"
    fields = [
        inode,
        mode,
        0,
        0,
        nlink,
        0,
        len(data),
        0,
        0,
        0,
        0,
        len(name_bytes),
        0,
    ]
    header = b"070701" + b"".join(f"{value:08x}".encode("ascii") for value in fields)
    record = header + name_bytes
    record += b"\0" * (-len(record) % 4)
    record += data
    record += b"\0" * (-len(data) % 4)
    return record


def make_initramfs(root: Path, valox: Path, busybox: Path, bench: int) -> Path:
    init = f"""#!/bin/sh
export PATH=/bin
mount -t proc proc /proc
mount -t sysfs sysfs /sys
mount -t devtmpfs devtmpfs /dev
echo VALOX_LINUX_START
uname -a
/valox --bench {bench}
echo VALOX_BENCH_EXIT=$?
printf 'uci\\nisready\\nposition startpos\\ngo depth 8\\nquit\\n' | /valox
echo VALOX_UCI_EXIT=$?
poweroff -f
""".encode("utf-8")
    records = []
    inode = 1
    for name, mode, nlink in DIRS:
        records.append(cpio_record(name, b"", mode, inode, nlink))
        inode += 1
    payloads = [
        ("bin/busybox", busybox.read_bytes(), 0o100755),
        *[(f"bin/{name}", b"busybox", 0o120777) for name in LINKS],
        ("init", init, 0o100755),
        ("valox", valox.read_bytes(), 0o100755),
    ]
    for name, data, mode in payloads:
        records.append(cpio_record(name, data, mode, inode))
        inode += 1
    records.append(cpio_record("TRAILER!!!", b"", 0, inode))
    output = root / "initramfs.cpio.gz"
    with gzip.open(output, "wb", compresslevel=1) as handle:
        handle.write(b"".join(records))
    return output


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--valox", type=Path, required=True)
    parser.add_argument("--kernel", type=Path, required=True)
    parser.add_argument("--busybox", type=Path, required=True)
    parser.add_argument("--qemu", default="qemu-system-x86_64")
    parser.add_argument("--qemu-share", type=Path)
    parser.add_argument("--bench", type=int, default=10)
    parser.add_argument("--memory", type=int, default=2048)
    parser.add_argument("--timeout", type=int, default=900)
    parser.add_argument(
        "--expect-nodes",
        type=int,
        help="fail unless the bench reports exactly this node count",
    )
    args = parser.parse_args()

    for path in (args.valox, args.kernel, args.busybox):
        if not path.is_file():
            raise SystemExit(f"missing file: {path}")

    with tempfile.TemporaryDirectory(prefix="valox-linux-") as directory:
        root = Path(directory)
        initramfs = make_initramfs(root, args.valox, args.busybox, args.bench)
        command = [
            args.qemu,
            "-machine",
            "q35",
            "-cpu",
            "max",
            "-m",
            str(args.memory),
            "-kernel",
            str(args.kernel),
            "-initrd",
            str(initramfs),
            "-append",
            "console=ttyS0 rdinit=/init panic=-1",
            "-nographic",
            "-no-reboot",
        ]
        if args.qemu_share:
            command[1:1] = ["-L", str(args.qemu_share)]
        try:
            result = subprocess.run(
                command, capture_output=True, text=True, timeout=args.timeout
            )
        except subprocess.TimeoutExpired as error:
            output = (error.stdout or "") + (error.stderr or "")
            print(output)
            raise SystemExit(f"QEMU timed out after {args.timeout} seconds") from error

        output = result.stdout + result.stderr
        print(output)
        failures = []
        if "VALOX_LINUX_START" not in output:
            failures.append("the init script did not start")
        if "VALOX_BENCH_EXIT=0" not in output:
            failures.append("benchmark did not exit cleanly")
        if "VALOX_UCI_EXIT=0" not in output:
            failures.append("UCI session did not exit cleanly")
        if "id name Valox" not in output:
            failures.append("UCI identification was not printed")
        if args.expect_nodes is not None and (
            f"bench depth {args.bench} nodes {args.expect_nodes}" not in output
        ):
            failures.append(f"expected {args.expect_nodes} benchmark nodes")
        if failures:
            for failure in failures:
                print(f"FAIL: {failure}", file=sys.stderr)
            return 1
        print(f"Linux QEMU smoke passed: bench depth {args.bench}")
        return 0


if __name__ == "__main__":
    sys.exit(main())
