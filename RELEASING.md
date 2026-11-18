# Releasing mem

Nothing here has been published yet. Version 0.0.1 is the first one; the steps below set up GitHub
releases and winget once.

## One-time setup

1. **Pick a license** and add `LICENSE` (for example MIT, or `MIT OR Apache-2.0` with both files).
   Set `license = "..."` and `repository = "https://github.com/OWNER/mem"` in `Cargo.toml`.
   winget requires a license field; the release archives include `LICENSE` when it exists.
2. **Push the repository to GitHub** (`OWNER/mem`). CI (`.github/workflows/ci.yml`) runs fmt, clippy and
   the tests on Windows, Linux and macOS, plus an embed build on Linux.
3. **Choose the winget identifier**, e.g. `YourName.Mem`. Check that it, and the command alias `mem`,
   are not taken: `winget search mem`, and look in https://github.com/microsoft/winget-pkgs/tree/master/manifests.

## Every release

1. Update `version` in `Cargo.toml` and run `cargo build` so `Cargo.lock` follows.
2. Add a section to `CHANGELOG.md`.
3. `cargo fmt --check && cargo clippy --release --all-targets -- -D warnings && cargo test --release`
4. Commit, then tag and push:
   ```sh
   git tag v0.0.1
   git push origin v0.0.1
   ```
5. `.github/workflows/release.yml` then:
   - checks that the tag equals the `Cargo.toml` version,
   - builds `mem-<version>-<target>.zip|tar.gz` for Windows x64/arm64 (static CRT), Linux x64 (musl,
     fully static), macOS x64/arm64,
   - tries the optional `mem-embed-*` builds (llama.cpp inside; failures do not block),
   - publishes a GitHub release with all archives and `SHA256SUMS` (0.x tags are marked pre-release),
   - runs the winget job (below).

## winget

**First version — by hand** (winget-pkgs only accepts new packages through a reviewed PR):

```powershell
winget install Microsoft.WingetCreate
wingetcreate new https://github.com/OWNER/mem/releases/download/v0.0.1/mem-0.0.1-x86_64-pc-windows-msvc.zip `
                 https://github.com/OWNER/mem/releases/download/v0.0.1/mem-0.0.1-aarch64-pc-windows-msvc.zip
```

Answer the prompts with the values in `packaging/winget/*.yaml` (installer type zip, nested portable
`mem.exe`, command alias `mem`), let it validate, and submit. A Microsoft reviewer and the validation bots
check the PR; it usually takes a few days.

**Later versions — automatic**: create a classic GitHub token with `public_repo` scope, save it as the
repository secret `WINGET_TOKEN`, and set the repository variable `WINGET_ID` to the identifier. The
`winget` job then opens the update PR for each tag. Without them the job only prints that it skipped.

Users then install with:

```powershell
winget install YourName.Mem
mem where      # shows which store a call in this folder would use
```

## The embed build

The winget package is the keyword-only build (about 5 MB, starts in milliseconds). Embeddings need the
`mem-embed-*` archive from the release page plus the EmbeddingGemma model
(`embeddinggemma-300M-Q8_0.gguf`, downloaded separately under its own license, not redistributed here):
put it next to the store as `.mem/model.gguf` or set `MEM_MODEL`, then run `mem index` once.
Building it yourself: `cargo build --release --features embed` (needs CMake and a C++ compiler;
on Windows also set `LLAMA_STATIC_CRT=1`). See docs/ARCHITECTURE.md, "Embeddings".

## Not set up

- crates.io (`publish = false` in Cargo.toml; check the crate name before turning it on).
- Homebrew / Scoop / apt. The macOS and Linux archives are on the release page; a Scoop manifest would be a
  small JSON file pointing at the same Windows zip if wanted later.
- Code signing. Unsigned exes can trigger SmartScreen on first run; winget itself does not require signing.
