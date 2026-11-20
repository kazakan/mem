# winget manifests

The manifest set for the winget package `kazakan.mem`, in the multi-file format winget-pkgs requires: a **zip**
holding a **portable** `mem.exe`, exposed as the command `mem`, schema 1.28.0.

`kazakan.mem.yaml` (version), `kazakan.mem.installer.yaml` and `kazakan.mem.locale.en-US.yaml` (defaultLocale) are
written for v0.0.1; only the two `InstallerSha256` values and `ReleaseDate` are filled in when that release
exists. They are needed for the first submission only: later versions are generated from the copy in winget-pkgs
by the release workflow. The procedure is in [RELEASING.md](../../RELEASING.md#winget).
