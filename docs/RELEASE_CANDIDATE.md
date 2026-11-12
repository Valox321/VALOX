# Valox local release candidate

Built on 2026-11-12 from the workspace state in `C:\Users\Valox\Pictures\Valox Engine`.

| Field | Value |
|---|---|
| Engine | Valox 0.5.0 |
| Platform | Windows x86_64, MSVC |
| Rust compiler | `rustc 1.99.1 (58cf8f9c3 2026-10-13)` |
| Cargo | `cargo 1.99.1 (5f94df478 2026-08-27)` |
| Build command | `cargo build --release` |
| Executable | `build\valox.exe` |
| Executable SHA-256 | `87EF90A88B2A629A357DE7ABB460216E5733B7130E7AC7BCE96F1C94E03951BF` |
| NNUE | Reckless v60, embedded at compile time |
| NNUE SHA-256 | `7F587DFB1FE5D74D53909328AFA6FD51650C8C7F45907602DB7FBB1E52948C61` |
| Source snapshot | `build\valox-source-20261112.zip` (49 files: engine source, build inputs, model, trainer source, scripts, documentation, and third-party license texts; generated datasets/checkpoints and match binaries excluded) |
| Source snapshot SHA-256 | See `build\valox-source-20261112.zip.sha256` |

This is a local candidate record, not a public release. The source snapshot records the build inputs but is not a Git commit; the workspace has no repository metadata or remote. The build passes locally on Windows; Linux CI has not run.

The current source snapshot was freshly extracted and rebuilt on Windows. Formatting, all 13 Rust tests, Clippy with warnings denied, the release build, the UCI standard/Chess960/stop smoke check, and 10 UCI stress searches passed. The extracted executable hash was `17979C09F92E17A1F5C135227A1CD729387C302E8A82FAC564A22EAD427DE106`; it has the same 63,667,200-byte size as the pinned executable above, with 24 differing PE metadata bytes from linking at a different path. This verifies that the snapshot builds and runs, but the Windows executable is not bit-for-bit reproducible across build paths. Linux-target type checking and Clippy also pass locally; GitHub Actions has not run.

Before distributing or submitting this binary, choose and document Valox's source license, include dependency notices, and resolve redistribution rights for the embedded Reckless v60 weights. Do not treat this candidate as cleared for redistribution until those items are resolved.
