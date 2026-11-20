# winget manifests (templates)

These three files are the shape of the manifest set winget-pkgs expects for mem: a **zip** holding a
**portable** `mem.exe`, exposed as the command `mem`. The package identifier is `kazakan.mem`; only the zero
hashes (and the version for later releases) are left to fill in once a release exists.

Easiest path (see ../../RELEASING.md):

```powershell
winget install Microsoft.WingetCreate
wingetcreate new https://github.com/kazakan/mem/releases/download/v0.0.1/mem-0.0.1-x86_64-pc-windows-msvc.zip `
                 https://github.com/kazakan/mem/releases/download/v0.0.1/mem-0.0.1-aarch64-pc-windows-msvc.zip
```

`wingetcreate` downloads the zips, computes the hashes and asks for the same fields as these templates.
Validate locally before submitting:

```powershell
winget validate --manifest <folder with the 3 files>
winget install --manifest <folder with the 3 files>   # needs: winget settings --enable LocalManifestFiles
```

After the first version is merged, the `winget` job in `.github/workflows/release.yml` opens the update
PRs for new tags (needs the `WINGET_TOKEN` secret).
