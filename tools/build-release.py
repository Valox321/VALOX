#!/usr/bin/env python3
"""Build Valox release binaries and create tournament-ready archives.

The Reckless v60 network is not redistributable until its rights are clarified.
This script therefore builds a weight-free binary by default. The binary can
load a compatible model at runtime through the UCI ``EvalFile`` option, so the
weights never have to be part of the archive. An embedded build is still
possible with ``--network`` and requires ``--allow-network-redistribution``.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import os
import re
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MODEL_BYTES = 63_266_880
PLATFORMS = {
    "windows": "windows-x86_64",
    "linux-musl": "linux-x86_64-musl",
}
RELEASE_FILES = [
    "README.md",
    "LICENSE-MIT",
    "LICENSE-APACHE",
    "THIRD_PARTY_NOTICES.md",
]


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def cargo_version() -> str:
    text = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    match = re.search(r'^version\s*=\s*"([^"]+)"', text, re.MULTILINE)
    if not match:
        raise SystemExit("could not read the package version from Cargo.toml")
    return match.group(1)


def command_output(*command: str) -> str:
    return subprocess.run(
        command, cwd=ROOT, check=True, capture_output=True, text=True
    ).stdout.strip()


def host_triple() -> str:
    for line in command_output("rustc", "-vV").splitlines():
        if line.startswith("host: "):
            return line.removeprefix("host: ")
    raise SystemExit("could not determine the Rust host triple")


def find_rust_lld() -> Path:
    sysroot = Path(command_output("rustc", "--print", "sysroot"))
    name = "rust-lld.exe" if os.name == "nt" else "rust-lld"
    candidates = list(sysroot.glob(f"lib/rustlib/*/bin/{name}"))
    if not candidates:
        raise SystemExit(
            "rust-lld not found in the Rust sysroot; install the Rust toolchain "
            "with its self-contained linker"
        )
    return candidates[0]


def ensure_musl_target() -> None:
    installed = command_output("rustup", "target", "list", "--installed").split()
    if "x86_64-unknown-linux-musl" not in installed:
        print("installing the x86_64-unknown-linux-musl target")
        subprocess.run(
            ["rustup", "target", "add", "x86_64-unknown-linux-musl"],
            cwd=ROOT,
            check=True,
        )


def build(platform: str, network: Path | None) -> Path:
    env = os.environ.copy()
    if network is None:
        # Never let a default local network slip into a release by accident.
        env["VALOX_NO_NNUE"] = "1"
        env.pop("VALOX_NNUE_PATH", None)
    else:
        env["VALOX_NNUE_PATH"] = str(network)
        env.pop("VALOX_NO_NNUE", None)

    if platform == "windows":
        if os.name != "nt":
            raise SystemExit("Windows builds require a Windows host")
        subprocess.run(["cargo", "build", "--release", "--bin", "valox"], cwd=ROOT, env=env, check=True)
        return ROOT / "target" / "release" / "valox.exe"

    ensure_musl_target()
    if os.name == "nt":
        env["CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER"] = str(find_rust_lld())
    else:
        env.setdefault("CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER", "musl-gcc")
        if shutil.which(env["CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER"]) is None:
            raise SystemExit(
                "musl-gcc not found; install musl-tools (Debian/Ubuntu: "
                "sudo apt-get install musl-tools)"
            )
    subprocess.run(
        [
            "cargo",
            "build",
            "--release",
            "--bin",
            "valox",
            "--target",
            "x86_64-unknown-linux-musl",
        ],
        cwd=ROOT,
        env=env,
        check=True,
    )
    return ROOT / "target" / "x86_64-unknown-linux-musl" / "release" / "valox"


def write_checksums(directory: Path) -> None:
    lines = []
    for path in sorted(directory.rglob("*")):
        if path.is_file() and path.name != "SHA256SUMS":
            lines.append(f"{sha256_file(path)}  {path.relative_to(directory).as_posix()}")
    (directory / "SHA256SUMS").write_text("\n".join(lines) + "\n", encoding="utf-8")


def package(
    platform: str,
    binary: Path,
    version: str,
    network: Path | None,
    output: Path,
) -> Path:
    folder_name = f"valox-{version}-{PLATFORMS[platform]}"
    package_dir = output / folder_name
    if package_dir.exists():
        shutil.rmtree(package_dir)
    package_dir.mkdir(parents=True)

    binary_name = "valox.exe" if platform == "windows" else "valox"
    shutil.copy2(binary, package_dir / binary_name)
    for name in RELEASE_FILES:
        source = ROOT / name
        if source.is_file():
            shutil.copy2(source, package_dir / name)
    licenses = ROOT / "licenses"
    if licenses.is_dir():
        shutil.copytree(licenses, package_dir / "licenses")

    info = [
        f"Valox {version}",
        f"Platform: {PLATFORMS[platform]}",
        f"Built: {dt.datetime.now(dt.timezone.utc).isoformat(timespec='seconds')}",
        f"Rust: {command_output('rustc', '--version')}",
        f"Cargo: {command_output('cargo', '--version')}",
    ]
    try:
        info.append(f"Git commit: {command_output('git', 'rev-parse', 'HEAD')}")
    except (subprocess.CalledProcessError, FileNotFoundError):
        info.append("Git commit: unavailable")
    if network is None:
        info.append("Evaluation: classical by default; no NNUE weights embedded")
        info.append("External NNUE: set the UCI option EvalFile to a compatible network file")
    else:
        info.append(f"Evaluation: embedded NNUE {network.name}")
        info.append(f"NNUE SHA-256: {sha256_file(network)}")
        info.append("Network redistribution permission: confirmed by the release caller")
    (package_dir / "BUILD-INFO.txt").write_text("\n".join(info) + "\n", encoding="utf-8")

    write_checksums(package_dir)
    archive = output / f"{folder_name}.zip"
    if archive.exists():
        archive.unlink()
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as bundle:
        for path in sorted(package_dir.rglob("*")):
            if path.is_file():
                bundle.write(path, path.relative_to(package_dir.parent).as_posix())
    archive_hash = sha256_file(archive)
    (archive.with_suffix(".zip.sha256")).write_text(
        f"{archive_hash}  {archive.name}\n", encoding="utf-8"
    )
    print(f"created {archive} ({archive.stat().st_size} bytes)")
    print(f"sha256 {archive_hash}")
    return archive


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", default=cargo_version())
    parser.add_argument(
        "--target",
        action="append",
        choices=sorted(PLATFORMS),
        dest="targets",
        help="repeat to build several platforms (default: host-appropriate)",
    )
    parser.add_argument("--network", type=Path, help="NNUE file embedded into the build")
    parser.add_argument(
        "--allow-network-redistribution",
        action="store_true",
        help="confirm written permission to redistribute the network",
    )
    parser.add_argument(
        "--build-only", action="store_true", help="build but do not create archives"
    )
    parser.add_argument(
        "--output", type=Path, default=ROOT / "build" / "release"
    )
    args = parser.parse_args()

    network = args.network.resolve() if args.network else None
    if network is not None:
        if not network.is_file():
            raise SystemExit(f"network not found: {network}")
        if network.stat().st_size != MODEL_BYTES:
            raise SystemExit(
                f"network size {network.stat().st_size} does not match the v60 "
                f"layout ({MODEL_BYTES} bytes)"
            )
        if not args.build_only and not args.allow_network_redistribution:
            raise SystemExit(
                "refusing to package a binary with the Reckless v60 network: "
                "its redistribution rights are not confirmed. Pass "
                "--allow-network-redistribution only after written permission."
            )

    targets = args.targets or (["windows", "linux-musl"] if os.name == "nt" else ["linux-musl"])
    args.output.mkdir(parents=True, exist_ok=True)
    for platform in targets:
        print(f"building Valox {args.version} for {PLATFORMS[platform]}")
        binary = build(platform, network)
        print(f"binary: {binary} ({binary.stat().st_size} bytes)")
        if args.build_only:
            continue
        package(platform, binary, args.version, network, args.output)
    return 0


if __name__ == "__main__":
    sys.exit(main())
