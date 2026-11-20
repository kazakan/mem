# winget manifests

The manifest set for the winget package `kazakan.mem`, in the multi-file format winget-pkgs requires: a **zip**
holding a **portable** `mem.exe`, exposed as the command `mem`, schema 1.28.0.

- `kazakan.mem.yaml` (version), `kazakan.mem.installer.yaml`, `kazakan.mem.locale.en-US.yaml` (defaultLocale):
  templates for v0.0.1 with zero hashes.
- `prepare.ps1 -Version X.Y.Z`: fills them from the GitHub release (installer URLs, SHA-256 from the release's
  `SHA256SUMS`, release date) into `target/winget/manifests/k/kazakan/mem/X.Y.Z/` and runs `winget validate`.

The whole procedure (local install test, PR to microsoft/winget-pkgs, automatic updates) is in
[RELEASING.md](../../RELEASING.md#winget). Change the templates here, not the generated copies; fields that are
the same for every version (description, tags, license, installer type) live only here.
