# Releasing mem

Nothing here has been published yet. Version 0.0.1 is the first one; the steps below set up GitHub
releases and winget once.

## One-time setup (done)

1. **License**: MIT (`LICENSE`, `license = "MIT"` in `Cargo.toml`). The release archives include `LICENSE`.
2. **Repository**: https://github.com/kazakan/mem. CI (`.github/workflows/ci.yml`) runs fmt, clippy and the
   tests on Windows, Linux and macOS, plus an embed build on Linux.
3. **winget identifier**: `kazakan.mem`. On 2026-11-20 neither it nor a package with the command `mem` was in
   the winget source (`winget search --id kazakan.mem`, `winget search --command mem`).

## Every release

1. Update `version` in `Cargo.toml` and run `cargo build` so `Cargo.lock` follows.
2. Add a section to `CHANGELOG.md`.
3. `cargo fmt --check && cargo clippy --release --all-targets -- -D warnings && cargo test --release`
4. Commit and push `main`, then tag that commit and push the tag:
   ```sh
   git push origin main
   git tag v0.0.1
   git push origin v0.0.1
   ```
5. Only a tag of the form `vX.Y.Z` starts `.github/workflows/release.yml`; branch pushes, other tags
   (`v1`, `v0.1.0-rc1`, `test`) and manual runs do not. A failed run is retried with *Re-run jobs* on its
   page in the Actions tab. The workflow then:
   - checks that the tag equals the `Cargo.toml` version and that the tagged commit is already on `main`
     (a tag on an unmerged commit fails here, before anything is built),
   - generates `THIRD_PARTY_NOTICES.txt` with cargo-about (`about.toml`, `about.hbs`); a dependency under a
     license not accepted in `about.toml` stops the release here,
   - builds `mem-<version>-<target>.zip|tar.gz` for Windows x64/arm64 (static CRT), Linux x64 (musl,
     fully static), macOS x64/arm64, each with `LICENSE` and `THIRD_PARTY_NOTICES.txt`,
   - tries the optional `mem-embed-*` builds (llama.cpp inside; failures do not block),
   - publishes a GitHub release with all archives, `THIRD_PARTY_NOTICES.txt` and `SHA256SUMS` (0.x tags are
     marked pre-release),
   - runs the winget job (below).

## winget

**First version — by hand** (winget-pkgs only accepts new packages through a reviewed PR):

```powershell
winget install Microsoft.WingetCreate
wingetcreate new https://github.com/kazakan/mem/releases/download/v0.0.1/mem-0.0.1-x86_64-pc-windows-msvc.zip `
                 https://github.com/kazakan/mem/releases/download/v0.0.1/mem-0.0.1-aarch64-pc-windows-msvc.zip
```

Answer the prompts with the values in `packaging/winget/kazakan.mem*.yaml` (identifier `kazakan.mem`, installer
type zip, nested portable `mem.exe`, command alias `mem`, license MIT), let it validate, and submit. A Microsoft
reviewer and the validation bots check the PR; it usually takes a few days.

**Later versions — automatic**: fork `microsoft/winget-pkgs` to the `kazakan` account (the action pushes its
branch there), create a classic GitHub token with `public_repo` scope and save it as the repository secret
`WINGET_TOKEN`. The `winget` job then opens the update PR for each tag. Without the secret the job only prints
that it skipped.

Users then install with:

```powershell
winget install kazakan.mem
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
