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

Follows Microsoft's "Submit your manifest to the repository" guide
(https://learn.microsoft.com/en-us/windows/package-manager/package/repository). The templates in
`packaging/winget/` are a multi-file manifest (version, installer, defaultLocale en-US) on schema 1.28.0, the
newest in winget-pkgs on 2026-11-20; they pass `winget validate`.

**Once**

- Fork `microsoft/winget-pkgs` to the `kazakan` account: `gh repo fork microsoft/winget-pkgs --clone=false`.
  The PRs come from this fork, and the automatic updates below push their branches to it.
- Git 2.37 or newer (sparse checkout).
- On the first PR the Microsoft CLA bot asks you to sign the Contributor License Agreement; the PR cannot be
  merged before that, even when validation passed.

**First version — by hand** (winget-pkgs only accepts new packages through a reviewed PR):

1. Release `v0.0.1` as above and wait until the release workflow has published it.
2. Check out the fork (sparse) next to `mem` and copy the templates into the package's folder:
   ```powershell
   git clone --filter=blob:none --no-checkout https://github.com/kazakan/winget-pkgs.git
   cd winget-pkgs
   git sparse-checkout set manifests/k/kazakan
   git checkout
   git switch -c kazakan.mem-0.0.1
   $m = 'manifests\k\kazakan\mem\0.0.1'
   New-Item -ItemType Directory -Force $m | Out-Null
   Copy-Item ..\mem\packaging\winget\kazakan.mem*.yaml $m
   ```
3. In `$m\kazakan.mem.installer.yaml` replace the two zero `InstallerSha256` values with the x64 and arm64 zip
   lines of the release's checksums, and set `ReleaseDate`. Version and URLs already say 0.0.1.
   ```powershell
   gh release download v0.0.1 -R kazakan/mem -p SHA256SUMS -O -
   winget validate --manifest $m     # must say "Manifest validation succeeded." with no warnings
   ```
4. Install from the local manifest and check it (the first command once, in an administrator shell):
   ```powershell
   winget settings --enable LocalManifestFiles
   winget install --manifest $m
   mem --version             # mem 0.0.1, in a new terminal
   winget uninstall kazakan.mem
   ```
   For a clean machine, run `.\Tools\SandboxTest.ps1 $m` in this checkout instead (Windows Sandbox).
5. Open the PR — one package version per PR, manifest files only:
   ```powershell
   git add $m
   git commit -m "New package: kazakan.mem version 0.0.1"
   git push -u origin kazakan.mem-0.0.1
   gh pr create --repo microsoft/winget-pkgs --base master --head kazakan:kazakan.mem-0.0.1 --web
   ```
   `--web` opens the PR form with winget-pkgs' checklist; fill it in and create the PR.
6. Watch the PR. The validation pipeline installs the package and scans it; then a moderator reviews it. Labels:
   `Azure-Pipeline-Passed` (waiting for approval), `Needs-Author-Feedback` (answer within 10 days or the bot
   closes the PR), `Validation-Completed` (will be merged), `Blocking-Issue` (cannot be approved). On a failure
   open the "Details" link of the failed check. mem.exe is unsigned, so an antivirus false positive is possible
   (`Binary-Validation-Error`); report it at https://www.microsoft.com/wdsi/filesubmission.

**Later versions — automatic**: create a classic GitHub token with `public_repo` scope (fine-grained tokens do not
work) and save it as the repository secret `WINGET_TOKEN`. For each release tag the `winget` job
(winget-releaser, pinned to a commit of v2) writes the new version's manifests from the release and opens the
update PR from the fork. Without the secret the job only prints that it skipped. If it fails, update by hand with
winget-create, which starts from the manifest already in winget-pkgs and computes the hashes:
`wingetcreate update kazakan.mem --version X.Y.Z --urls <x64 zip URL> <arm64 zip URL> --submit`.

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
