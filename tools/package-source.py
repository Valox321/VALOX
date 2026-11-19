"""Create a source-focused, reproducible archive for the local engine candidate."""

from __future__ import annotations

import hashlib
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "build" / "valox-source-20261119.zip"
INCLUDE_FILES = {
    ".gitignore", "Cargo.toml", "Cargo.lock", "README.md", "STRENGTH_ROADMAP.md", "build.rs",
    "THIRD_PARTY_NOTICES.md", "LICENSE-MIT", "LICENSE-APACHE", "run-browser.ps1", "stop-browser.ps1",
    "tools/engine-match/openings.pgn", "tools/package-source.py",
    "tools/build-release.py", "tools/make-synthetic-net.py",
    "tools/linux-qemu-smoke.py",
    "tools/training/.gitignore", "tools/training/Cargo.toml", "tools/training/Cargo.lock",
    "tools/training/README.md", "tools/training/EXPERIMENT-2026-11.md",
    "tools/training/generate_data.py", "tools/training/prepare_data.ps1", "tools/training/train.ps1",
}
INCLUDE_DIRS = (".cargo", ".github", "src", "web", "networks", "docs", "licenses", "tools/engine-match", "tools/training/src")
EXCLUDED_PARTS = {"target", "results", "stockfish", "cutechess", "__pycache__"}
EXCLUDED_FILES = {"valox.wasm", "stockfish.zip", "cutechess.zip"}
# Model weights are third-party data with unresolved redistribution terms.
EXCLUDED_SUFFIXES = {".nnue"}


def source_files() -> list[Path]:
    files = [ROOT / name for name in INCLUDE_FILES]
    for directory in INCLUDE_DIRS:
        base = ROOT / directory
        if not base.exists():
            continue
        for path in base.rglob("*"):
            if (
                not path.is_file()
                or EXCLUDED_PARTS.intersection(path.parts)
                or path.name in EXCLUDED_FILES
                or path.suffix in EXCLUDED_SUFFIXES
            ):
                continue
            files.append(path)
    return sorted(set(files), key=lambda path: path.relative_to(ROOT).as_posix())


def main() -> None:
    OUTPUT.parent.mkdir(exist_ok=True)
    with ZipFile(OUTPUT, "w", ZIP_DEFLATED, compresslevel=9) as archive:
        for path in source_files():
            relative = path.relative_to(ROOT).as_posix()
            entry = ZipInfo(relative, date_time=(2026, 11, 19, 0, 0, 0))
            entry.compress_type = ZIP_DEFLATED
            entry.external_attr = 0o100644 << 16
            archive.writestr(entry, path.read_bytes(), compress_type=ZIP_DEFLATED, compresslevel=9)
    digest = hashlib.sha256(OUTPUT.read_bytes()).hexdigest()
    OUTPUT.with_suffix(OUTPUT.suffix + ".sha256").write_text(
        f"{digest}  {OUTPUT.name}\n", encoding="ascii"
    )
    print(f"created {OUTPUT.relative_to(ROOT)} ({len(source_files())} files, {OUTPUT.stat().st_size} bytes)")
    print(f"sha256 {digest}")


if __name__ == "__main__":
    main()
